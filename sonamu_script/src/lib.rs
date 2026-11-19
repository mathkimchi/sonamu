//! Luau scripting (see `.claude/COMMANDS.md`): one Luau state on its own thread, shared by every `eval`.
//!
//! Commands block the script thread until their world answers; worlds never wait on it, so this can't deadlock.

mod require;
pub mod tools;

use mlua::{
    Lua, LuaSerdeExt, MetaMethod, MultiValue, UserData, UserDataMethods, Value as LuaValue, VmState,
};
use sonamu_command::{self as command, CommandResult, Effect, NODE_COMMANDS, NodeUid, Value};
use sonamu_common::{
    settings::{Action, Key, Keybind, config_dir},
    settings_file::{self, Source},
};
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
        /// Commands above it are refused
        max: Effect,
    },
    Complete {
        text: String,
        /// With signatures (`signed_completions`)
        signed: bool,
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

/// A `keymap.set` key: for focus of `applet_type` only, or any (`None`); its function is `id` in the state's
/// `BINDINGS_KEY` table
struct Binding {
    keybind: Keybind,
    applet_type: Option<String>,
    id: u64,
}
static BINDINGS: Mutex<Vec<Binding>> = Mutex::new(Vec::new());
/// The innermost focus's applet type, as of the last `focus` event (for `binding`)
static FOCUS_TYPE: Mutex<Option<String>> = Mutex::new(None);
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

/// The `keymap.set` binding for this key with exactly these mods (`[ctrl, alt, shift, logo]`), if any: the focus's
/// applet type's (like an Emacs major mode's map), else a global one
pub fn binding(symbol: Option<char>, raw_keycode: u32, mods: [bool; 4]) -> Option<u64> {
    let focus_type = FOCUS_TYPE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let bindings = BINDINGS.lock().unwrap_or_else(PoisonError::into_inner);
    let matching = || {
        bindings
            .iter()
            .filter(|binding| binding.keybind.matches_char(symbol, raw_keycode, mods))
    };
    matching()
        .find(|binding| binding.applet_type.is_some() && binding.applet_type == focus_type)
        .or_else(|| matching().find(|binding| binding.applet_type.is_none()))
        .map(|binding| binding.id)
}

/// Runs a `binding`'s function
pub fn run_binding(id: u64) {
    send_job(Job::Call(Call::Binding(id)));
}

/// Calls the `events.on` functions for `event` with `args`, then sends it to `subscribe`rs. `focus` is emitted
/// without args: the innermost focus is found on the script thread (worlds don't wait on each other).
pub fn emit(event: &str, args: Vec<Value>) {
    send_job(Job::Call(Call::Emit(event.to_string(), args)));
}

/// An emitted event and its args
pub type Event = (String, Vec<Value>);

static SUBSCRIBERS: Mutex<Vec<mpsc::Sender<Event>>> = Mutex::new(Vec::new());

/// Every event emitted from now on (eg for `sonamu-msg subscribe`), until the receiver is dropped
pub fn subscribe() -> mpsc::Receiver<Event> {
    let (events, received) = mpsc::channel();
    SUBSCRIBERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(events);
    received
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
    eval_limited(code, Effect::Spawn, notify)
}

/// `eval`, refusing commands (and builtins) whose effect is above `max`
pub fn eval_limited(
    code: String,
    max: Effect,
    notify: impl Fn() + Send + 'static,
) -> (EvalHandle, mpsc::Receiver<Output>) {
    let (output, outputs) = mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    send_job(Job::Eval {
        code,
        output,
        notify: Box::new(notify),
        cancelled: Arc::clone(&cancelled),
        max,
    });
    (EvalHandle(cancelled), outputs)
}

/// What could follow the end of `text` (the word it ends in), from the live state, after any evals before it.
/// `notify` is called once it is ready.
pub fn complete(text: String, notify: impl Fn() + Send + 'static) -> mpsc::Receiver<Vec<String>> {
    complete_job(text, false, Box::new(notify))
}

/// `complete`'s candidates with their signatures where known (eg `title(): string`), waiting for them
#[must_use]
pub fn complete_signed(text: String) -> Vec<String> {
    complete_job(text, true, Box::new(|| {}))
        .recv()
        .unwrap_or_default()
}

fn complete_job(
    text: String,
    signed: bool,
    notify: Box<dyn Fn() + Send>,
) -> mpsc::Receiver<Vec<String>> {
    let (reply, replied) = mpsc::channel();
    send_job(Job::Complete {
        text,
        signed,
        reply,
        notify,
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
    max: Effect,
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

/// The running eval's limit; bindings, hooks and completion have none
fn max_effect() -> Effect {
    CURRENT.with_borrow(|current| {
        current
            .as_ref()
            .map_or(Effect::Spawn, |current| current.max)
    })
}

/// Refuses `name` if `effect` is above the running eval's limit
fn allow(effect: Effect, name: &str) -> mlua::Result<()> {
    let max = max_effect();
    if effect > max {
        return Err(mlua::Error::runtime(effect.above(name, max)));
    }
    Ok(())
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
        let (code, output, notify, cancelled, max) = match job {
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
                max,
            } => (code, output, notify, cancelled, max),
            Job::Complete {
                text,
                signed,
                reply,
                notify,
            } => {
                let complete = if signed {
                    signed_completions
                } else {
                    completions
                };
                let _ = reply.send(complete(&lua, &text));
                notify();
                continue;
            }
        };
        CURRENT.set(Some(Current {
            output,
            notify,
            cancelled,
            max,
            reset: false,
        }));
        // a limited one can't change what bindings and hooks (unlimited) see
        let result = if max < Effect::Spawn {
            require::own_environment(&lua)
                .map_err(|err| err.to_string())
                .and_then(|environment| {
                    run_code(&lua, &code, require::EVAL_CHUNK, Some(&environment))
                })
        } else {
            run_code(&lua, &code, require::EVAL_CHUNK, None)
        };
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
            let Some(path) = config_dir().map(|dir| dir.join("init.luau")) else {
                return Ok(());
            };
            let Ok(code) = std::fs::read_to_string(&path) else {
                return Ok(());
            };
            run_code(lua, &code, &format!("@{}", path.display()), None).map(drop)
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
        Call::Emit(event, mut args) => {
            if event == "focus" && args.is_empty() {
                args.extend(command::focus(&cancelled).ok());
                let focus_type = match args.first() {
                    Some(&Value::Node(uid)) => {
                        match command::call(Some(uid), "type", Vec::new(), &cancelled) {
                            Ok(Value::String(applet_type)) => Some(applet_type),
                            _ => None,
                        }
                    }
                    _ => None,
                };
                *FOCUS_TYPE.lock().unwrap_or_else(PoisonError::into_inner) = focus_type;
            }
            SUBSCRIBERS
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .retain(|subscriber| subscriber.send((event.clone(), args.clone())).is_ok());
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
/// `name`: the chunk's, eg `@<path>` for a file (`require` is relative to it); `environment`: its globals, else the
/// shared ones
fn run_code(
    lua: &Lua,
    code: &str,
    name: &str,
    environment: Option<&mlua::Table>,
) -> Result<Vec<Value>, String> {
    let load = |code: &str| {
        let chunk = lua.load(code.to_string()).set_name(name);
        match environment {
            Some(environment) => chunk.set_environment(environment.clone()),
            None => chunk,
        }
        .into_function()
    };
    let function = load(&format!("return {code}"))
        .or_else(|_| load(code))
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

    add_clipboard(&lua, &globals)?;

    add_process(&lua, &globals)?;

    let sonamu = lua.create_table()?;
    for name in ["open", "quit", "reload_settings", "bar_layout", "traverse"] {
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
    let help_function = lua.create_function(|_, name: Option<String>| {
        Ok(match name {
            Some(name) => describe(&name).unwrap_or_else(|| {
                format!("Nothing is named {name}; sonamu.complete lists what there is")
            }),
            None => help(),
        })
    })?;
    sonamu.set("help", help_function.clone())?;
    sonamu.set(
        "complete",
        lua.create_function(|lua, text: String| Ok(signed_completions(lua, &text)))?,
    )?;
    globals.set("help", help_function)?;
    sonamu.set(
        "reset",
        lua.create_function(|_, ()| {
            allow(Effect::Destructive, "sonamu.reset")?;
            CURRENT.with_borrow_mut(|current| {
                if let Some(current) = current {
                    current.reset = true;
                }
            });
            Ok(())
        })?,
    )?;
    add_settings(&lua, &sonamu)?;
    globals.set("sonamu", sonamu)?;

    add_keymap_and_events(&lua, &globals)?;
    globals.set(
        "require",
        lua.create_require_function(require::ConfigRequirer::default())?,
    )?;

    // builtins read-only; what evals define goes in a table of its own
    lua.sandbox(true)?;
    Ok(lua)
}

/// `sonamu.setting`, `sonamu.set_setting` and `settings`
fn add_settings(lua: &Lua, sonamu: &mlua::Table) -> mlua::Result<()> {
    lua.globals().set("settings", Settings(String::new()))?;
    sonamu.set(
        "setting",
        lua.create_function(|lua, path: String| {
            setting_to_lua(lua, &setting(&path).map_err(mlua::Error::runtime)?)
        })?,
    )?;
    sonamu.set(
        "set_setting",
        lua.create_function(|lua, (path, value): (String, LuaValue)| {
            allow(Effect::Modify, "sonamu.set_setting")?;
            set_setting(&path, lua.from_value(value)?).map_err(mlua::Error::runtime)
        })?,
    )
}

fn setting_to_lua(lua: &Lua, value: &serde_json::Value) -> mlua::Result<LuaValue> {
    let options = mlua::serde::SerializeOptions::new()
        .serialize_none_to_null(false)
        .serialize_unit_to_null(false);
    lua.to_value_with(value, options)
}

/// `settings`: the settings under a dotted path (`""`: all) as fields, eg `settings.border.width = 5` is
/// `sonamu.set_setting('border.width', 5)`; a field holding an object is another `Settings`
struct Settings(String);

impl Settings {
    fn path(&self, key: &str) -> String {
        if self.0.is_empty() {
            key.to_owned()
        } else {
            format!("{}.{key}", self.0)
        }
    }

    fn field(&self, lua: &Lua, key: &str) -> mlua::Result<LuaValue> {
        let path = self.path(key);
        match setting(&path).map_err(mlua::Error::runtime)? {
            serde_json::Value::Object(_) => {
                lua.create_userdata(Settings(path)).map(LuaValue::UserData)
            }
            value => setting_to_lua(lua, &value),
        }
    }

    /// Its fields: the defaults' and the user's
    fn keys(&self) -> Vec<String> {
        let path = split_path(&self.0);
        let mut keys = Vec::new();
        let user = user_settings().map(|(_, user)| user).unwrap_or_default();
        for settings in [settings_file::defaults(), user] {
            if let serde_json::Value::Object(map) = settings_file::value_at(&settings, &path) {
                keys.extend(
                    map.into_iter()
                        .map(|(key, _)| key)
                        .filter(|key| !key.starts_with('$')),
                );
            }
        }
        keys
    }
}

impl UserData for Settings {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Index, |lua, this, key: String| {
            this.field(lua, &key)
        });
        methods.add_meta_method(
            MetaMethod::NewIndex,
            |lua, this, (key, value): (String, LuaValue)| {
                allow(Effect::Modify, "settings")?;
                set_setting(&this.path(&key), lua.from_value(value)?).map_err(mlua::Error::runtime)
            },
        );
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(setting(&this.0).map_err(mlua::Error::runtime)?.to_string())
        });
    }
}

fn split_path(path: &str) -> Vec<&str> {
    path.split('.').filter(|key| !key.is_empty()).collect()
}

/// The setting at the dotted `path` (eg `border.width`): the user's, else the default
pub(crate) fn setting(path: &str) -> Result<serde_json::Value, String> {
    let (_, user) = user_settings()?;
    Ok(settings_file::value_at(&user, &split_path(path)))
}

/// Writes the setting at the dotted `path` to `settings.json`, like the settings editor (if valid; the default
/// removes it); the watcher reloads it
pub(crate) fn set_setting(path: &str, value: serde_json::Value) -> Result<(), String> {
    let (file, mut user) = user_settings()?;
    settings_file::migrate(&mut user);
    settings_file::set(&mut user, &split_path(path), value);
    settings_file::check(&user).map_err(|err| err.to_string())?;
    settings_file::write(&file, Some(&user)).map_err(|err| err.to_string())
}

/// `settings.json` and what it holds (`{}` if there is none)
fn user_settings() -> Result<(std::path::PathBuf, serde_json::Value), String> {
    let dir = config_dir().ok_or("no config dir")?;
    let file = match settings_file::source(&dir) {
        Source::Json(file) => file,
        Source::Program(program) => {
            return Err(format!(
                "settings come from a program: {}",
                program.display()
            ));
        }
    };
    let user = settings_file::read(&file)
        .map_err(|err| format!("{}: {err}", file.display()))?
        .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));
    Ok((file, user))
}

