//! Luau scripting (see `.claude/COMMANDS.md`): one Luau state on its own thread, shared by every `eval`.
//!
//! Commands block the script thread until their world answers; worlds never wait on it, so this can't deadlock.

use mlua::{Lua, MetaMethod, MultiValue, UserData, UserDataMethods, Value as LuaValue, VmState};
use sonamu_command::{self as command, CommandResult, NODE_COMMANDS, NodeUid, Value};
use sonamu_common::settings::{Key, Keybind};
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

/// A `print` or error of Luau no one waits on (`init.luau`, bindings, hooks), like Emacs' `*Messages*`
#[derive(Clone, Debug)]
pub struct Message {
    pub text: String,
    pub error: bool,
}

/// Kept; older ones are dropped
const MAX_MESSAGES: usize = 200;
/// The messages, and how many there were ever (so the root can tell a new last one)
static MESSAGES: Mutex<(Vec<Message>, u64)> = Mutex::new((Vec::new(), 0));
/// Called after each message
static ON_MESSAGE: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> = std::sync::OnceLock::new();

/// Calls `notify` after each new message (eg wakes the root to show it); once
pub fn on_message(notify: impl Fn() + Send + Sync + 'static) {
    let _ = ON_MESSAGE.set(Box::new(notify));
}

/// The last message, and how many there were ever
pub fn last_message() -> (Option<Message>, u64) {
    let messages = MESSAGES.lock().unwrap_or_else(PoisonError::into_inner);
    (messages.0.last().cloned(), messages.1)
}

fn message(text: String, error: bool) {
    eprintln!("Luau: {text}");
    {
        let mut messages = MESSAGES.lock().unwrap_or_else(PoisonError::into_inner);
        messages.0.push(Message { text, error });
        if messages.0.len() > MAX_MESSAGES {
            messages.0.remove(0);
        }
        messages.1 += 1;
    }
    if let Some(notify) = ON_MESSAGE.get() {
        notify();
    }
}

/// Set by `interrupt`, cleared when the next job starts
static QUIT: AtomicBool = AtomicBool::new(false);

/// Cancels whatever runs on the script thread, like Emacs' `C-g` (eg a stuck `keymap.set` function)
pub fn interrupt() {
    QUIT.store(true, Ordering::Relaxed);
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
    /// Runs Luau no one waits on; its `print`s and errors go to stderr
    Call(Call),
}

enum Call {
    /// `init.luau` in the config dir
    Init,
    /// A `keymap.set` function
    Binding(u64),
    /// `events.on` functions for this event
    Emit(String, Vec<Value>),
    /// A `process.output` callback (in the state's `CALLBACKS_KEY` table, taken out), with the program's result
    Callback(u64, Result<String, String>),
}

/// `keymap.set`'s keys, by binding id (the function is in the state's `BINDINGS_KEY` table)
static BINDINGS: Mutex<Vec<(Keybind, u64)>> = Mutex::new(Vec::new());
/// Named registry values of the state: `keymap.set`'s functions by id, `events.on`'s by event
const BINDINGS_KEY: &str = "sonamu_bindings";
const HOOKS_KEY: &str = "sonamu_hooks";
const CALLBACKS_KEY: &str = "sonamu_callbacks";
/// Ids of bindings and callbacks (not reused across `sonamu.reset()`)
static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Runs `init.luau` from the config dir, once Sonamu has started (again after `sonamu.reset()`)
pub fn start() {
    send_job(Job::Call(Call::Init));
}

/// The `keymap.set` binding for this key with exactly these mods (`[ctrl, alt, shift, logo]`), if any
pub fn binding(symbol: Option<char>, raw_keycode: u32, mods: [bool; 4]) -> Option<u64> {
    BINDINGS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .find(|(keybind, _)| keybind.matches(symbol, raw_keycode, mods))
        .map(|&(_, id)| id)
}

/// Runs a `binding`'s function
pub fn run_binding(id: u64) {
    send_job(Job::Call(Call::Binding(id)));
}

