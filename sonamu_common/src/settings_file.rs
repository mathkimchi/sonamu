//! Editing the user's `settings.json` (the settings editor, #107): only what differs from the defaults is kept

use crate::settings::{DEFAULTS, LoadError, is_executable, parse};
use serde_json::{Map, Value};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// Where the settings come from
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// `settings.json` (it may not exist yet)
    Json(PathBuf),
    /// An executable `settings`: its output, so nothing can be edited
    Program(PathBuf),
}

/// The settings' source in the config dir `dir`, like `settings::load`
#[must_use]
pub fn source(dir: &Path) -> Source {
    let program = dir.join("settings");
    if is_executable(&program) {
        Source::Program(program)
    } else {
        Source::Json(dir.join("settings.json"))
    }
}

/// The JSON at `path`; `None` if there is no file
pub fn read(path: &Path) -> Result<Option<Value>, LoadError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(serde_json::from_str(&text)?)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err.into()),
    }
}

/// Every setting's default
#[must_use]
pub fn defaults() -> Value {
    serde_json::from_str(DEFAULTS).expect("valid default settings")
}

/// The setting at `path`: the user's, else the default, else `Null`
#[must_use]
pub fn value_at(user: &Value, path: &[&str]) -> Value {
    find(user, path)
        .cloned()
        .or_else(|| find(&defaults(), path).cloned())
        .unwrap_or(Value::Null)
}

/// Sets the setting at `path` in `user`: removed (with objects it leaves empty) if it's the default
pub fn set(user: &mut Value, path: &[&str], value: Value) {
    if find(&defaults(), path).is_some_and(|default| same(default, &value)) {
        remove(user, path);
        return;
    }
    let mut at = user;
    for key in path {
        if !at.is_object() {
            *at = Value::Object(Map::new());
        }
        at = at
            .as_object_mut()
            .expect("just made an object")
            .entry(*key)
            .or_insert(Value::Null);
    }
    *at = value;
}

/// Whether `user` parses as settings
pub fn check(user: &Value) -> Result<(), LoadError> {
    parse(&user.to_string()).map(|_| ())
}

/// Writes `user` to `path` (through a temporary file, so the watcher never reads half of it); `None` deletes it
pub fn write(path: &Path, user: Option<&Value>) -> io::Result<()> {
    let Some(user) = user else {
        return match fs::remove_file(path) {
            Err(err) if err.kind() != io::ErrorKind::NotFound => Err(err),
            _ => Ok(()),
        };
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, format(user))?;
    fs::rename(temporary, path)
}

/// Pretty JSON (2 spaces), with arrays of plain values and each keybind on one line
#[must_use]
pub fn format(user: &Value) -> String {
    let mut out = String::new();
    pretty(&mut out, user, 0, Level::Top);
    out.push('\n');
    out
}

/// Where a value is, for `pretty`
#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    Top,
    Keybinds,
    Group,
    Other,
}

fn pretty(out: &mut String, value: &Value, indent: usize, level: Level) {
    let pad = " ".repeat(indent + 2);
    match value {
        Value::Object(map) if !map.is_empty() => {
            out.push_str("{\n");
            for (index, (key, value)) in map.iter().enumerate() {
                let inner = match level {
                    Level::Top if key == "keybinds" => Level::Keybinds,
                    Level::Keybinds => Level::Group,
                    _ => Level::Other,
                };
                out.push_str(&pad);
                out.push_str(&Value::from(key.as_str()).to_string());
                out.push_str(": ");
                pretty(out, value, indent + 2, inner);
                out.push_str(if index + 1 < map.len() { ",\n" } else { "\n" });
            }
            out.push_str(&" ".repeat(indent));
            out.push('}');
        }
        Value::Array(items)
            if !items.is_empty()
                && (level == Level::Group
                    || items.iter().any(|item| item.is_object() || item.is_array())) =>
        {
            out.push_str("[\n");
            for (index, item) in items.iter().enumerate() {
                out.push_str(&pad);
                if level == Level::Group {
                    out.push_str(&inline(item));
                } else {
                    pretty(out, item, indent + 2, Level::Other);
                }
                out.push_str(if index + 1 < items.len() { ",\n" } else { "\n" });
            }
            out.push_str(&" ".repeat(indent));
            out.push(']');
        }
        _ => out.push_str(&inline(value)),
    }
}