/// `clipboard.copy` and `clipboard.paste`
fn add_clipboard(lua: &Lua, globals: &mlua::Table) -> mlua::Result<()> {
    let clipboard = lua.create_table()?;
    // `{primary = true}`: the primary selection
    let primary = |options: Option<mlua::Table>| -> mlua::Result<bool> {
        Ok(options
            .map(|options| options.get::<Option<bool>>("primary"))
            .transpose()?
            .flatten()
            .unwrap_or(false))
    };
    clipboard.set(
        "copy",
        lua.create_function(move |_, (text, options): (String, Option<mlua::Table>)| {
            allow(Effect::Modify, "clipboard.copy")?;
            Ok(if primary(options)? {
                sonamu_common::clipboard::copy_primary(text)
            } else {
                sonamu_common::clipboard::copy(text)
            })
        })?,
    )?;
    clipboard.set(
        "paste",
        lua.create_function(move |_, options: Option<mlua::Table>| {
            Ok(if primary(options)? {
                sonamu_common::clipboard::paste_primary()
            } else {
                sonamu_common::clipboard::paste()
            })
        })?,
    )?;
    globals.set("clipboard", clipboard)?;
    Ok(())
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
                allow(Effect::Spawn, "process.output")?;
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

impl Binding {
    fn is(&self, keybind: &Keybind, applet_type: Option<&String>) -> bool {
        self.keybind.mods == keybind.mods
            && self.keybind.key == keybind.key
            && self.applet_type.as_ref() == applet_type
    }
}

/// `keymap.set`'s/`unset`'s key (a character or an evdev code) and its options' `type`
fn keymap_key(
    mods: Vec<String>,
    key: &LuaValue,
    options: Option<&mlua::Table>,
) -> mlua::Result<(Keybind, Option<String>)> {
    let key = match key {
        &LuaValue::Integer(code) => Key::Code(u32::try_from(code).map_err(mlua::Error::runtime)?),
        #[expect(clippy::cast_sign_loss, reason = "a key code")]
        &LuaValue::Number(code) => Key::Code(code as u32),
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
    let applet_type = options
        .map(|options| options.get("type"))
        .transpose()?
        .flatten();
    let keybind = Keybind {
        mods,
        key,
        action: Action::Shell {
            shell: String::new(),
        },
    };
    Ok((keybind, applet_type))
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
            |lua,
             (mods, key, function, options): (
                Vec<String>,
                LuaValue,
                mlua::Function,
                Option<mlua::Table>,
            )| {
                // it runs later without a limit
                allow(Effect::Spawn, "keymap.set")?;
                let (keybind, applet_type) = keymap_key(mods, &key, options.as_ref())?;
                let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
                lua.named_registry_value::<mlua::Table>(BINDINGS_KEY)?
                    .set(id, function)?;
                let mut bindings = BINDINGS.lock().unwrap_or_else(PoisonError::into_inner);
                // the same key bound again (for the same type) replaces it
                bindings.retain(|bound| !bound.is(&keybind, applet_type.as_ref()));
                bindings.push(Binding {
                    keybind,
                    applet_type,
                    id,
                });
                Ok(())
            },
        )?,
    )?;
    keymap.set(
        "unset",
        lua.create_function(
            |_, (mods, key, options): (Vec<String>, LuaValue, Option<mlua::Table>)| {
                allow(Effect::Modify, "keymap.unset")?;
                let (keybind, applet_type) = keymap_key(mods, &key, options.as_ref())?;
                let mut bindings = BINDINGS.lock().unwrap_or_else(PoisonError::into_inner);
                let count = bindings.len();
                bindings.retain(|bound| !bound.is(&keybind, applet_type.as_ref()));
                // whether it was bound
                Ok(bindings.len() != count)
            },
        )?,
    )?;
    globals.set("keymap", keymap)?;
    let events = lua.create_table()?;
    events.set(
        "on",
        lua.create_function(|lua, (event, function): (String, mlua::Function)| {
            // it runs later without a limit
            allow(Effect::Spawn, "events.on")?;
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
help(name?) / sonamu.help(name?)   this, or about name, eg help('sonamu.set_setting'), help('code_editor.save')
sonamu.complete(text): {string}   what could follow text, with signatures (like Tab), eg
                               sonamu.complete('context.focus():') lists what the focused node can do
context.focus(): Node          the innermost focused node
context.root(): Node           the root node
process.spawn(program, {args}?)
process.output(program, {args}?, dir?): string   waits for it (Ctrl+C kills it); stdout, then stderr
process.output(program, {args}?, dir?, fn(out, err))   doesn't wait (other Luau runs meanwhile): fn gets either
keymap.set({mods}, key, fn, {type = t}?)   a key (`ctrl`, `alt`, `shift`, `logo`; a character or evdev code),
                               global or while focus is of applet type t (wins); keymap.unset({mods}, key, {type}?)
events.on(event, fn)           `focus` (focus moved: the node), `open` (a new node), `close` (its gone node),
                               `window` (a wl window opened: its app id), `settings` (reloaded), `screens` (count)
~/.config/sonamu/init.luau     runs at start and on sonamu.reset(); `require('./name')` loads name.luau beside it
                               (own globals; it shares what it returns), from the buffer line too
clipboard.copy(text, {primary = true}?)   Sonamu's clipboard (or primary selection), for wl clients too
clipboard.paste({primary = true}?): string?
sonamu.open(applet, {args}?)   like `sonamu-msg open`
sonamu.reload_settings()
sonamu.setting(path): any      a setting (yours, else the default), path dotted, eg 'border.width'
sonamu.set_setting(path, value)   writes it to settings.json (only if valid; the default removes it); reloads
settings                       the settings as fields, eg settings.border.width = 5 is set_setting('border.width', 5);
                               an object is another `settings` (tostring: JSON; sonamu.setting gives a table)
sonamu.traverse(kind, n?)      moves focus like a `traverse` keybind: next, prev, parent, child n (from 1),
                               recent_child, last_child, sibling offset, select, enter
sonamu.bar_layout({left}, {right})   the status bar's segments by name, each left to right; default
                               {tree, launcher, time}, {tray, bell, resources, hub}
sonamu.quit()
sonamu.reset()                 a fresh Luau state
sonamu.messages(): string      prints and errors of init.luau, bindings and hooks (the last shows at the bottom)
sonamu.definitions(): string   luau-lsp definitions of all this, eg `sonamu-msg eval 'sonamu.definitions()'`
Node: close() focus() add_child(applet, {args}?): Node swap_sibling(offset) toggle_half() wrap_project()
      parent(): Node? children(): {Node} type(): string title(): string cast(type): Node?
      inner(): Node? (the root of a project's own tree)
      (also <type>.from_node(n), eg file_manager.from_node(n): typed in sonamu.definitions())
      spawn(program, {args}?) (in its tree's directory) spawn_sibling(program) (its window after the node) quit()
      any other method is a command of its applet, eg hub:clear()
Example: context.focus():add_child('file_manager', {'/tmp'}):focus()
Effects (`sonamu-msg eval --max <effect>` refuses higher): read < modify < destructive (close, quit, reset) < spawn
      (processes, keymap.set, events.on); applet commands show theirs in [ ]
Super+Esc: the buffer line (Esc leaves). There and in the hub: Tab completes, Up/Down recalls, Shift+Enter adds a line, PageUp/PageDown scrolls, Esc clears, Ctrl+V pastes, Ctrl+Shift+C copies the last output, Ctrl+C cancels.
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
    function spawn_sibling(self, program: string): ()
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
declare clipboard: {
    copy: (text: string, options: { primary: boolean? }?) -> boolean,
    paste: (options: { primary: boolean? }?) -> string?,
}
declare keymap: {
    set: (mods: {string}, key: string | number, fn: () -> (), options: { type: string? }?) -> (),
    unset: (mods: {string}, key: string | number, options: { type: string? }?) -> boolean,
}
declare events: { on: (event: string, fn: (...any) -> ()) -> () }
declare sonamu: {
    open: (applet: string, args: {string}?) -> (),
    quit: () -> (),
    reload_settings: () -> (),
    traverse: (kind: string, n: number?) -> (),
    complete: (text: string) -> {string},
    setting: (path: string) -> any,
    set_setting: (path: string, value: any) -> (),
    bar_layout: (left: {string}, right: {string}) -> (),
    reset: () -> (),
    messages: () -> string,
    help: (name: string?) -> string,
    definitions: () -> string,
}
declare function help(name: string?): string
declare function require(path: string): any
",
    );
    let _ = writeln!(
        out,
        "declare settings: {}",
        luau_type(&settings_file::defaults())
    );
    out
}

/// The Luau type of a default setting (`any` where it may be anything, eg `null`)
fn luau_type(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Bool(_) => "boolean".to_owned(),
        serde_json::Value::Number(_) => "number".to_owned(),
        serde_json::Value::String(_) => "string".to_owned(),
        serde_json::Value::Object(map) => format!(
            "{{ {} }}",
            map.iter()
                .filter(|(key, _)| !key.starts_with('$'))
                .map(|(key, value)| format!("{key}: {}", luau_type(value)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        _ => "any".to_owned(),
    }
}

/// `HELP`, then each applet type's commands (also the agent's Luau reference)
pub fn help() -> String {
    let mut help = HELP.to_string();
    for (applet_type, signatures) in command::all_signatures() {
        if signatures.is_empty() {
            continue;
        }
        let _ = write!(help, "\n{applet_type}:");
        for signature in signatures {
            let _ = write!(help, "\n  {}", signature_line(&signature));
        }
    }
    help
}

/// What `name` is: an agent tool, a Luau function (eg `sonamu.open`), a node method (`Node:close`) or an applet
/// command (`code_editor.save`); every match
#[must_use]
pub fn describe(name: &str) -> Option<String> {
    let mut found: Vec<String> = tools::tool_text(name).into_iter().collect();
    let (owner, method) = match name.rfind(['.', ':']) {
        Some(index) => (Some(&name[..index]), &name[index + 1..]),
        None => (None, name),
    };
    if matches!(owner, None | Some("Node")) {
        found.extend(node_method_line(method).map(|line| format!("Node:{line}")));
    }
    if let Some(applet_type) = owner {
        found.extend(
            command::signatures(applet_type)
                .iter()
                .find(|signature| signature.name == method)
                .map(|signature| format!("{applet_type}:{}", signature_line(signature))),
        );
    }
    found.extend(help_lines(name));
    (!found.is_empty()).then(|| found.join("\n\n"))
}

/// `HELP`'s lines about `name` (eg `sonamu.open`): those starting with it, and the indented lines after them
fn help_lines(name: &str) -> Option<String> {
    let mut found = Vec::new();
    let mut taking = false;
    for line in HELP.lines() {
        if line.starts_with(' ') {
            if taking {
                found.push(line.trim());
            }
            continue;
        }
        taking = line
            .strip_prefix(name)
            .is_some_and(|rest| rest.starts_with(['(', ' ', ':']));
        if taking {
            found.push(line);
        }
    }
    (!found.is_empty()).then(|| found.join("\n"))
}

/// A node method's signature from `NODE_METHODS`, eg `title(): string`
fn node_method_line(name: &str) -> Option<String> {
    let line = NODE_METHODS
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with(&format!("function {name}(")))?;
    Some(
        line.trim_start_matches("function ")
            .replace("(self, ", "(")
            .replace("(self)", "()"),
    )
}

/// An applet command's signature, eg `save(): nil  [modify] Saves the file`
fn signature_line(signature: &command::Signature) -> String {
    format!(
        "{}({}): {}  [{}] {}",
        signature.name,
        signature.params,
        signature.returns,
        signature.effect.name(),
        signature.doc
    )
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
    let result: CommandResult = command::call_limited(uid, name, args, max_effect(), &cancelled);
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
    candidates(lua, text)
        .into_iter()
        .map(|candidate| candidate.name)
        .collect()
}

/// `completions` with their signatures where known, eg `title(): string`
fn signed_completions(lua: &Lua, text: &str) -> Vec<String> {
    candidates(lua, text)
        .into_iter()
        .map(|candidate| candidate.signature.unwrap_or(candidate.name))
        .collect()
}

struct Candidate {
    name: String,
    signature: Option<String>,
}

fn candidates(lua: &Lua, text: &str) -> Vec<Candidate> {
    // the trailing chain, eg `context.focus():ti`
    let start = text
        .char_indices()
        .rev()
        .find(|&(_, c)| !(c.is_alphanumeric() || "_.:()".contains(c)))
        .map_or(0, |(index, c)| index + c.len_utf8());
    let chain = &text[start..];
    let unsigned = |names: Vec<String>| {
        names
            .into_iter()
            .map(|name| Candidate {
                name,
                signature: None,
            })
            .collect()
    };
    // right after a call: its separator, so the next Tab lists what follows
    if chain.ends_with(')') {
        return unsigned(match resolve(lua, chain) {
            Some(Resolved::Node) => vec![":".to_owned()],
            Some(Resolved::Value(LuaValue::UserData(node))) if node.is::<Node>() => {
                vec![":".to_owned()]
            }
            Some(Resolved::Value(LuaValue::Table(_))) => vec![".".to_owned()],
            _ => Vec::new(),
        });
    }
    let (receiver, separator, word) = match chain.rfind(['.', ':']) {
        Some(index) => (&chain[..index], &chain[index..=index], &chain[index + 1..]),
        None => ("", "", chain),
    };
    let first_help_line = |name: &str| help_lines(name)?.lines().next().map(str::to_owned);
    let mut candidates: Vec<Candidate> = if receiver.is_empty() {
        global_names(lua)
            .into_iter()
            .map(|name| Candidate {
                signature: first_help_line(&name),
                name,
            })
            .collect()
    } else {
        let applet_type = match (resolve(lua, receiver), separator) {
            (Some(Resolved::Node), ":") => Some(None),
            (Some(Resolved::Value(LuaValue::UserData(node))), ":") if node.is::<Node>() => {
                Some(match node_call(&node, "type") {
                    Some(Value::String(applet_type)) => Some(applet_type),
                    _ => None,
                })
            }
            (Some(Resolved::Value(LuaValue::UserData(settings))), ".")
                if settings.is::<Settings>() =>
            {
                let keys = settings
                    .borrow::<Settings>()
                    .map(|s| s.keys())
                    .unwrap_or_default();
                return finish(unsigned(keys), word);
            }
            (Some(Resolved::Value(LuaValue::Table(table))), ".") => {
                return finish(
                    keys(&table)
                        .into_iter()
                        .map(|name| Candidate {
                            signature: first_help_line(&format!("{receiver}.{name}")),
                            name,
                        })
                        .collect(),
                    word,
                );
            }
            _ => None,
        };
        match applet_type {
            None => Vec::new(),
            Some(applet_type) => {
                let mut methods: Vec<Candidate> = node_methods()
                    .into_iter()
                    .map(|name| Candidate {
                        signature: node_method_line(&name),
                        name,
                    })
                    .collect();
                if let Some(applet_type) = applet_type {
                    methods.extend(command::signatures(&applet_type).iter().map(|signature| {
                        Candidate {
                            name: signature.name.to_string(),
                            signature: Some(signature_line(signature)),
                        }
                    }));
                }
                methods
            }
        }
    };
    candidates = finish(candidates, word);
    candidates
}

/// Those starting with `word`, sorted, once each
fn finish(mut candidates: Vec<Candidate>, word: &str) -> Vec<Candidate> {
    candidates.retain(|candidate| candidate.name.starts_with(word));
    candidates.sort_by(|a, b| a.name.cmp(&b.name));
    candidates.dedup_by(|a, b| a.name == b.name);
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
            (Some(Resolved::Value(LuaValue::UserData(settings))), Some('.'), false)
                if settings.is::<Settings>() =>
            {
                // its fields come from the defaults too, so no need to read it
                let path = settings.borrow::<Settings>().ok()?.path(name);
                Resolved::Value(LuaValue::UserData(
                    lua.create_userdata(Settings(path)).ok()?,
                ))
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
        assert_eq!(complete_all("context.focus()"), [":"]);
        assert_eq!(complete_all("string.le"), ["len"]);
        assert_eq!(
            complete_all("context.focus():title():"),
            Vec::<String>::new()
        );
        assert!(eval_all("my_var = 1").1.is_ok());
        assert_eq!(complete_all("my_v"), ["my_var"]);
        assert_eq!(complete_all("settings.bor"), ["border"]);
        assert_eq!(complete_all("settings.border.wi"), ["width"]);
        // the fake root is a command hub (its `type` answer is `"type []"`, declared below)
        command::declare(
            "type []",
            &[command::Signature {
                name: "set_title",
                params: "title: string",
                returns: "nil",
                effect: Effect::Modify,
                doc: "",
            }],
        );
        assert_eq!(complete_all("context.root():set"), ["set_title"]);
        assert_eq!(
            complete_signed("context.root():set".to_string()),
            ["set_title(title: string): nil  [modify] "]
        );
        assert_eq!(
            complete_signed("context.focus():ti".to_string()),
            ["title(): string"]
        );
        assert_eq!(
            eval_all("sonamu.complete('sonamu.set_s')[1]").1,
            Ok(vec![Value::String(
                "sonamu.set_setting(path, value)   writes it to settings.json (only if valid; the default \
                 removes it); reloads"
                    .to_string()
            )])
        );
    }

    #[test]
    fn help_by_name() {
        assert!(describe("Node:close").unwrap().contains("Node:close(): ()"));
        let set_setting = describe("set_setting").unwrap();
        assert!(set_setting.contains("Tool set_setting"), "{set_setting}");
        assert!(
            set_setting.contains(r#"eg {"path": "gap""#),
            "{set_setting}"
        );
        let traverse = describe("sonamu.traverse").unwrap();
        assert!(
            traverse.contains("recent_child"),
            "continued lines: {traverse}"
        );
        assert_eq!(describe("nope"), None);
        assert_eq!(
            eval_all("help('sonamu.open')").1,
            Ok(vec![Value::String(
                "sonamu.open(applet, {args}?)   like `sonamu-msg open`".to_string()
            )])
        );
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
        // typed ones only for focus of their type (none yet)
        assert!(
            eval_all("keymap.set({'logo'}, 'q', function() end, {type = 'file_manager'})")
                .1
                .is_ok()
        );
        assert_eq!(binding(Some('q'), 0, logo), Some(id));
        *FOCUS_TYPE.lock().unwrap() = Some("file_manager".to_string());
        assert_ne!(binding(Some('q'), 0, logo), Some(id));
        *FOCUS_TYPE.lock().unwrap() = None;
        assert_eq!(
            eval_all("keymap.unset({'logo'}, 'q')").1,
            Ok(vec![Value::Bool(true)])
        );
        assert_eq!(binding(Some('q'), 0, logo), None);

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

    /// `require` and settings, together since both read `SONAMU_CONFIG`
    #[test]
    fn config_dir_modules_and_settings() {
        let dir = std::env::temp_dir().join(format!("sonamu-require-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("module.luau"),
            "module_global = 1\nreturn { get = function() return module_global end }",
        )
        .unwrap();
        // SAFETY: only this test reads `SONAMU_CONFIG` (no `start()` in tests)
        unsafe { std::env::set_var("SONAMU_CONFIG", &dir) };
        assert_eq!(
            eval_all("require('./module').get()").1,
            Ok(vec![Value::Number(1.0)])
        );
        assert_eq!(
            eval_all("module_global").1,
            Ok(vec![Value::Nil]),
            "modules have their own globals"
        );

        let file = dir.join("settings.json");
        let _ = std::fs::remove_file(&file);
        assert!(eval_all("sonamu.set_setting('border.width', 5)").1.is_ok());
        assert_eq!(
            eval_all("sonamu.setting('border.width')").1,
            Ok(vec![Value::Number(5.0)])
        );
        assert!(eval_all("settings.palette.fg = '#ffffff'").1.is_ok());
        assert_eq!(
            eval_all("settings.palette.fg, settings.border.width, settings.gap").1,
            Ok(vec![
                Value::String("#ffffff".to_string()),
                Value::Number(5.0),
                Value::Number(8.0)
            ])
        );
        assert!(eval_all("settings.palette.fg = '#dfdfdf'").1.is_ok());
        assert!(
            eval_all("sonamu.set_setting('gap', 'wide')").1.is_err(),
            "invalid settings aren't written"
        );
        assert!(
            eval_all("sonamu.set_setting('agent_url', 'http://x')")
                .1
                .is_ok()
        );
        assert!(eval_all("sonamu.set_setting('agent_url', nil)").1.is_ok());
        assert_eq!(
            eval_all("sonamu.setting('agent_url')").1,
            Ok(vec![Value::Nil])
        );
        let user: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(user, serde_json::json!({"border": {"width": 5}}));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn limited_evals() {
        let (_handle, outputs) =
            eval_limited("process.output('true')".to_string(), Effect::Read, || {});
        let Ok(Output::Done(Err(err))) = outputs.recv() else {
            panic!("refused");
        };
        assert!(err.contains("above this eval's limit"), "{err}");
        let (_handle, outputs) =
            eval_limited("limited_global = 1".to_string(), Effect::Read, || {});
        assert!(matches!(outputs.recv(), Ok(Output::Done(Ok(_)))));
        assert_eq!(
            eval_all("limited_global").1,
            Ok(vec![Value::Nil]),
            "its own env"
        );
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
                effect: Effect::Modify,
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
