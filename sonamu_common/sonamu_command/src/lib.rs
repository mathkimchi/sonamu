//! Commands on nodes, called from scripts (see `.claude/COMMANDS.md`).
//!
//! Every node has a `NodeUid`, unique across worlds (the root's tree and projects'). The registry maps it to the
//! world holding the node, which runs the `NodeCall` on its own thread.

use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    sync::{
        LazyLock, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};

/// Identifies a node in any world; kept when the node moves to another world
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct NodeUid(u64);
impl NodeUid {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}
impl Default for NodeUid {
    fn default() -> Self {
        Self::new()
    }
}

/// What commands take and return
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(untagged)]
pub enum Value {
    Nil,
    Bool(bool),
    Number(f64),
    String(String),
    List(Vec<Self>),
    /// A Luau table with string keys
    Map(BTreeMap<String, Self>),
    Node(NodeUid),
}
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nil => write!(f, "nil"),
            Self::Bool(value) => write!(f, "{value}"),
            Self::Number(value) => write!(f, "{value}"),
            Self::String(value) => write!(f, "{value:?}"),
            Self::List(values) => {
                write!(f, "{{")?;
                for (i, value) in values.iter().enumerate() {
                    let separator = if i == 0 { "" } else { ", " };
                    write!(f, "{separator}{value}")?;
                }
                write!(f, "}}")
            }
            Self::Map(values) => {
                write!(f, "{{")?;
                for (i, (key, value)) in values.iter().enumerate() {
                    let separator = if i == 0 { "" } else { ", " };
                    write!(f, "{separator}{key} = {value}")?;
                }
                write!(f, "}}")
            }
            Self::Node(NodeUid(uid)) => write!(f, "Node({uid})"),
        }
    }
}

impl Value {
    /// Its Luau type's name, for errors
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::Nil => "nil",
            Self::Bool(_) => "boolean",
            Self::Number(_) => "number",
            Self::String(_) => "string",
            Self::List(_) | Self::Map(_) => "table",
            Self::Node(_) => "Node",
        }
    }
}

pub type CommandResult = Result<Value, String>;

/// A command for a node (`uid`), or for the world itself (`None`)
pub struct NodeCall {
    pub uid: Option<NodeUid>,
    pub name: String,
    pub args: Vec<Value>,
    pub reply: mpsc::Sender<CommandResult>,
}

/// To a world's thread
pub type World = calloop::channel::Sender<NodeCall>;

static WORLDS: LazyLock<Mutex<HashMap<NodeUid, World>>> = LazyLock::new(Default::default);
static ROOT_WORLD: Mutex<Option<World>> = Mutex::new(None);

/// The node is in `world` now
pub fn register(uid: NodeUid, world: World) {
    WORLDS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(uid, world);
}

pub fn unregister(uid: NodeUid) {
    WORLDS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&uid);
}

/// The root's world, for calls with no node (`call(None, ..)`)
pub fn set_root_world(world: World) {
    *ROOT_WORLD.lock().unwrap_or_else(PoisonError::into_inner) = Some(world);
}

/// Runs the command in the node's world (the root's for `None`) and waits for it, until `cancelled`
pub fn call(
    uid: Option<NodeUid>,
    name: &str,
    args: Vec<Value>,
    cancelled: &dyn Fn() -> bool,
) -> CommandResult {
    let world = match uid {
        Some(uid) => WORLDS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&uid)
            .cloned(),
        None => ROOT_WORLD
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone(),
    };
    let world = world.ok_or_else(gone)?;
    let (reply, replied) = mpsc::channel();
    world
        .send(NodeCall {
            uid,
            name: name.to_string(),
            args,
            reply,
        })
        .map_err(|_| gone())?;
    loop {
        match replied.recv_timeout(Duration::from_millis(50)) {
            Ok(result) => return result,
            Err(mpsc::RecvTimeoutError::Timeout) if cancelled() => {
                return Err("cancelled".to_string());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            // dropped without an answer, eg the node closed meanwhile
            Err(mpsc::RecvTimeoutError::Disconnected) => return Err(gone()),
        }
    }
}