/// `value` on one line, spaced like the rest
fn inline(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let fields: Vec<String> = map
                .iter()
                .map(|(key, value)| format!("{}: {}", Value::from(key.as_str()), inline(value)))
                .collect();
            format!("{{{}}}", fields.join(", "))
        }
        Value::Array(items) => {
            let items: Vec<String> = items.iter().map(inline).collect();
            format!("[{}]", items.join(", "))
        }
        _ => value.to_string(),
    }
}

fn find<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(value, |at, key| at.get(key))
}

/// Removes `path` from `value`, and objects that become empty; whether `value` is now empty
fn remove(value: &mut Value, path: &[&str]) -> bool {
    let Some(map) = value.as_object_mut() else {
        return false;
    };
    match path {
        [] => {}
        [key] => {
            map.shift_remove(*key);
        }
        [key, rest @ ..] => {
            if map.get_mut(*key).is_some_and(|inner| remove(inner, rest)) {
                map.shift_remove(*key);
            }
        }
    }
    map.is_empty()
}

/// Equal, numbers by value (`2` is `2.0`) and objects in order
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b)
                    .all(|((a_key, a), (b_key, b))| a_key == b_key && same(a, b))
        }
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A fresh dir for one test
    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sonamu-settings-file-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn set_default_removes_key_and_empty_parent() {
        let mut user = json!({"border": {"width": 3.0}});
        set(
            &mut user,
            &["border", "width"],
            defaults()["border"]["width"].clone(),
        );
        assert_eq!(user, json!({}));
    }

    #[test]
    fn set_keeps_unknown_keys_and_order() {
        let mut user = json!({"$schema": "x", "watch": ["/a"], "gap": 4.0});
        set(&mut user, &["font_size"], json!(30.0));
        let keys: Vec<_> = user.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["$schema", "watch", "gap", "font_size"]);
    }

    #[test]
    fn set_creates_parents() {
        let mut user = json!({});
        set(&mut user, &["border", "width"], json!(5.0));
        assert_eq!(user, json!({"border": {"width": 5.0}}));
    }

    #[test]
    fn value_at_falls_back_to_default() {
        let user = json!({"gap": 4.0});
        assert_eq!(value_at(&user, &["gap"]), json!(4.0));
        assert_eq!(value_at(&user, &["radius"]), defaults()["radius"]);
        assert_eq!(value_at(&user, &["nope"]), Value::Null);
    }

    #[test]
    fn keybinds_one_per_line() {
        let user = json!({"keybinds": {"G": [{"key": "x", "shell": "true"}]}});
        let text = format(&user);
        assert!(
            text.contains("\n      {\"key\": \"x\", \"shell\": \"true\"}\n"),
            "{text}"
        );
        assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), user);
    }

    #[test]
    fn check_rejects_bad_keybind() {
        assert!(check(&json!({"keybinds": {"G": [{"key": "x"}]}})).is_err());
        assert!(check(&json!({"gap": 4.0})).is_ok());
    }

    #[test]
    fn write_then_read() {
        let path = temp_dir("write").join("sub").join("settings.json");
        write(&path, Some(&json!({"gap": 4.0}))).unwrap();
        assert_eq!(read(&path).unwrap(), Some(json!({"gap": 4.0})));
        write(&path, None).unwrap();
        assert!(!path.exists());
        assert_eq!(read(&path).unwrap(), None);
    }

    #[test]
    fn source_prefers_program() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("source");
        assert!(matches!(source(&dir), Source::Json(_)));
        let program = dir.join("settings");
        fs::write(&program, "#!/bin/sh\necho {}").unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(source(&dir), Source::Program(_)));
    }
}
