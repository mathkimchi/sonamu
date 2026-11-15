//! The Keybinds section's model: groups of binds being edited, some maybe unfinished

use serde_json::{Map, Value, json};
use sonamu_common::{
    settings::{Action, Key, Keybind},
    settings_file,
};
use sonamu_sttk::{
    man_page,
    standard_keybinds::{ArgKind, KEYBIND_COMMANDS},
};

/// A bind being edited: written only once it's complete and valid (`Draft::to_json`)
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    pub mods: Vec<String>,
    /// `None` until recorded
    pub key: Option<Key>,
    pub action: DraftAction,
    /// A bind the editor can't edit (unknown command, args written otherwise, extra keys): written back as it was
    raw: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DraftAction {
    Command { name: String, args: Vec<Value> },
    Shell(String),
}

impl Draft {
    /// A bind of the setting; kept `raw` unless it would be written back the same
    fn from_json(value: &Value) -> Self {
        let parsed = serde_json::from_value::<Keybind>(value.clone())
            .ok()
            .map(|keybind| Self {
                mods: keybind.mods,
                key: Some(keybind.key),
                action: match keybind.action {
                    Action::Command { command, args } => DraftAction::Command {
                        name: command,
                        args,
                    },
                    Action::Shell { shell } => DraftAction::Shell(shell),
                },
                raw: None,
            });
        match parsed {
            Some(draft) if draft.to_json().as_ref() == Some(value) => draft,
            _ => Self {
                mods: Vec::new(),
                key: None,
                action: DraftAction::Shell(String::new()),
                raw: Some(value.clone()),
            },
        }
    }

    /// The bind as settings JSON, if it's complete: keys recorded, a known command with args it accepts, or a shell
    /// command
    #[must_use]
    pub fn to_json(&self) -> Option<Value> {
        if let Some(raw) = &self.raw {
            return Some(raw.clone());
        }
        let mut bind = Map::new();
        if !self.mods.is_empty() {
            bind.insert("mods".to_owned(), json!(self.mods));
        }
        bind.insert(
            "key".to_owned(),
            serde_json::to_value(self.key.as_ref()?).ok()?,
        );
        match &self.action {
            DraftAction::Command { name, args } => {
                if !arg_kind(name)?.accepts(args) {
                    return None;
                }
                bind.insert("command".to_owned(), json!(name));
                if !args.is_empty() {
                    bind.insert("args".to_owned(), json!(args));
                }
            }
            DraftAction::Shell(command) if command.trim().is_empty() => return None,
            DraftAction::Shell(command) => {
                bind.insert("shell".to_owned(), json!(command));
            }
        }
        Some(Value::Object(bind))
    }

    /// Whether the editor can change it
    #[must_use]
    pub fn editable(&self) -> bool {
        self.raw.is_none()
    }

    /// Its keys like the help page shows them, eg `Super+Shift+T`
    #[must_use]
    pub fn keys_label(&self) -> String {
        if let Some(raw) = &self.raw {
            return raw.to_string();
        }
        match &self.key {
            Some(key) => man_page::keys_label(&Keybind {
                mods: self.mods.clone(),
                key: key.clone(),
                action: Action::Shell {
                    shell: String::new(),
                },
            }),
            None => "no keys".to_owned(),
        }
    }

    /// The command's name, or `shell`
    #[must_use]
    pub fn action_name(&self) -> &str {
        if self.raw.is_some() {
            return "edit in settings.json";
        }
        match &self.action {
            DraftAction::Command { name, .. } => name,
            DraftAction::Shell(_) => SHELL,
        }
    }

    /// Its args (or shell command) as text to edit
    #[must_use]
    pub fn args_text(&self) -> String {
        if self.raw.is_some() {
            return String::new();
        }
        match &self.action {
            DraftAction::Command { name, args } => {
                arg_kind(name).map_or_else(|| json!(args).to_string(), |kind| args_text(kind, args))
            }
            DraftAction::Shell(command) => command.clone(),
        }
    }

    /// Whether its action takes typed args (or a shell command)
    #[must_use]
    pub fn takes_args(&self) -> bool {
        if self.raw.is_some() {
            return false;
        }
        match &self.action {
            DraftAction::Command { name, .. } => {
                arg_kind(name).is_some_and(|kind| kind != ArgKind::None)
            }
            DraftAction::Shell(_) => true,
        }
    }

