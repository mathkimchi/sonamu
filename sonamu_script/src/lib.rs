//! Luau scripting (see `.claude/COMMANDS.md`): one Luau state on its own thread, shared by every `eval`.
//!
//! Commands block the script thread until their world answers; worlds never wait on it, so this can't deadlock.

use mlua::{Lua, MetaMethod, MultiValue, UserData, UserDataMethods, Value as LuaValue, VmState};
use sonamu_command::{self as command, CommandResult, NODE_COMMANDS, NodeUid, Value};
use std::{
    cell::RefCell,
    fmt::Write as _,
    sync::{
        Arc, LazyLock, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

/// What an `eval` gives, in order
#[derive(Debug)]
pub enum Output {
    /// From `print`
    Print(String),
    /// Its return values, last
    Done(Result<Vec<Value>, String>),
}

/// Cancels its eval (eg Ctrl+C in the hub)
pub struct EvalHandle(Arc<AtomicBool>);
impl EvalHandle {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

enum Job {
    Eval {
        code: String,
        output: mpsc::Sender<Output>,
        /// Called after each output (eg wakes the hub's thread)
        notify: Box<dyn Fn() + Send>,
        cancelled: Arc<AtomicBool>,
    },
    Complete {
        text: String,
        reply: mpsc::Sender<Vec<String>>,
        notify: Box<dyn Fn() + Send>,
    },
}

fn send_job(job: Job) {
    static JOBS: LazyLock<Mutex<mpsc::Sender<Job>>> = LazyLock::new(|| {
        let (jobs, received) = mpsc::channel();
        std::thread::Builder::new()
            .name("script".to_string())
            .spawn(move || run(&received))
            .unwrap();
        Mutex::new(jobs)
    });
    // the thread runs as long as the process
    let _ = JOBS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .send(job);
}

/// Runs `code` on the script thread, after any evals before it. `notify` is called after each output.
pub fn eval(
    code: String,
    notify: impl Fn() + Send + 'static,
) -> (EvalHandle, mpsc::Receiver<Output>) {
    let (output, outputs) = mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    send_job(Job::Eval {
        code,
        output,
        notify: Box::new(notify),
        cancelled: Arc::clone(&cancelled),
    });
    (EvalHandle(cancelled), outputs)
}

/// What could follow the end of `text` (the word it ends in), from the live state, after any evals before it.
/// `notify` is called once it is ready.
pub fn complete(text: String, notify: impl Fn() + Send + 'static) -> mpsc::Receiver<Vec<String>> {
    let (reply, replied) = mpsc::channel();
    send_job(Job::Complete {
        text,
        reply,
        notify: Box::new(notify),
    });
    replied
}

thread_local! {
    /// The eval running on the script thread
    static CURRENT: RefCell<Option<Current>> = const { RefCell::new(None) };
}

struct Current {
    output: mpsc::Sender<Output>,
    notify: Box<dyn Fn() + Send>,
    cancelled: Arc<AtomicBool>,
    /// `sonamu.reset()`: a fresh state after this eval
    reset: bool,
}

fn cancelled() -> bool {
    CURRENT.with_borrow(|current| {
        current
            .as_ref()
            .is_some_and(|current| current.cancelled.load(Ordering::Relaxed))
    })
}

fn send(output: Output) {
    CURRENT.with_borrow(|current| {
        if let Some(current) = current {
            let _ = current.output.send(output);
            (current.notify)();
        }
    });
}

fn run(jobs: &mpsc::Receiver<Job>) {
    let mut lua = new_lua().expect("the script state");
    for job in jobs {
        let (code, output, notify, cancelled) = match job {
            Job::Eval {
                code,
                output,
                notify,
                cancelled,
            } => (code, output, notify, cancelled),
            Job::Complete {
                text,
                reply,
                notify,
            } => {
                let _ = reply.send(completions(&lua, &text));
                notify();
                continue;
            }
        };
        CURRENT.set(Some(Current {
            output,
            notify,
            cancelled,
            reset: false,
        }));
        let result = run_code(&lua, &code);
        send(Output::Done(result));
        let reset = CURRENT.take().is_some_and(|current| current.reset);
        if reset {
            lua = new_lua().expect("the script state");
        }
    }
}

/// As an expression if it is one (like a REPL), else as statements
fn run_code(lua: &Lua, code: &str) -> Result<Vec<Value>, String> {
    let function = lua
        .load(format!("return {code}"))
        .set_name("=input")
        .into_function()
        .or_else(|_| lua.load(code).set_name("=input").into_function())
        .map_err(|err| err.to_string())?;
    let values = function
        .call::<MultiValue>(())
        .map_err(|err| err.to_string())?;
    Ok(values.into_iter().map(|value| returned(&value)).collect())
}

fn new_lua() -> mlua::Result<Lua> {
    let lua = Lua::new();
    lua.set_interrupt(|_| {
        if cancelled() {
            Err(mlua::Error::runtime("cancelled"))
        } else {
            Ok(VmState::Continue)
        }
    });
    let globals = lua.globals();
    globals.set(
        "print",
        lua.create_function(|_, values: MultiValue| {
            let line = values
                .iter()
                .map(|value| value.to_string().unwrap_or_else(|_| format!("{value:?}")))
                .collect::<Vec<_>>()
                .join("\t");
            send(Output::Print(line));
            Ok(())
        })?,
    )?;

    let context = lua.create_table()?;
    context.set(
        "focus",
        lua.create_function(|lua, ()| {
            to_lua(
                lua,
                command::focus(&cancelled).map_err(mlua::Error::runtime)?,
            )
        })?,
    )?;
    context.set("root", tree_function(&lua, "root")?)?;
    globals.set("context", context)?;

    let clipboard = lua.create_table()?;
    clipboard.set(
        "copy",
        lua.create_function(|_, text: String| Ok(sonamu_common::clipboard::copy(text)))?,
    )?;
    clipboard.set(
        "paste",
        lua.create_function(|_, ()| Ok(sonamu_common::clipboard::paste()))?,
    )?;
    globals.set("clipboard", clipboard)?;

    let process = lua.create_table()?;
    process.set("spawn", tree_function(&lua, "spawn")?)?;
    globals.set("process", process)?;

    let sonamu = lua.create_table()?;
    for name in ["open", "quit", "reload_settings"] {
        sonamu.set(name, tree_function(&lua, name)?)?;
    }
    let help_function = lua.create_function(|_, ()| Ok(help()))?;
    sonamu.set("help", help_function.clone())?;
    globals.set("help", help_function)?;
    sonamu.set(
        "reset",
        lua.create_function(|_, ()| {
            CURRENT.with_borrow_mut(|current| {
                if let Some(current) = current {
                    current.reset = true;
                }
            });
            Ok(())
        })?,
    )?;
    globals.set("sonamu", sonamu)?;

    // builtins read-only; what evals define goes in a table of its own
    lua.sandbox(true)?;
    Ok(lua)
}

const HELP: &str = "\
help() / sonamu.help()         this
context.focus(): Node          the innermost focused node
context.root(): Node           the root node
process.spawn(program, {args}?)
clipboard.copy(text)           Sonamu's clipboard, for wl clients too
clipboard.paste(): string?
sonamu.open(applet, {args}?)   like `sonamu-msg open`
sonamu.reload_settings()
sonamu.quit()
sonamu.reset()                 a fresh Luau state
Node: close() focus() add_child(applet, {args}?): Node swap_sibling(offset) toggle_half() wrap_project()
      parent(): Node? children(): {Node} type(): string title(): string cast(type): Node?
      any other method is a command of its applet, eg hub:clear()
Super+Esc: the buffer line (Esc leaves). There and in the hub: Tab completes, Up/Down recalls, PageUp/PageDown scrolls, Esc clears, Ctrl+V pastes, Ctrl+C cancels.
Super+? lists every keybind.";

/// `HELP`, then each applet type's commands
fn help() -> String {
    let mut help = HELP.to_string();
    for (applet_type, signatures) in command::all_signatures() {
        if signatures.is_empty() {
            continue;
        }
        let _ = write!(help, "\n{applet_type}:");
        for signature in signatures {
            let _ = write!(
                help,
                "\n  {}({}): {}  {}",
                signature.name, signature.params, signature.returns, signature.doc
            );
        }
    }
    help
}

/// A command of the root's tree
fn tree_function(lua: &Lua, name: &'static str) -> mlua::Result<mlua::Function> {
    lua.create_function(move |lua, args: MultiValue| call(lua, None, name, &args))
}

fn call(lua: &Lua, uid: Option<NodeUid>, name: &str, args: &MultiValue) -> mlua::Result<LuaValue> {
    let args = args.iter().map(from_lua).collect::<mlua::Result<_>>()?;
    let result: CommandResult = command::call(uid, name, args, &cancelled);
    to_lua(lua, result.map_err(mlua::Error::runtime)?)
}

/// A handle to a node in any world
#[derive(Clone, Copy)]
struct Node(NodeUid);

impl UserData for Node {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        for name in NODE_COMMANDS {
            methods.add_method(name, move |lua, node, args: MultiValue| {
                call(lua, Some(node.0), name, &args)
            });
        }
        // itself if its applet is of that type
        methods.add_method("cast", |lua, node, applet_type: String| {
            let actual = call(lua, Some(node.0), "type", &MultiValue::new())?;
            if actual
                .as_string()
                .is_some_and(|actual| *actual.as_bytes() == *applet_type.as_bytes())
            {
                Ok(Some(*node))
            } else {
                Ok(None)
            }
        });
        // any other method is its applet's command
        methods.add_meta_method(MetaMethod::Index, |lua, _, name: String| {
            lua.create_function(
                move |lua, (node, args): (mlua::UserDataRef<Node>, MultiValue)| {
                    call(lua, Some(node.0), &name, &args)
                },
            )
        });
        methods.add_meta_method(MetaMethod::Eq, |_, node, other: mlua::UserDataRef<Node>| {
            Ok(node.0 == other.0)
        });
        methods.add_meta_method(MetaMethod::ToString, |_, node, ()| {
            Ok(Value::Node(node.0).to_string())
        });
    }
}

/// Read-only functions completion may run to know what they give (eg `context.focus():`)
const READ_ONLY_CALLS: [&str; 2] = ["focus", "root"];

/// What a chain ends in. Only `READ_ONLY_CALLS` and `parent` are run; `add_child` is known to give a `Node`.
enum Resolved {
    Value(LuaValue),
    /// Of no known type
    Node,
}

/// Candidates for the word `text` ends in: globals, a table's fields after `.`, or a node's methods (its applet's
/// commands too) after `:`
fn completions(lua: &Lua, text: &str) -> Vec<String> {
    // the trailing chain, eg `context.focus():ti`
    let start = text
        .char_indices()
        .rev()
        .find(|&(_, c)| !(c.is_alphanumeric() || "_.:()".contains(c)))
        .map_or(0, |(index, c)| index + c.len_utf8());
    let chain = &text[start..];
    let (receiver, separator, word) = match chain.rfind(['.', ':']) {
        Some(index) => (&chain[..index], &chain[index..=index], &chain[index + 1..]),
        None => ("", "", chain),
    };
    let mut candidates: Vec<String> = if receiver.is_empty() {
        global_names(lua)
    } else {
        match (resolve(lua, receiver), separator) {
            (Some(Resolved::Node), ":") => node_methods(),
            (Some(Resolved::Value(LuaValue::UserData(node))), ":") if node.is::<Node>() => {
                let mut methods = node_methods();
                if let Some(Value::String(applet_type)) = node_call(&node, "type") {
                    methods.extend(
                        command::signatures(&applet_type)
                            .iter()
                            .map(|signature| signature.name.to_string()),
                    );
                }
                methods
            }
            (Some(Resolved::Value(LuaValue::Table(table))), ".") => keys(&table),
            _ => Vec::new(),
        }
    };
    candidates.retain(|candidate| candidate.starts_with(word));
    candidates.sort();
    candidates.dedup();
    candidates
}

/// `chain`'s value, eg `context.focus():parent()`
fn resolve(lua: &Lua, chain: &str) -> Option<Resolved> {
    let mut resolved: Option<Resolved> = None;
    let mut separator = None;
    let mut rest = chain;
    loop {
        let end = rest.find(['.', ':']).unwrap_or(rest.len());
        let segment = &rest[..end];
        let (name, called) = match segment.strip_suffix("()") {
            Some(name) => (name, true),
            None => (segment, false),
        };
        resolved = Some(match (resolved, separator, called) {
            (None, None, false) => Resolved::Value(lua.globals().get(name).ok()?),
            (Some(Resolved::Value(LuaValue::Table(table))), Some('.'), false) => {
                Resolved::Value(table.get(name).ok()?)
            }
            (Some(Resolved::Value(LuaValue::Table(table))), Some('.'), true)
                if READ_ONLY_CALLS.contains(&name) =>
            {
                // known to give a node, even if asking fails
                let function = table.get::<mlua::Function>(name).ok()?;
                function.call(()).map_or(Resolved::Node, Resolved::Value)
            }
            (Some(Resolved::Value(LuaValue::UserData(node))), Some(':'), true)
                if node.is::<Node>() && name == "parent" =>
            {
                Resolved::Value(to_lua(lua, node_call(&node, name)?).ok()?)
            }
            (Some(_), Some(':'), true) if name == "add_child" => Resolved::Node,
            _ => return None,
        });
        if end == rest.len() {
            return resolved;
        }
        separator = rest[end..].chars().next();
        rest = &rest[end + 1..];
    }
}

/// A node command with no arguments, on a `Node` userdata
fn node_call(node: &mlua::AnyUserData, name: &str) -> Option<Value> {
    let uid = node.borrow::<Node>().ok()?.0;
    command::call(Some(uid), name, Vec::new(), &cancelled).ok()
}

fn node_methods() -> Vec<String> {
    NODE_COMMANDS
        .iter()
        .chain(&["cast"])
        .map(ToString::to_string)
        .collect()
}

fn keys(table: &mlua::Table) -> Vec<String> {
    table
        .pairs::<LuaValue, LuaValue>()
        .filter_map(|pair| match pair.ok()?.0 {
            LuaValue::String(key) => Some(key.to_str().ok()?.to_string()),
            _ => None,
        })
        .collect()
}

/// The eval's own globals, and the builtins under them (sandbox mode puts those in the globals' `__index`)
fn global_names(lua: &Lua) -> Vec<String> {
    let globals = lua.globals();
    let mut names = keys(&globals);
    if let Some(LuaValue::Table(builtins)) = globals
        .metatable()
        .and_then(|metatable| metatable.get::<LuaValue>("__index").ok())
    {
        names.extend(keys(&builtins));
    }
    names
}

fn from_lua(value: &LuaValue) -> mlua::Result<Value> {
    Ok(match value {
        LuaValue::Nil => Value::Nil,
        LuaValue::Boolean(value) => Value::Bool(*value),
        #[expect(clippy::cast_precision_loss, reason = "Luau numbers are f64 anyway")]
        LuaValue::Integer(value) => Value::Number(*value as f64),
        LuaValue::Number(value) => Value::Number(*value),
        LuaValue::String(value) => Value::String(value.to_str()?.to_string()),
        LuaValue::UserData(data) => Value::Node(data.borrow::<Node>()?.0),
        LuaValue::Table(table) => Value::List(
            table
                .sequence_values::<LuaValue>()
                .map(|value| from_lua(&value?))
                .collect::<mlua::Result<_>>()?,
        ),
        other => {
            return Err(mlua::Error::runtime(format!(
                "can't pass a {}",
                other.type_name()
            )));
        }
    })
}

/// A returned value; what commands can't take is shown as text
fn returned(value: &LuaValue) -> Value {
    from_lua(value).unwrap_or_else(|_| {
        Value::String(value.to_string().unwrap_or_else(|_| format!("{value:?}")))
    })
}

fn to_lua(lua: &Lua, value: Value) -> mlua::Result<LuaValue> {
    Ok(match value {
        Value::Nil => LuaValue::Nil,
        Value::Bool(value) => LuaValue::Boolean(value),
        Value::Number(value) => LuaValue::Number(value),
        Value::String(value) => LuaValue::String(lua.create_string(value)?),
        Value::List(values) => LuaValue::Table(
            lua.create_sequence_from(
                values
                    .into_iter()
                    .map(|value| to_lua(lua, value))
                    .collect::<mlua::Result<Vec<_>>>()?,
            )?,
        ),
        Value::Node(uid) => LuaValue::UserData(lua.create_userdata(Node(uid))?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval_all(code: &str) -> (Vec<String>, Result<Vec<Value>, String>) {
        let (_handle, outputs) = eval(code.to_string(), || {});
        let mut printed = Vec::new();
        for output in outputs {
            match output {
                Output::Print(line) => printed.push(line),
                Output::Done(result) => return (printed, result),
            }
        }
        unreachable!("ends with Done")
    }

    #[test]
    fn expressions_statements_and_globals() {
        assert_eq!(eval_all("1 + 2").1, Ok(vec![Value::Number(3.0)]));
        assert_eq!(eval_all("x = 5").1, Ok(vec![]));
        assert_eq!(eval_all("x * 2").1, Ok(vec![Value::Number(10.0)]));
        assert_eq!(eval_all("print('a', 1)").0, vec!["a\t1"]);
        assert!(
            eval_all("string.len = nil").1.is_err(),
            "builtins are read-only"
        );
        assert_eq!(
            eval_all("help() == sonamu.help()").1,
            Ok(vec![Value::Bool(true)])
        );
    }

    #[test]
    fn completion() {
        let complete_all = |text: &str| complete(text.to_string(), || {}).recv().unwrap();
        assert_eq!(complete_all("cont"), ["context"]);
        assert_eq!(complete_all("x = context.fo"), ["focus"]);
        assert_eq!(complete_all("context.focus():ti"), ["title"]);
        assert_eq!(complete_all("string.le"), ["len"]);
        assert_eq!(
            complete_all("context.focus():title():"),
            Vec::<String>::new()
        );
        assert!(eval_all("my_var = 1").1.is_ok());
        assert_eq!(complete_all("my_v"), ["my_var"]);
        // the fake root is a command hub (its `type` answer is `"type []"`, declared below)
        command::declare(
            "type []",
            &[command::Signature {
                name: "set_title",
                params: "title: string",
                returns: "nil",
                doc: "",
            }],
        );
        assert_eq!(complete_all("context.root():set"), ["set_title"]);
    }

    #[test]
    fn cancel() {
        let (handle, outputs) = eval("while true do end".to_string(), || {});
        std::thread::sleep(std::time::Duration::from_millis(50));
        handle.cancel();
        let Ok(Output::Done(result)) = outputs.recv() else {
            panic!("done");
        };
        assert!(result.unwrap_err().contains("cancelled"));
    }

    /// A world answering each call with its name and args, with `uid` as its root
    fn fake_world(uid: NodeUid) {
        let (world, calls) = calloop::channel::channel::<command::NodeCall>();
        command::register(uid, world.clone());
        command::set_root_world(world);
        std::thread::spawn(move || {
            let mut event_loop = calloop::EventLoop::<()>::try_new().unwrap();
            event_loop
                .handle()
                .insert_source(calls, move |event, &mut (), ()| {
                    if let calloop::channel::Event::Msg(call) = event {
                        let answer = match call.name.as_str() {
                            "root" => Value::Node(uid),
                            name => Value::String(format!("{name} {:?}", call.args)),
                        };
                        let _ = call.reply.send(Ok(answer));
                    }
                })
                .unwrap();
            event_loop.run(None, &mut (), |()| {}).unwrap();
        });
    }

    #[test]
    fn commands() {
        let (_, result) = eval_all("context.root()");
        assert!(result.unwrap_err().contains("the node is gone"));

        let uid = NodeUid::new();
        fake_world(uid);
        assert_eq!(eval_all("context.root()").1, Ok(vec![Value::Node(uid)]));
        assert_eq!(
            eval_all("context.root():title()").1,
            Ok(vec![Value::String("title []".to_string())])
        );
        // not a node command: its applet's
        assert_eq!(
            eval_all("context.root():open('/tmp', {'a'})").1,
            Ok(vec![Value::String(
                r#"open [String("/tmp"), List([String("a")])]"#.to_string()
            )])
        );
        assert_eq!(
            eval_all("context.root() == context.root()").1,
            Ok(vec![Value::Bool(true)])
        );

        // the fake world answers `type` with `"type []"`
        command::declare(
            "type []",
            &[command::Signature {
                name: "set_title",
                params: "title: string",
                returns: "nil",
                doc: "",
            }],
        );
        let complete_all = |text: &str| complete(text.to_string(), || {}).recv().unwrap();
        assert_eq!(complete_all("context.root():set"), ["set_title"]);
        let help = eval_all("sonamu.help()").1.unwrap();
        assert!(help[0].to_string().contains("set_title"));
    }
}