/// Runs the command in the node's world without waiting for it (eg a keybind); dropped if the node is gone
pub fn send(uid: NodeUid, name: &str, args: Vec<Value>) {
    let world = WORLDS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&uid)
        .cloned();
    if let Some(world) = world {
        // no one reads the answer
        let (reply, _) = mpsc::channel();
        let _ = world.send(NodeCall {
            uid: Some(uid),
            name: name.to_string(),
            args,
            reply,
        });
    }
}

/// The error for a node that closed or a world that ended
pub fn gone() -> String {
    "the node is gone".to_string()
}

/// The innermost focused node, across worlds: the root's focused node, then, while focus is inside its applet,
/// that applet's (`INNER_FOCUS`)
pub fn focus(cancelled: &dyn Fn() -> bool) -> CommandResult {
    let mut focus = call(None, "focus", Vec::new(), cancelled)?;
    loop {
        let Value::List(pair) = &focus else {
            return Err(format!("unexpected focus {focus}"));
        };
        let [node @ Value::Node(uid), Value::Bool(inside)] = pair.as_slice() else {
            return Err(format!("unexpected focus {focus}"));
        };
        if !inside {
            return Ok(node.clone());
        }
        match call(Some(*uid), INNER_FOCUS, Vec::new(), cancelled) {
            Ok(inner) => focus = inner,
            // an applet without an inner world
            Err(_) => return Ok(node.clone()),
        }
    }
}

/// Applet command answered by applets holding a world (eg projects): `{node, inside}` like the root's `focus`
pub const INNER_FOCUS: &str = "__inner_focus";

/// A command an applet doesn't have
pub fn unknown(name: &str) -> String {
    format!("no command `{name}`")
}

/// The argument at `index`, as a string
pub fn string_arg(args: &[Value], index: usize) -> Result<&str, String> {
    match args.get(index) {
        Some(Value::String(value)) => Ok(value),
        other => Err(format!(
            "argument {} must be a string, not {other:?}",
            index + 1
        )),
    }
}

/// The argument at `index`, a list of strings (`nil` or missing: empty)
pub fn strings_arg(args: &[Value], index: usize) -> Result<Vec<String>, String> {
    match args.get(index) {
        None | Some(Value::Nil) => Ok(Vec::new()),
        Some(Value::List(values)) => values
            .iter()
            .map(|value| match value {
                Value::String(value) => Ok(value.clone()),
                other => Err(format!(
                    "argument {} must hold strings, not {other}",
                    index + 1
                )),
            })
            .collect(),
        Some(other) => Err(format!(
            "argument {} must be a list, not {other}",
            index + 1
        )),
    }
}

/// The argument at `index`, as a number
pub fn number_arg(args: &[Value], index: usize) -> Result<f64, String> {
    match args.get(index) {
        Some(Value::Number(value)) => Ok(*value),
        other => Err(format!(
            "argument {} must be a number, not {other:?}",
            index + 1
        )),
    }
}

/// The node commands every node has (`Node` methods in scripts); an applet's can't use these names
pub const NODE_COMMANDS: [&str; 12] = [
    "close",
    "focus",
    "add_child",
    "swap_sibling",
    "toggle_half",
    "wrap_project",
    "parent",
    "children",
    "type",
    "title",
    "spawn",
    "quit",
];

/// An applet command, as declared by its spawner (`AppletSpawnerTrait::commands`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Signature {
    pub name: &'static str,
    /// Luau-style, eg `"title: string"`
    pub params: &'static str,
    /// Luau type, eg `"nil"`
    pub returns: &'static str,
    pub doc: &'static str,
}

static SIGNATURES: Mutex<BTreeMap<String, Vec<Signature>>> = Mutex::new(BTreeMap::new());

