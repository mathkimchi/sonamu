//! The agent's tools (#155): Sonamu's always available functions, on the focused node, and `help`, `complete`,
//! `run_lua`
// TODO: relative and absolute node paths, not only the focused node

use crate::{Output, complete_signed, describe, eval, help, set_setting, setting};
use serde_json::{Value as Json, json};
use sonamu_command::{self as command, Value};
use std::fmt::Write as _;

struct Tool {
    name: &'static str,
    description: &'static str,
    params: &'static [Param],
    /// Only where it helps
    example: Option<&'static str>,
}

struct Param {
    name: &'static str,
    /// JSON schema type; `None`: any
    kind: Option<&'static str>,
    description: &'static str,
    required: bool,
}

const fn param(name: &'static str, kind: &'static str, description: &'static str) -> Param {
    Param {
        name,
        kind: Some(kind),
        description,
        required: true,
    }
}

const fn optional(name: &'static str, kind: &'static str, description: &'static str) -> Param {
    Param {
        required: false,
        ..param(name, kind, description)
    }
}

const ARGS: Param = optional("args", "array", "its arguments, strings");

const TOOLS: &[Tool] = &[
    Tool {
        name: "help",
        description: "An overview of Sonamu's tools and Luau API, or what `name` is",
        params: &[optional(
            "name",
            "string",
            "a tool, Luau function or applet command, eg `sonamu.set_setting`, `code_editor.save`",
        )],
        example: Some(r#"{"name": "code_editor.save"}"#),
    },
    Tool {
        name: "complete",
        description: "What could follow some Luau (like Tab), with signatures",
        params: &[param("text", "string", "Luau, eg `context.focus():`")],
        example: Some(r#"{"text": "context.focus():"} lists what the focused node can do"#),
    },
    Tool {
        name: "run_lua",
        description: "Runs Luau in Sonamu: its prints and return values. See help",
        params: &[param("code", "string", "Luau")],
        example: Some(r#"{"code": "return context.focus():title()"}"#),
    },
    Tool {
        name: "title",
        description: "The focused node's title",
        params: &[],
        example: None,
    },
    Tool {
        name: "type",
        description: "The focused node's applet type",
        params: &[],
        example: None,
    },
    Tool {
        name: "setting",
        description: "A setting (the user's, else the default)",
        params: &[param("path", "string", "dotted, eg `border.width`")],
        example: None,
    },
    Tool {
        name: "set_setting",
        description: "Writes a setting to settings.json (only if valid; the default removes it)",
        params: &[
            param("path", "string", "dotted, eg `border.width`"),
            Param {
                name: "value",
                kind: None,
                description: "its value",
                required: true,
            },
        ],
        example: Some(r#"{"path": "gap", "value": 8}"#),
    },
    Tool {
        name: "traverse",
        description: "Moves focus through the tree, like a `traverse` keybind",
        params: &[
            param(
                "kind",
                "string",
                "next, prev, parent, child (n from 1), recent_child, last_child, sibling (n: offset), select, \
                 enter",
            ),
            optional("n", "number", "for child and sibling"),
        ],
        example: Some(r#"{"kind": "child", "n": 1}"#),
    },
    Tool {
        name: "swap_sibling",
        description: "Swaps the focused node with a sibling",
        params: &[param("offset", "number", "eg -1: the previous one")],
        example: None,
    },
    Tool {
        name: "toggle_half",
        description: "Toggles the focused node between full and half width",
        params: &[],
        example: None,
    },
    Tool {
        name: "wrap_project",
        description: "Wraps the focused node in a new project",
        params: &[],
        example: None,
    },
    Tool {
        name: "add_child",
        description: "Adds an applet as the focused node's child",
        params: &[param("applet", "string", "its type"), ARGS],
        example: Some(r#"{"applet": "file_manager", "args": ["/tmp"]}"#),
    },
    Tool {
        name: "open",
        description: "Opens an applet, like `sonamu-msg open`",
        params: &[param("applet", "string", "its type"), ARGS],
        example: Some(r#"{"applet": "settings_editor"}"#),
    },
    Tool {
        name: "close",
        description: "Closes the focused node",
        params: &[],
        example: None,
    },
    Tool {
        name: "spawn",
        description: "Runs a program in the focused node's directory; its window goes in the tree",
        params: &[param("program", "string", "eg `alacritty`"), ARGS],
        example: Some(r#"{"program": "alacritty", "args": ["-e", "htop"]}"#),
    },
    Tool {
        name: "spawn_sibling",
        description: "Runs a program; its window goes after the focused node",
        params: &[param("program", "string", "eg `alacritty`")],
        example: None,
    },
];

/// The tools, for an OpenAI-compatible server's `tools`
#[must_use]
pub fn definitions() -> Json {
    TOOLS
        .iter()
        .map(|tool| {
            let properties: serde_json::Map<String, Json> = tool
                .params
                .iter()
                .map(|param| {
                    let mut schema = json!({"description": param.description});
                    if let Some(kind) = param.kind {
                        schema["type"] = kind.into();
                    }
                    if param.kind == Some("array") {
                        schema["items"] = json!({"type": "string"});
                    }
                    (param.name.to_owned(), schema)
                })
                .collect();
            let required: Vec<&str> = tool
                .params
                .iter()
                .filter(|param| param.required)
                .map(|param| param.name)
                .collect();
            json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": {"type": "object", "properties": properties, "required": required},
                },
            })
        })
        .collect()
}

/// The tool `name`: what it does, its params, an example
pub(crate) fn tool_text(name: &str) -> Option<String> {
    let tool = TOOLS.iter().find(|tool| tool.name == name)?;
    let mut text = format!("Tool {}: {}", tool.name, tool.description);
    for param in tool.params {
        let optional = if param.required { "" } else { ", optional" };
        let kind = param.kind.unwrap_or("any");
        let _ = write!(
            text,
            "\n  {}: {kind}{optional}: {}",
            param.name, param.description
        );
        if param.name == "applet" {
            let _ = write!(text, ": {}", applet_names().join(", "));
        }
    }
    if let Some(example) = tool.example {
        let _ = write!(text, "\n  eg {example}");
    }
    Some(text)
}

/// The applet types that can be opened by name: every declared one but `wl_surface` (programs' windows)
fn applet_names() -> Vec<String> {
    command::all_signatures()
        .into_keys()
        .filter(|name| name != "wl_surface")
        .collect()
}

/// A call as shown to the user, eg `traverse(child, 1)`
#[must_use]
pub fn shown(name: &str, args: &Json) -> String {
    let args: Vec<String> = TOOLS
        .iter()
        .find(|tool| tool.name == name)
        .map(|tool| tool.params)
        .unwrap_or_default()
        .iter()
        .filter_map(|param| match &args[param.name] {
            Json::Null => None,
            Json::String(text) => Some(text.clone()),
            value => Some(value.to_string()),
        })
        .collect();
    format!("{name}({})", args.join(", "))
}

/// Runs the tool `name` with the JSON object `args`: what to tell the model
#[must_use]
pub fn call(name: &str, args: &Json, cancelled: &dyn Fn() -> bool) -> String {
    run(name, args, cancelled).unwrap_or_else(|err| format!("error: {err}"))
}

fn run(name: &str, args: &Json, cancelled: &dyn Fn() -> bool) -> Result<String, String> {
    let tool = TOOLS
        .iter()
        .find(|tool| tool.name == name)
        .ok_or_else(|| format!("no tool {name}"))?;
    let text = |param: &str| {
        args[param]
            .as_str()
            .ok_or_else(|| format!("`{param}` must be a string"))
    };
    let mut positional: Vec<Value> = tool
        .params
        .iter()
        .map(|param| Value::from(args[param.name].clone()))
        .collect();
    while positional.last() == Some(&Value::Nil) {
        positional.pop();
    }
    Ok(match name {
        "help" => match args["name"].as_str() {
            Some(name) => describe_or_suggest(name),
            None => format!(
                "Tools: {}. They act on the focused node (move it with traverse); help(name) tells more. \
                 run_lua runs Luau:\n{}",
                TOOLS
                    .iter()
                    .map(|tool| tool.name)
                    .collect::<Vec<_>>()
                    .join(", "),
                help()
            ),
        },
        "complete" => {
            let candidates = complete_signed(text("text")?.to_owned());
            if candidates.is_empty() {
                "(nothing)".to_owned()
            } else {
                candidates.join("\n")
            }
        }
        "run_lua" => run_lua(text("code")?),
        "setting" => setting(text("path")?)?.to_string(),
        "set_setting" => {
            set_setting(text("path")?, args["value"].clone())?;
            "ok".to_owned()
        }
        "open" | "traverse" => result(command::call(None, name, positional, cancelled)?),
        _ => {
            let focus = command::focus(cancelled)?;
            let Value::Node(uid) = focus else {
                return Err(format!("unexpected focus {focus}"));
            };
            result(command::call(Some(uid), name, positional, cancelled)?)
        }
    })
}

fn describe_or_suggest(name: &str) -> String {
    describe(name)
        .unwrap_or_else(|| format!("Nothing is named {name}; complete lists what there is"))
}

fn result(value: Value) -> String {
    match value {
        Value::Nil | Value::Node(_) => "ok".to_owned(),
        value => value.to_string(),
    }
}

/// Runs `code` on the script thread, waiting: its prints and return values, or its error
fn run_lua(code: &str) -> String {
    let (_handle, outputs) = eval(code.to_owned(), || {});
    let mut text = String::new();
    for output in outputs {
        match output {
            Output::Print(line) => {
                text.push_str(&line);
                text.push('\n');
            }
            Output::Done(Ok(values)) => {
                let values: Vec<String> = values.iter().map(ToString::to_string).collect();
                text.push_str(&values.join(", "));
                break;
            }
            Output::Done(Err(err)) => {
                let _ = write!(text, "error: {err}");
                break;
            }
        }
    }
    if text.is_empty() {
        "(nothing)".to_owned()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definitions_are_unique_and_described() {
        let definitions = definitions();
        let names: Vec<&str> = definitions
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["function"]["name"].as_str().unwrap())
            .collect();
        let mut unique = names.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), names.len());
        for name in names {
            assert!(tool_text(name).is_some(), "{name}");
        }
        assert_eq!(
            definitions[2]["function"]["parameters"]["required"],
            json!(["code"])
        );
    }

    #[test]
    fn shown_and_errors() {
        assert_eq!(
            shown("traverse", &json!({"kind": "child", "n": 1})),
            "traverse(child, 1)"
        );
        assert_eq!(shown("close", &Json::Null), "close()");
        assert_eq!(call("nope", &json!({}), &|| false), "error: no tool nope");
        assert!(call("help", &json!({"name": "title"}), &|| false).contains("Tool title"));
        command::declare("file_manager", &[]);
        command::declare("wl_surface", &[]);
        let open = call("help", &json!({"name": "open"}), &|| false);
        assert!(
            open.contains("its type: ") && open.contains("file_manager"),
            "{open}"
        );
        assert!(!open.contains("wl_surface"), "{open}");
        assert!(call("help", &json!({}), &|| false).starts_with("Tools: help, complete"));
        assert_eq!(call("run_lua", &json!({"code": "1 + 1"}), &|| false), "2");
    }
}