/// Calls the `events.on` functions for `event` with `args` (eg `"focus"`)
pub fn emit(event: &str, args: Vec<Value>) {
    send_job(Job::Call(Call::Emit(event.to_string(), args)));
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
    QUIT.load(Ordering::Relaxed)
        || CURRENT.with_borrow(|current| {
            current
                .as_ref()
                .is_some_and(|current| current.cancelled.load(Ordering::Relaxed))
        })
}

fn send(output: Output) {
    CURRENT.with_borrow(|current| match (current, output) {
        (Some(current), output) => {
            let _ = current.output.send(output);
            (current.notify)();
        }
        // a binding or hook (`Job::Call`)
        (None, Output::Print(line)) => message(line, false),
        (None, Output::Done(_)) => {}
    });
}

fn run(jobs: &mpsc::Receiver<Job>) {
    let mut lua = new_lua().expect("the script state");
    for job in jobs {
        QUIT.store(false, Ordering::Relaxed);
        if let Err(err) = add_type_tables(&lua) {
            message(err.to_string(), true);
        }
        let (code, output, notify, cancelled) = match job {
            Job::Call(call) => {
                if let Err(err) = run_call(&lua, call) {
                    message(err, true);
                }
                continue;
            }
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
            BINDINGS
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clear();
            lua = new_lua().expect("the script state");
            if let Err(err) = run_call(&lua, Call::Init) {
                message(err, true);
            }
        }
    }
}

