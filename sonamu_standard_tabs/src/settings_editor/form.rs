//! The editor's fields, from the settings' JSON schema, so a new setting shows up by itself

use serde_json::Value;

/// A group of fields under a title
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    pub title: String,
    pub fields: Vec<Field>,
}

/// One setting
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// Keys down to it, eg `["border", "width"]`
    pub path: Vec<String>,
    pub label: String,
    /// The schema's description
    pub help: String,
    pub kind: FieldKind,
}

/// How a setting is edited
#[derive(Debug, Clone, PartialEq)]
pub enum FieldKind {
    Toggle,
    /// Clamped to the schema's range; rounded if `integer`
    Number {
        min: Option<f64>,
        max: Option<f64>,
        integer: bool,
    },
    /// One of these strings
    Choice(Vec<String>),
    /// `#rrggbb` or `#rrggbbaa`
    Color,
    /// A string; empty is `null` if `nullable`
    Text {
        nullable: bool,
    },
    /// A list of strings, one per line
    TextList,
    /// Shown as JSON, edited in `settings.json`
    ReadOnly,
}

/// The fields of `schema` (`settings::schema()`): every color in "Colors" first, then a section per object setting
/// and "General" for the rest, in the schema's order. `keybinds` has its own section, not here.
#[must_use]
pub fn sections(schema: &Value) -> Vec<Section> {
    let defs = &schema["$defs"];
    let mut colors = Vec::new();
    let mut sections: Vec<Section> = Vec::new();
    let Some(properties) = schema["properties"].as_object() else {
        return Vec::new();
    };
    for (key, property) in properties {
        if key == "keybinds" {
            continue;
        }
        let resolved = resolve(property, defs);
        let (title, fields) = match resolved["properties"].as_object() {
            Some(inner) => (
                label(key),
                inner
                    .iter()
                    .map(|(inner_key, inner_property)| {
                        field(vec![key.clone(), inner_key.clone()], inner_property, defs)
                    })
                    .collect(),
            ),
            None => (
                "General".to_owned(),
                vec![field(vec![key.clone()], property, defs)],
            ),
        };
        let (color_fields, fields): (Vec<Field>, Vec<Field>) = fields
            .into_iter()
            .partition(|field| field.kind == FieldKind::Color);
        colors.extend(color_fields);
        if fields.is_empty() {
            continue;
        }
        match sections.iter_mut().find(|section| section.title == title) {
            Some(section) => section.fields.extend(fields),
            None => sections.push(Section { title, fields }),
        }
    }
    if !colors.is_empty() {
        sections.insert(
            0,
            Section {
                title: "Colors".to_owned(),
                fields: colors,
            },
        );
    }
    sections
}

fn field(path: Vec<String>, property: &Value, defs: &Value) -> Field {
    let resolved = resolve(property, defs);
    let help = property["description"]
        .as_str()
        .or_else(|| resolved["description"].as_str())
        .unwrap_or_default()
        .to_owned();
    let kind = kind(resolved, defs);
    let label = if kind == FieldKind::Color {
        label(&path.join(" "))
    } else {
        label(path.last().map_or("", String::as_str))
    };
    Field {
        path,
        label,
        help,
        kind,
    }
}

fn kind(schema: &Value, defs: &Value) -> FieldKind {
    if schema
        .get("pattern")
        .is_some_and(|pattern| pattern.as_str().is_some_and(|p| p.starts_with("^#")))
    {
        return FieldKind::Color;
    }
    if let Some(options) = choices(schema) {
        return FieldKind::Choice(options);
    }
    match &schema["type"] {
        Value::String(kind) => match kind.as_str() {
            "boolean" => FieldKind::Toggle,
            "number" | "integer" => FieldKind::Number {
                min: schema["minimum"].as_f64(),
                max: schema["maximum"].as_f64(),
                integer: kind == "integer",
            },
            "string" => FieldKind::Text { nullable: false },
            "array" if resolve(&schema["items"], defs)["type"] == "string" => FieldKind::TextList,
            _ => FieldKind::ReadOnly,
        },
        Value::Array(kinds)
            if kinds.len() == 2
                && kinds.contains(&"string".into())
                && kinds.contains(&"null".into()) =>
        {
            FieldKind::Text { nullable: true }
        }
        _ => FieldKind::ReadOnly,
    }
}

