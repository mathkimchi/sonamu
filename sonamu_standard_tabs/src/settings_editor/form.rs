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
    /// The schema's default, for a field the settings can leave out (eg a shader wallpaper's `fps`)
    pub default: Option<Value>,
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
    /// A file (schema `format: path`), picked with `ui::file_chooser` or typed
    Path,
    /// A list of strings, one per line
    TextList,
    /// Shown as JSON, edited in `settings.json`
    ReadOnly,
    /// One of several kinds (eg `wallpaper`: none, color, image, shader), each maybe with its own fields; those of
    /// the current kind follow this field. Each kind's name, and the setting's value when switched to it
    Variant(Vec<(String, Value)>),
}

/// The fields of `schema` (`settings::schema()`): every color in "Colors" first, then a section per object setting
/// and "General" for the rest, in the schema's order. `keybinds` has its own section, not here.
/// `value` gives a setting's current value, which picks a variant's fields.
#[must_use]
pub fn sections(schema: &Value, value: &dyn Fn(&[String]) -> Value) -> Vec<Section> {
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
        let fields: Vec<Field> = fields
            .into_iter()
            .flat_map(|field| {
                let current = with_variant_fields(&field, property, defs, value);
                std::iter::once(field).chain(current)
            })
            .collect();
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

/// A variant field's current kind's fields (none for other fields)
fn with_variant_fields(
    field: &Field,
    property: &Value,
    defs: &Value,
    value: &dyn Fn(&[String]) -> Value,
) -> Vec<Field> {
    if !matches!(field.kind, FieldKind::Variant(_)) {
        return Vec::new();
    }
    let current = value(&field.path);
    let name = current.as_str().map(str::to_owned).or_else(|| {
        current
            .as_object()
            .and_then(|map| map.keys().next().cloned())
    });
    let Some((name, Some(schema))) = name.and_then(|name| {
        variants(resolve(property, defs))?
            .into_iter()
            .find(|(variant, _)| *variant == name)
    }) else {
        return Vec::new();
    };
    let path: Vec<String> = field.path.iter().cloned().chain([name.clone()]).collect();
    if let Some(inner) = resolve(schema, defs)["properties"].as_object() {
        return inner
            .iter()
            .map(|(key, inner_property)| {
                let mut inner_path = path.clone();
                inner_path.push(key.clone());
                self::field(inner_path, inner_property, defs)
            })
            .collect();
    }
    let mut only = self::field(path, schema, defs);
    only.label = label(&name);
    vec![only]
}

/// A variant schema's kinds: a `oneOf` of `const` strings and objects with one property (named by it, with its
/// schema)
fn variants(schema: &Value) -> Option<Vec<(String, Option<&Value>)>> {
    schema["oneOf"]
        .as_array()?
        .iter()
        .map(
            |option| match (option["const"].as_str(), option["properties"].as_object()) {
                (Some(name), _) => Some((name.to_owned(), None)),
                (None, Some(properties)) if properties.len() == 1 => {
                    let (name, schema) = properties.iter().next()?;
                    Some((name.clone(), Some(schema)))
                }
                _ => None,
            },
        )
        .collect()
}

/// The value a variant setting takes when switched to kind `name`: its required fields at their simplest
#[must_use]
pub fn variant_value(schema: &Value, defs: &Value, name: &str) -> Value {
    match variants(resolve(schema, defs))
        .and_then(|variants| variants.into_iter().find(|(variant, _)| variant == name))
    {
        Some((_, Some(inner))) => Value::Object(serde_json::Map::from_iter([(
            name.to_owned(),
            simplest(inner, defs),
        )])),
        _ => Value::from(name),
    }
}

/// The simplest valid value of `schema`: required fields only, empty strings, black, minimums
fn simplest(schema: &Value, defs: &Value) -> Value {
    let schema = resolve(schema, defs);
    if kind(schema, defs) == FieldKind::Color {
        return Value::from("#000000");
    }
    match schema["type"].as_str() {
        Some("object") => {
            let required = schema["required"].as_array().cloned().unwrap_or_default();
            Value::Object(
                required
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|key| (key.to_owned(), simplest(&schema["properties"][key], defs)))
                    .collect(),
            )
        }
        Some("number" | "integer") => schema["minimum"].clone(),
        Some("boolean") => Value::Bool(false),
        _ => Value::from(""),
    }
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
    let default = property
        .get("default")
        .or_else(|| resolved.get("default"))
        .cloned();
    Field {
        path,
        label,
        help,
        default,
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
    if let Some(variants) = variants(schema) {
        return FieldKind::Variant(
            variants
                .into_iter()
                .map(|(name, _)| {
                    let value = variant_value(schema, defs, &name);
                    (name, value)
                })
                .collect(),
        );
    }
    match &schema["type"] {
        Value::String(kind) => match kind.as_str() {
            "boolean" => FieldKind::Toggle,
            "number" | "integer" => FieldKind::Number {
                min: schema["minimum"].as_f64(),
                max: schema["maximum"].as_f64(),
                integer: kind == "integer",
            },
            "string" if schema["format"] == "path" => FieldKind::Path,
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

    /// The defaults' value at a path
    fn defaults(path: &[String]) -> Value {
        let keys: Vec<&str> = path.iter().map(String::as_str).collect();
        settings_file::value_at(&settings_file::defaults(), &keys)
    }

    fn fields_with(wallpaper: Value) -> Vec<Field> {
        let value = move |path: &[String]| {
            if path.first().is_some_and(|key| key == "wallpaper") {
                let keys: Vec<&str> = path[1..].iter().map(String::as_str).collect();
                keys.iter()
                    .try_fold(&wallpaper, |at, key| at.get(key))
                    .cloned()
                    .unwrap_or(Value::Null)
            } else {
                defaults(path)
            }
        };
        sections(&settings::schema(), &value)
            .into_iter()
            .flat_map(|section| section.fields)
            .collect()
    }

    #[test]
    fn variants_choose_then_show_their_fields() {
        let paths = |fields: &[Field]| -> Vec<String> {
            fields
                .iter()
                .map(|field| field.path.join("."))
                .filter(|path| path.starts_with("wallpaper"))
                .collect()
        };
        let none = fields_with(serde_json::json!("none"));
        assert_eq!(paths(&none), ["wallpaper"]);
        let kind = none
            .iter()
            .find(|field| field.path == ["wallpaper"])
            .unwrap();
        let FieldKind::Variant(variants) = &kind.kind else {
            panic!("not a variant: {:?}", kind.kind);
        };
        let names: Vec<&str> = variants.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["none", "color", "image", "shader"]);
        let battery = none
            .iter()
            .find(|field| field.path == ["battery_wallpaper"])
            .unwrap();
        let FieldKind::Variant(variants) = &battery.kind else {
            panic!("not a variant: {:?}", battery.kind);
        };
        let names: Vec<&str> = variants.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["same", "none", "color", "image", "shader"]);
        let shader = fields_with(serde_json::json!({"shader": {"path": "x"}}));
        assert_eq!(
            paths(&shader),
            [
                "wallpaper",
                "wallpaper.shader.path",
                "wallpaper.shader.scale",
                "wallpaper.shader.fps"
            ]
        );
        let color = fields_with(serde_json::json!({"color": "#102030"}));
        let field = color
            .iter()
            .find(|field| field.path.join(".") == "wallpaper.color")
            .unwrap();
        assert_eq!(
            (field.kind.clone(), field.label.as_str()),
            (FieldKind::Color, "Color")
        );
        // files are paths, in either kind
        let kind_of = |fields: &[Field], path: &str| {
            fields
                .iter()
                .find(|field| field.path.join(".") == path)
                .map(|field| field.kind.clone())
        };
        assert_eq!(
            kind_of(&shader, "wallpaper.shader.path"),
            Some(FieldKind::Path)
        );
        let image = fields_with(serde_json::json!({"image": "x.png"}));
        assert_eq!(kind_of(&image, "wallpaper.image"), Some(FieldKind::Path));
    }

    #[test]
    fn variant_defaults() {
        let schema = settings::schema();
        let wallpaper = &schema["$defs"]["Wallpaper"];
        let defs = &schema["$defs"];
        assert_eq!(
            variant_value(wallpaper, defs, "none"),
            serde_json::json!("none")
        );
        assert_eq!(
            variant_value(wallpaper, defs, "color"),
            serde_json::json!({"color": "#000000"})
        );
        assert_eq!(
            variant_value(wallpaper, defs, "shader"),
            serde_json::json!({"shader": {"path": ""}})
        );
    }

    fn field(path: &str) -> Field {
        sections(&settings::schema(), &defaults)
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
        let fields: Vec<String> = sections(&settings::schema(), &defaults)
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
        let sections = sections(&settings::schema(), &defaults);
        assert_eq!(sections[0].title, "Colors");
        assert_eq!(sections[0].fields.len(), 7);
        assert!(sections.iter().all(|section| !section.fields.is_empty()));
    }
}