    /// Sets its args (or shell command) from `text`; whether it was valid
    pub fn set_args_text(&mut self, text: &str) -> bool {
        match &mut self.action {
            DraftAction::Command { name, args } => {
                match arg_kind(name).and_then(|kind| parse_args(kind, text)) {
                    Some(parsed) => {
                        *args = parsed;
                        true
                    }
                    None => false,
                }
            }
            DraftAction::Shell(command) => {
                text.clone_into(command);
                true
            }
        }
    }

    /// Switches to the next (`forward`) or previous command, `shell` last; args reset to the command's simplest
    pub fn cycle_action(&mut self, forward: bool) {
        if self.raw.is_some() {
            return;
        }
        let names: Vec<&str> = KEYBIND_COMMANDS
            .iter()
            .map(|spec| spec.name)
            .chain([SHELL])
            .collect();
        let current = names
            .iter()
            .position(|name| *name == self.action_name())
            .unwrap_or(0);
        let next = names[sonamu_sttk::ui::fields::cycle(names.len(), current, forward)];
        self.action = match arg_kind(next) {
            Some(kind) => DraftAction::Command {
                name: next.to_owned(),
                args: default_args(kind),
            },
            None => DraftAction::Shell(String::new()),
        };
    }
}

/// The setting's key, for `settings_file::migrate`
const KEYBINDS: &str = "keybinds";

/// `cycle_action`'s name for a shell command
const SHELL: &str = "shell";

fn arg_kind(name: &str) -> Option<ArgKind> {
    KEYBIND_COMMANDS
        .iter()
        .find(|spec| spec.name == name)
        .map(|spec| spec.args)
}

/// The args a command starts with when picked (a program has to be typed)
fn default_args(kind: ArgKind) -> Vec<Value> {
    match kind {
        ArgKind::None | ArgKind::Program => Vec::new(),
        ArgKind::Offset => vec![json!(1)],
        ArgKind::Direction => vec![json!("left")],
        ArgKind::Traversal => vec![json!("next")],
    }
}

/// `args` as typed: words separated by spaces (a program is the whole text)
#[must_use]
pub fn args_text(kind: ArgKind, args: &[Value]) -> String {
    let words: Vec<String> = args
        .iter()
        .map(|arg| arg.as_str().map_or_else(|| arg.to_string(), str::to_owned))
        .collect();
    match kind {
        ArgKind::None => String::new(),
        _ => words.join(" "),
    }
}

/// `text` as args of `kind`, if they're valid
#[must_use]
pub fn parse_args(kind: ArgKind, text: &str) -> Option<Vec<Value>> {
    let text = text.trim();
    let args = match kind {
        ArgKind::None => Vec::new(),
        ArgKind::Program => vec![json!(text)],
        ArgKind::Offset => vec![json!(text.parse::<i64>().ok()?)],
        ArgKind::Direction => vec![json!(text)],
        ArgKind::Traversal => {
            let mut words = text.split_whitespace();
            let mut args = vec![json!(words.next()?)];
            args.extend(words.map(|word| {
                word.parse::<i64>()
                    .map_or_else(|_| json!(word), |n| json!(n))
            }));
            args
        }
    };
    (kind.accepts(&args) && !(kind == ArgKind::Program && text.is_empty())).then_some(args)
}