/// The strings an enum schema allows (`enum`, or `oneOf` of `const`s)
fn choices(schema: &Value) -> Option<Vec<String>> {
    let strings = |values: &Vec<Value>| -> Option<Vec<String>> {
        values
            .iter()
            .map(|value| value.as_str().map(str::to_owned))
            .collect()
    };
    if let Some(values) = schema["enum"].as_array() {
        return strings(values);
    }
    let consts: Vec<Value> = schema["oneOf"]
        .as_array()?
        .iter()
        .map(|option| option.get("const").cloned())
        .collect::<Option<_>>()?;
    strings(&consts)
}

/// `schema`'s definition if it's a `$ref`
fn resolve<'a>(schema: &'a Value, defs: &'a Value) -> &'a Value {
    match schema["$ref"].as_str() {
        Some(reference) => &defs[reference.trim_start_matches("#/$defs/")],
        None => schema,
    }
}

/// `key` as words: `font_size` is "Font size"
fn label(key: &str) -> String {
    let words = key.replace('_', " ");
    let mut chars = words.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sonamu_common::{settings, settings_file};

    fn field(path: &str) -> Field {
        sections(&settings::schema())
            .into_iter()
            .flat_map(|section| section.fields)
            .find(|field| field.path.join(".") == path)
            .unwrap_or_else(|| panic!("no field {path}"))
    }

    /// Paths of `value`'s leaves (non-objects), dot-joined
    fn leaves(value: &Value, prefix: &str, out: &mut Vec<String>) {
        match value.as_object() {
            Some(map) => {
                for (key, value) in map {
                    let path = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{prefix}.{key}")
                    };
                    leaves(value, &path, out);
                }
            }
            None => out.push(prefix.to_owned()),
        }
    }

    #[test]
    fn every_setting_has_a_field() {
        let mut paths = Vec::new();
        leaves(&settings_file::defaults(), "", &mut paths);
        let fields: Vec<String> = sections(&settings::schema())
            .into_iter()
            .flat_map(|section| section.fields)
            .map(|field| field.path.join("."))
            .collect();
        for path in paths
            .iter()
            .filter(|path| !path.starts_with("keybinds") && *path != "$schema")
        {
            assert!(fields.contains(path), "{path}");
        }
        assert!(!fields.iter().any(|path| path.starts_with("keybinds")));
    }

    #[test]
    fn kinds() {
        assert_eq!(field("always_frame").kind, FieldKind::Toggle);
        assert_eq!(
            field("font_size").kind,
            FieldKind::Number {
                min: Some(8.0),
                max: Some(64.0),
                integer: false
            }
        );
        assert_eq!(
            field("treeview.width").kind,
            FieldKind::Number {
                min: Some(10.0),
                max: Some(60.0),
                integer: true
            }
        );
        assert!(
            matches!(field("buffer_line").kind, FieldKind::Choice(options) if options.contains(&"always".to_owned()))
        );
        assert_eq!(field("palette.bg").kind, FieldKind::Color);
        assert_eq!(field("terminal").kind, FieldKind::Text { nullable: true });
        assert_eq!(field("startup").kind, FieldKind::TextList);
        assert_eq!(field("restore_programs").kind, FieldKind::ReadOnly);
        assert_eq!(field("treeview.floating_size").kind, FieldKind::ReadOnly);
    }

    #[test]
    fn labels_and_help() {
        let font = field("font_size");
        assert_eq!(font.label, "Font size");
        assert!(font.help.starts_with("Text height"));
        assert!(
            field("buffer_line")
                .help
                .starts_with("When the buffer line")
        );
    }

    #[test]
    fn colors_first() {
        let sections = sections(&settings::schema());
        assert_eq!(sections[0].title, "Colors");
        assert_eq!(sections[0].fields.len(), 7);
        assert!(sections.iter().all(|section| !section.fields.is_empty()));
    }
}