/// The commands of applets of `applet_type` (a registered spawner's name); ones named like a node command are
/// dropped
pub fn declare(applet_type: &str, signatures: &[Signature]) {
    let signatures = signatures
        .iter()
        .filter(|signature| {
            let clashes = NODE_COMMANDS.contains(&signature.name);
            if clashes {
                log::warn!(
                    "{applet_type}'s command `{}` is a node command; dropping it",
                    signature.name
                );
            }
            !clashes
        })
        .copied()
        .collect();
    SIGNATURES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(applet_type.to_string(), signatures);
}

/// What `declare` was given for `applet_type`
pub fn signatures(applet_type: &str) -> Vec<Signature> {
    SIGNATURES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(applet_type)
        .cloned()
        .unwrap_or_default()
}

/// Every applet type's declared commands
pub fn all_signatures() -> BTreeMap<String, Vec<Signature>> {
    SIGNATURES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// Checks `args` against `applet_type`'s declared `name` command (arity, and the Luau types `string`, `number`,
/// `boolean`, `{T}`, `T?`; others, eg `Node` or `any`, take anything but nil). Undeclared commands pass.
pub fn check(applet_type: &str, name: &str, args: &[Value]) -> Result<(), String> {
    let Some(signature) = signatures(applet_type)
        .into_iter()
        .find(|signature| signature.name == name)
    else {
        return Ok(());
    };
    let params: Vec<(&str, &str)> = signature
        .params
        .split(", ")
        .filter(|param| !param.is_empty())
        .map(|param| param.split_once(": ").unwrap_or((param, "any")))
        .collect();
    if args.len() > params.len() {
        return Err(format!(
            "`{name}({})` takes {} arguments, not {}",
            signature.params,
            params.len(),
            args.len()
        ));
    }
    for (index, (param, ty)) in params.iter().enumerate() {
        let arg = args.get(index).unwrap_or(&Value::Nil);
        if !matches_type(arg, ty) {
            return Err(format!(
                "`{name}`'s `{param}` must be a {ty} (got {})",
                arg.type_name()
            ));
        }
    }
    Ok(())
}

/// Whether `value` is of the Luau type `ty` (see `check`)
fn matches_type(value: &Value, ty: &str) -> bool {
    if let Some(inner) = ty.strip_suffix('?') {
        return *value == Value::Nil || matches_type(value, inner);
    }
    if let Some(element) = ty.strip_prefix('{').and_then(|ty| ty.strip_suffix('}')) {
        return match value {
            Value::List(values) => values.iter().all(|value| matches_type(value, element)),
            _ => false,
        };
    }
    match ty {
        "string" => matches!(value, Value::String(_)),
        "number" => matches!(value, Value::Number(_)),
        "boolean" => matches!(value, Value::Bool(_)),
        "nil" => *value == Value::Nil,
        "any" => true,
        _ => *value != Value::Nil,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_declared_signatures() {
        declare(
            "check_test",
            &[Signature {
                name: "open",
                params: "path: string, args: {string}?",
                returns: "nil",
                doc: "",
            }],
        );
        let path = Value::String("/tmp".to_string());
        assert_eq!(
            check("check_test", "open", std::slice::from_ref(&path)),
            Ok(())
        );
        assert_eq!(
            check(
                "check_test",
                "open",
                &[path.clone(), Value::List(vec![path.clone()])]
            ),
            Ok(())
        );
        assert_eq!(
            check("check_test", "open", &[]),
            Err("`open`'s `path` must be a string (got nil)".to_string())
        );
        assert!(
            check(
                "check_test",
                "open",
                &[path.clone(), Value::List(vec![Value::Nil])]
            )
            .is_err()
        );
        assert!(check("check_test", "open", &[path.clone(), Value::Nil, path]).is_err());
        assert_eq!(check("check_test", "undeclared", &[Value::Nil]), Ok(()));
    }
}