/// Every group, in order
#[derive(Debug, Clone, PartialEq)]
pub struct Keybinds {
    pub groups: Vec<(String, Vec<Draft>)>,
}
impl Keybinds {
    /// From the setting's JSON (binds that don't parse are dropped)
    #[must_use]
    pub fn from_json(value: &Value) -> Self {
        let mut value = Value::Object(Map::from_iter([(KEYBINDS.to_owned(), value.clone())]));
        settings_file::migrate(&mut value);
        let groups = value[KEYBINDS]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(name, binds)| {
                let binds = binds
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(Draft::from_json)
                    .collect();
                (name.clone(), binds)
            })
            .collect();
        Self { groups }
    }

    /// The setting's JSON: unfinished binds left out
    #[must_use]
    pub fn to_json(&self) -> Value {
        let groups = self
            .groups
            .iter()
            .map(|(name, binds)| {
                let binds: Vec<Value> = binds.iter().filter_map(Draft::to_json).collect();
                (name.clone(), Value::Array(binds))
            })
            .collect();
        Value::Object(groups)
    }

    /// Adds an empty group, named `Group N`; its index
    pub fn add_group(&mut self) -> usize {
        let name = (1..=self.groups.len() + 1)
            .map(|n| format!("Group {n}"))
            .find(|name| !self.has_group(name))
            .expect("some number is free");
        self.groups.push((name, Vec::new()));
        self.groups.len() - 1
    }

    /// Renames group `index`; false if `name` is empty or another group's
    pub fn rename_group(&mut self, index: usize, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty()
            || self
                .groups
                .iter()
                .enumerate()
                .any(|(other, (taken, _))| other != index && taken == name)
        {
            return false;
        }
        name.clone_into(&mut self.groups[index].0);
        true
    }

    /// Moves group `index` one up (`up`) or down; its new index
    pub fn move_group(&mut self, index: usize, up: bool) -> usize {
        let to = if up {
            index.saturating_sub(1)
        } else {
            (index + 1).min(self.groups.len() - 1)
        };
        self.groups.swap(index, to);
        to
    }

    pub fn delete_group(&mut self, index: usize) {
        self.groups.remove(index);
    }

    /// Adds an unfinished bind (no keys, running a shell command) at the end of `group`; its index
    pub fn add_bind(&mut self, group: usize) -> usize {
        let binds = &mut self.groups[group].1;
        binds.push(Draft {
            mods: Vec::new(),
            key: None,
            action: DraftAction::Shell(String::new()),
            raw: None,
        });
        binds.len() - 1
    }

    pub fn delete_bind(&mut self, (group, bind): (usize, usize)) {
        self.groups[group].1.remove(bind);
    }

    /// Moves a bind one up (`up`) or down, into the group before or after past its group's ends; where it went
    pub fn move_bind(&mut self, (group, bind): (usize, usize), up: bool) -> Option<(usize, usize)> {
        let len = self.groups[group].1.len();
        let to = match (up, bind) {
            (true, 0) => {
                let before = group.checked_sub(1)?;
                (before, self.groups[before].1.len())
            }
            (true, _) => (group, bind - 1),
            (false, _) if bind + 1 < len => (group, bind + 1),
            (false, _) => {
                let after = group + 1;
                (after < self.groups.len()).then_some((after, 0))?
            }
        };
        let draft = self.groups[group].1.remove(bind);
        self.groups[to.0].1.insert(to.1, draft);
        Some(to)
    }

    /// Appends the default groups that are missing (by name)
    pub fn add_defaults(&mut self) {
        for (name, binds) in Self::from_json(&settings_file::defaults()["keybinds"]).groups {
            if !self.has_group(&name) {
                self.groups.push((name, binds));
            }
        }
    }

    fn has_group(&self, name: &str) -> bool {
        self.groups.iter().any(|(taken, _)| taken == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sonamu_common::settings_file;

    fn defaults() -> Keybinds {
        Keybinds::from_json(&settings_file::defaults()["keybinds"])
    }

    fn names(keybinds: &Keybinds) -> Vec<&str> {
        keybinds
            .groups
            .iter()
            .map(|(name, _)| name.as_str())
            .collect()
    }

    #[test]
    fn round_trips_defaults() {
        assert_eq!(defaults().to_json(), settings_file::defaults()["keybinds"]);
    }

    #[test]
    fn unfinished_binds_are_left_out() {
        let mut keybinds = Keybinds::from_json(&json!({"G": []}));
        keybinds.add_bind(0);
        assert_eq!(keybinds.groups[0].1.len(), 1);
        assert_eq!(keybinds.to_json(), json!({"G": []}));
        keybinds.groups[0].1[0].key = Some(Key::Char('x'));
        keybinds.groups[0].1[0].action = DraftAction::Shell(String::new());
        assert_eq!(keybinds.to_json(), json!({"G": []}));
        keybinds.groups[0].1[0].action = DraftAction::Shell("true".to_owned());
        assert_eq!(
            keybinds.to_json(),
            json!({"G": [{"key": "x", "shell": "true"}]})
        );
    }

    #[test]
    fn bad_args_are_left_out() {
        let mut keybinds =
            Keybinds::from_json(&json!({"G": [{"key": "x", "command": "swipe", "args": ["up"]}]}));
        keybinds.groups[0].1[0].action = DraftAction::Command {
            name: "swipe".to_owned(),
            args: vec![json!("sideways")],
        };
        assert_eq!(keybinds.to_json(), json!({"G": []}));
    }

    #[test]
    fn move_bind_across_groups() {
        let mut keybinds = defaults();
        let first = keybinds.groups[0].1[0].clone();
        assert_eq!(keybinds.move_bind((0, 0), false), Some((0, 1)));
        assert_eq!(keybinds.groups[0].1[1], first);
        // past the first group's end: the next group's start
        let last = keybinds.groups[0].1.len() - 1;
        let moved = keybinds.groups[0].1[last].clone();
        assert_eq!(keybinds.move_bind((0, last), false), Some((1, 0)));
        assert_eq!(keybinds.groups[1].1[0], moved);
        assert_eq!(keybinds.move_bind((0, 0), true), None);
    }

    #[test]
    fn add_defaults_restores_missing_groups() {
        let mut keybinds = defaults();
        keybinds.delete_group(1);
        keybinds.groups[0].1.clear();
        keybinds.add_defaults();
        assert_eq!(
            names(&keybinds),
            ["Global", "Tree", "Traversal", "Swipe", "Windows"]
        );
        assert_eq!(keybinds.groups[0].1, Vec::new());
    }

    #[test]
    fn rename_group_keeps_position_and_rejects_duplicates() {
        let mut keybinds = defaults();
        assert!(keybinds.rename_group(1, "Apps"));
        assert_eq!(names(&keybinds)[1], "Apps");
        assert!(!keybinds.rename_group(1, "Global"));
        assert!(!keybinds.rename_group(1, " "));
    }

    #[test]
    fn new_group_names_are_unique() {
        let mut keybinds = Keybinds::from_json(&json!({}));
        keybinds.add_group();
        keybinds.add_group();
        assert_eq!(names(&keybinds), ["Group 1", "Group 2"]);
    }

    #[test]
    fn args_as_text() {
        assert_eq!(
            args_text(ArgKind::Traversal, &[json!("child"), json!(2)]),
            "child 2"
        );
        assert_eq!(
            parse_args(ArgKind::Traversal, "child 2"),
            Some(vec![json!("child"), json!(2)])
        );
        assert_eq!(parse_args(ArgKind::Traversal, "child"), None);
        assert_eq!(parse_args(ArgKind::Offset, "-1"), Some(vec![json!(-1)]));
        assert_eq!(
            parse_args(ArgKind::Program, "foot -e hx"),
            Some(vec![json!("foot -e hx")])
        );
        assert_eq!(parse_args(ArgKind::None, ""), Some(vec![]));
    }

    /// Binds it can't edit (unknown command, other args, extra keys) are written back as they were
    #[test]
    fn unknown_binds_kept() {
        let json = json!({"G": [
            {"key": "x", "command": "future_command"},
            {"key": "y", "command": "swap_sibling", "args": [1.0]},
            {"key": "z", "shell": "true", "comment": "mine"},
            {"key": "w", "shell": "true"}
        ]});
        let mut keybinds = Keybinds::from_json(&json);
        assert_eq!(keybinds.to_json(), json);
        assert!(!keybinds.groups[0].1[0].editable());
        assert!(keybinds.groups[0].1[3].editable());
        keybinds.add_group();
        assert_eq!(keybinds.to_json()["G"], json["G"]);
    }

    /// An old flat list (before groups) becomes one group
    #[test]
    fn flat_list_becomes_a_group() {
        let keybinds = Keybinds::from_json(&json!([{"key": "x", "shell": "true"}]));
        assert_eq!(
            keybinds.to_json(),
            json!({"Keybinds": [{"key": "x", "shell": "true"}]})
        );
    }

    #[test]
    fn keys_label() {
        let keybinds = Keybinds::from_json(
            &json!({"G": [{"mods": ["logo", "shift"], "key": "t", "shell": "x"}, {"key": 300, "shell": "y"}]}),
        );
        assert_eq!(keybinds.groups[0].1[0].keys_label(), "Super+Shift+T");
        assert_eq!(keybinds.groups[0].1[1].keys_label(), "key 300");
    }
}