fn run_call(lua: &Lua, call: Call) -> Result<(), String> {
    match call {
        Call::Init => {
            let Some(path) = sonamu_common::settings::config_dir().map(|dir| dir.join("init.luau"))
            else {
                return Ok(());
            };
            let Ok(code) = std::fs::read_to_string(&path) else {
                return Ok(());
            };
            run_code(lua, &code)
                .map(drop)
                .map_err(|err| format!("{}: {err}", path.display()))
        }
        Call::Binding(id) => {
            let bindings: mlua::Table = lua
                .named_registry_value(BINDINGS_KEY)
                .map_err(|err| err.to_string())?;
            let function: Option<mlua::Function> =
                bindings.get(id).map_err(|err| err.to_string())?;
            function.map_or(Ok(()), |function| {
                function.call::<()>(()).map_err(|err| err.to_string())
            })
        }
        Call::Callback(id, result) => {
            let callbacks: mlua::Table = lua
                .named_registry_value(CALLBACKS_KEY)
                .map_err(|err| err.to_string())?;
            let function: Option<mlua::Function> =
                callbacks.get(id).map_err(|err| err.to_string())?;
            callbacks
                .set(id, LuaValue::Nil)
                .map_err(|err| err.to_string())?;
            // gone after `sonamu.reset()`
            let Some(function) = function else {
                return Ok(());
            };
            match result {
                Ok(text) => function.call::<()>(text),
                Err(err) => function.call::<()>((LuaValue::Nil, err)),
            }
            .map_err(|err| err.to_string())
        }
        Call::Emit(event, args) => {
            let hooks: mlua::Table = lua
                .named_registry_value(HOOKS_KEY)
                .map_err(|err| err.to_string())?;
            let Some(functions) = hooks
                .get::<Option<mlua::Table>>(event)
                .map_err(|err| err.to_string())?
            else {
                return Ok(());
            };
            for function in functions.sequence_values::<mlua::Function>() {
                let args = args
                    .iter()
                    .map(|arg| to_lua(lua, arg.clone()))
                    .collect::<mlua::Result<MultiValue>>()
                    .map_err(|err| err.to_string())?;
                // one failing doesn't stop the others
                if let Err(err) = function.map_err(|err| err.to_string())?.call::<()>(args) {
                    message(err.to_string(), true);
                }
            }
            Ok(())
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

    add_process(&lua, &globals)?;

    let sonamu = lua.create_table()?;
    for name in ["open", "quit", "reload_settings"] {
        sonamu.set(name, tree_function(&lua, name)?)?;
    }
    sonamu.set(
        "definitions",
        lua.create_function(|_, ()| Ok(definitions()))?,
    )?;
    sonamu.set(
        "messages",
        lua.create_function(|_, ()| {
            let messages = MESSAGES.lock().unwrap_or_else(PoisonError::into_inner);
            Ok(messages
                .0
                .iter()
                .map(|message| message.text.as_str())
                .collect::<Vec<_>>()
                .join("\n"))
        })?,
    )?;
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

    add_keymap_and_events(&lua, &globals)?;

    // builtins read-only; what evals define goes in a table of its own
    lua.sandbox(true)?;
    Ok(lua)
}

/// `process.spawn` and `process.output`
fn add_process(lua: &Lua, globals: &mlua::Table) -> mlua::Result<()> {
    let process = lua.create_table()?;
    process.set("spawn", tree_function(lua, "spawn")?)?;
    process.set(
        "output",
        lua.create_function(
            |lua,
             (program, args, dir, callback): (
                String,
                Option<Vec<String>>,
                Option<String>,
                Option<mlua::Function>,
            )| {
                let args = args.unwrap_or_default();
                let Some(callback) = callback else {
                    return output(&program, &args, dir.as_deref(), &cancelled)
                        .map(Some)
                        .map_err(mlua::Error::runtime);
                };
                // like nvim's `vim.system(cmd, on_exit)`: off the script thread, so others run meanwhile
                let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
                lua.named_registry_value::<mlua::Table>(CALLBACKS_KEY)?
                    .set(id, callback)?;
                std::thread::spawn(move || {
                    let result = output(&program, &args, dir.as_deref(), &|| false);
                    send_job(Job::Call(Call::Callback(id, result)));
                });
                Ok(None)
            },
        )?,
    )?;
    globals.set("process", process)?;
    Ok(())
}

/// `keymap.set` and `events.on`, whose functions are kept in the state's registry
fn add_keymap_and_events(lua: &Lua, globals: &mlua::Table) -> mlua::Result<()> {
    lua.set_named_registry_value(BINDINGS_KEY, lua.create_table()?)?;
    lua.set_named_registry_value(CALLBACKS_KEY, lua.create_table()?)?;
    lua.set_named_registry_value(HOOKS_KEY, lua.create_table()?)?;
    let keymap = lua.create_table()?;
    keymap.set(
        "set",
        lua.create_function(
            |lua, (mods, key, function): (Vec<String>, LuaValue, mlua::Function)| {
                let key = match key {
                    LuaValue::Integer(code) => {
                        Key::Code(u32::try_from(code).map_err(mlua::Error::runtime)?)
                    }
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "a key code"
                    )]
                    LuaValue::Number(code) => Key::Code(code as u32),
                    LuaValue::String(text) => {
                        let text = text.to_str()?;
                        let mut chars = text.chars();
                        match (chars.next(), chars.next()) {
                            (Some(c), None) => Key::Char(c),
                            _ => {
                                return Err(mlua::Error::runtime(
                                    "the key is one character or a key code",
                                ));
                            }
                        }
                    }
                    other => {
                        return Err(mlua::Error::runtime(format!(
                            "can't bind a {}",
                            other.type_name()
                        )));
                    }
                };
                let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
                lua.named_registry_value::<mlua::Table>(BINDINGS_KEY)?
                    .set(id, function)?;
                let keybind = Keybind {
                    mods,
                    key,
                    command: String::new(),
                };
                let mut bindings = BINDINGS.lock().unwrap_or_else(PoisonError::into_inner);
                // the same key bound again replaces it
                bindings
                    .retain(|(bound, _)| bound.mods != keybind.mods || bound.key != keybind.key);
                bindings.push((keybind, id));
                Ok(())
            },
        )?,
    )?;
    globals.set("keymap", keymap)?;
    let events = lua.create_table()?;
    events.set(
        "on",
        lua.create_function(|lua, (event, function): (String, mlua::Function)| {
            let hooks: mlua::Table = lua.named_registry_value(HOOKS_KEY)?;
            let functions =
                if let Some(functions) = hooks.get::<Option<mlua::Table>>(event.as_str())? {
                    functions
                } else {
                    let functions = lua.create_table()?;
                    hooks.set(event, &functions)?;
                    functions
                };
            functions.push(function)
        })?,
    )?;
    globals.set("events", events)?;
    Ok(())
}

const HELP: &str = "\
help() / sonamu.help()         this
context.focus(): Node          the innermost focused node
context.root(): Node           the root node
process.spawn(program, {args}?)
process.output(program, {args}?, dir?): string   waits for it (Ctrl+C kills it); stdout, then stderr
process.output(program, {args}?, dir?, fn(out, err))   doesn't wait (other Luau runs meanwhile): fn gets either
keymap.set({mods}, key, fn)   a global key (`ctrl`, `alt`, `shift`, `logo`; a character or evdev code)
events.on(event, fn)           `focus` (focus moved), `window` (a wl window opened: its app id)
~/.config/sonamu/init.luau     runs at start and on sonamu.reset()
clipboard.copy(text)           Sonamu's clipboard, for wl clients too
clipboard.paste(): string?
sonamu.open(applet, {args}?)   like `sonamu-msg open`
sonamu.reload_settings()
sonamu.quit()
sonamu.reset()                 a fresh Luau state
sonamu.messages(): string      prints and errors of init.luau, bindings and hooks (the last shows at the bottom)
sonamu.definitions(): string   luau-lsp definitions of all this, eg `sonamu-msg eval 'sonamu.definitions()'`
Node: close() focus() add_child(applet, {args}?): Node swap_sibling(offset) toggle_half() wrap_project()
      parent(): Node? children(): {Node} type(): string title(): string cast(type): Node?
      inner(): Node? (the root of a project's own tree)
      (also <type>.from_node(n), eg file_manager.from_node(n): typed in sonamu.definitions())
      spawn(program, {args}?) (in its tree's directory) quit()
      any other method is a command of its applet, eg hub:clear()
Super+Esc: the buffer line (Esc leaves). There and in the hub: Tab completes, Up/Down recalls, Shift+Enter adds a line, PageUp/PageDown scrolls, Esc clears, Ctrl+V pastes, Ctrl+C cancels.
Super+? lists every keybind.";

/// The `Node` methods' Luau types, as in `NODE_COMMANDS` (and `cast`)
const NODE_METHODS: &str = "\
    function close(self): ()
    function focus(self): ()
    function add_child(self, applet: string, args: {string}?): Node
    function swap_sibling(self, offset: number): ()
    function toggle_half(self): ()
    function wrap_project(self): ()
    function parent(self): Node?
    function children(self): {Node}
    function type(self): string
    function title(self): string
    function cast(self, applet_type: string): Node?  -- typed: <applet_type>.from_node(n)
    function inner(self): Node?
    function spawn(self, program: string, args: {string}?): ()
    function quit(self): ()
";

/// A luau-lsp definitions file: the globals, `Node`, and a class per applet type (from its declared commands)
/// extending it, named after the type (eg `file_manager`)
fn definitions() -> String {
    let mut out = format!("declare class Node\n{NODE_METHODS}end\n");
    for (applet_type, signatures) in command::all_signatures() {
        let _ = writeln!(out, "\ndeclare class {applet_type} extends Node");
        for signature in signatures {
            let params = if signature.params.is_empty() {
                String::new()
            } else {
                format!(", {}", signature.params)
            };
            let _ = writeln!(out, "    -- {}", signature.doc);
            let _ = writeln!(
                out,
                "    function {}(self{params}): {}",
                signature.name, signature.returns
            );
        }
        out.push_str("end\n");
        let _ = writeln!(
            out,
            "declare {applet_type}: {{ from_node: (node: Node) -> {applet_type}? }}"
        );
    }
    out.push_str(
        "
declare context: { focus: () -> Node, root: () -> Node }
declare process: {
    spawn: (program: string, args: {string}?) -> (),
    output: ((program: string, args: {string}?, dir: string?) -> string)
        & ((program: string, args: {string}?, dir: string?, fn: (string?, string?) -> ()) -> ()),
}
declare clipboard: { copy: (text: string) -> boolean, paste: () -> string? }
declare keymap: { set: (mods: {string}, key: string | number, fn: () -> ()) -> () }
declare events: { on: (event: string, fn: (...any) -> ()) -> () }
declare sonamu: {
    open: (applet: string, args: {string}?) -> (),
    quit: () -> (),
    reload_settings: () -> (),
    reset: () -> (),
    messages: () -> string,
    help: () -> string,
    definitions: () -> string,
}
declare function help(): string
",
    );
    out
}

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

/// Runs the program (in `dir`, else Sonamu's) and waits for it, polling `cancelled` to kill it;
/// its stdout then stderr, an error if it failed
fn output(
    program: &str,
    args: &[String],
    dir: Option<&str>,
    cancelled: &dyn Fn() -> bool,
) -> Result<String, String> {
    use std::io::Read as _;
    let mut command = std::process::Command::new(program);
    command
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    sonamu_common::connect_wl_client(&mut command);
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    let mut child = command.spawn().map_err(|err| format!("{program}: {err}"))?;
    // read on threads, so a full pipe can't stall it
    let read = |pipe: Option<Box<dyn std::io::Read + Send>>| {
        std::thread::spawn(move || {
            let mut text = String::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_string(&mut text);
            }
            text
        })
    };
    let stdout = read(child.stdout.take().map(|pipe| Box::new(pipe) as _));
    let stderr = read(child.stderr.take().map(|pipe| Box::new(pipe) as _));
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|err| err.to_string())? {
            break status;
        }
        if cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err("cancelled".to_string());
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    let mut text = stdout.join().unwrap_or_default();
    text.push_str(&stderr.join().unwrap_or_default());
    if status.success() {
        Ok(text)
    } else {
        Err(format!("{text}{status}"))
    }
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
            cast(lua, *node, &applet_type)
        });
        methods.add_method("inner", |lua, node, ()| {
            to_lua(
                lua,
                command::inner(node.0, &cancelled).map_err(mlua::Error::runtime)?,
            )
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

/// `node` if its applet is of `applet_type`
fn cast(lua: &Lua, node: Node, applet_type: &str) -> mlua::Result<Option<Node>> {
    let actual = call(lua, Some(node.0), "type", &MultiValue::new())?;
    Ok(actual
        .as_string()
        .is_some_and(|actual| *actual.as_bytes() == *applet_type.as_bytes())
        .then_some(node))
}

/// A global table per declared applet type with `from_node(n)` (`cast`, but typed in `definitions`), unless the
/// name is taken; types are declared as spawners register, so before each job
fn add_type_tables(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    for applet_type in command::all_signatures().into_keys() {
        if globals.contains_key(applet_type.as_str())? {
            continue;
        }
        let table = lua.create_table()?;
        let name = applet_type.clone();
        table.set(
            "from_node",
            lua.create_function(move |lua, node: mlua::UserDataRef<Node>| cast(lua, *node, &name))?,
        )?;
        globals.set(applet_type, table)?;
    }
    Ok(())
}

/// Read-only functions completion may run to know what they give (eg `context.focus():`)
const READ_ONLY_CALLS: [&str; 2] = ["focus", "root"];

/// What a chain ends in. Only `READ_ONLY_CALLS`, `parent` and `inner` are run; `add_child` is known to give a `Node`.
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
            (Some(Resolved::Value(LuaValue::UserData(node))), Some(':'), true)
                if node.is::<Node>() && name == "inner" =>
            {
                let uid = node.borrow::<Node>().ok()?.0;
                Resolved::Value(to_lua(lua, command::inner(uid, &cancelled).ok()?).ok()?)
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
        .chain(&["cast", "inner"])
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
        // a sequence (or empty): a list; else string keys
        LuaValue::Table(table)
            if table.pairs::<LuaValue, LuaValue>().count() == table.raw_len() =>
        {
            Value::List(
                table
                    .sequence_values::<LuaValue>()
                    .map(|value| from_lua(&value?))
                    .collect::<mlua::Result<_>>()?,
            )
        }
        LuaValue::Table(table) => Value::Map(
            table
                .pairs::<LuaValue, LuaValue>()
                .map(|pair| {
                    let (key, value) = pair?;
                    let LuaValue::String(key) = key else {
                        return Err(mlua::Error::runtime(format!(
                            "can't pass a table with a {} key (only a list or string keys)",
                            key.type_name()
                        )));
                    };
                    Ok((key.to_str()?.to_string(), from_lua(&value)?))
                })
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
        Value::Map(values) => LuaValue::Table(
            lua.create_table_from(
                values
                    .into_iter()
                    .map(|(key, value)| Ok((key, to_lua(lua, value)?)))
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
        assert_eq!(eval_all("{1, 2}").1.unwrap()[0].to_string(), "{1, 2}");
        assert_eq!(
            eval_all("{a = 1, b = {true}}").1.unwrap()[0].to_string(),
            "{a = 1, b = {true}}"
        );
        assert_eq!(eval_all("{}").1, Ok(vec![Value::List(vec![])]));
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
    fn bindings_and_hooks() {
        let logo = [false, false, false, true];
        assert!(
            eval_all("keymap.set({'logo'}, 'q', function() bound = true end)")
                .1
                .is_ok()
        );
        assert_eq!(binding(Some('Q'), 0, [false; 4]), None, "mods must match");
        let id = binding(Some('Q'), 0, logo).expect("bound, case-insensitive");
        run_binding(id);
        assert_eq!(eval_all("bound").1, Ok(vec![Value::Bool(true)]));

        assert!(
            eval_all("events.on('focus', function(x) got = x end)")
                .1
                .is_ok()
        );
        emit("focus", vec![Value::Number(3.0)]);
        assert_eq!(eval_all("got").1, Ok(vec![Value::Number(3.0)]));

        // no one waits on them: messages
        assert!(
            eval_all("events.on('hook_test', function() error('boom') end)")
                .1
                .is_ok()
        );
        emit("hook_test", vec![]);
        assert!(eval_all("1").1.is_ok());
        assert!(
            eval_all("sonamu.messages()").1.unwrap()[0]
                .to_string()
                .contains("boom")
        );
    }

    #[test]
    fn process_output() {
        assert_eq!(
            eval_all("process.output('sh', {'-c', 'echo hi; echo err >&2'})").1,
            Ok(vec![Value::String("hi\nerr\n".to_string())])
        );
        assert!(eval_all("process.output('false')").1.is_err());
        assert!(
            eval_all("process.output('echo', {'later'}, nil, function(out) later = out end)")
                .1
                .is_ok()
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while eval_all("later").1 != Ok(vec![Value::String("later\n".to_string())]) {
            assert!(std::time::Instant::now() < deadline, "the callback ran");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
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

    #[test]
    fn interrupt_a_binding() {
        assert!(
            eval_all("keymap.set({'logo'}, 'l', function() while true do end end)")
                .1
                .is_ok()
        );
        run_binding(binding(Some('l'), 0, [false, false, false, true]).unwrap());
        // queued behind the binding
        let (_handle, outputs) = eval("1".to_string(), || {});
        std::thread::sleep(std::time::Duration::from_millis(100));
        interrupt();
        assert!(matches!(
            outputs.recv_timeout(std::time::Duration::from_secs(5)),
            Ok(Output::Done(Ok(_)))
        ));
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
        let definitions = definitions();
        assert!(definitions.contains("declare class Node"));
        assert!(definitions.contains("function set_title(self, title: string): nil"));
        assert!(definitions.contains("declare type []: { from_node"));
        // the fake root isn't of this type
        command::declare("from_node_test", &[]);
        assert_eq!(
            eval_all("from_node_test.from_node(context.root())").1,
            Ok(vec![Value::Nil])
        );
    }
}
