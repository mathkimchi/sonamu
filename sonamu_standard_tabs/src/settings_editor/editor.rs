//! The editor's state: the user's settings JSON and its writes, apart from drawing

use super::{
    form::{Field, FieldKind, Section, sections},
    pastel::pastel,
};
use serde_json::Value;
use sonamu_common::{
    settings,
    settings_file::{self, Source},
};
use sonamu_ui::color::Color;
use std::path::Path;

pub struct Editor {
    source: Source,
    /// The file when the editor opened (`None`: there was none), for Revert; `Err` if it couldn't be read
    baseline: Result<Option<Value>, String>,
    /// The user's settings as last read or written
    pub user: Value,
    pub sections: Vec<Section>,
}
impl Editor {
    /// Reads the settings in the config dir `dir`
    #[must_use]
    pub fn open(dir: &Path) -> Self {
        let source = settings_file::source(dir);
        let baseline = match &source {
            Source::Json(path) => settings_file::read(path).map_err(|err| err.to_string()),
            Source::Program(_) => Ok(None),
        };
        let mut editor = Self {
            user: Value::Object(serde_json::Map::new()),
            baseline,
            source,
            sections: Vec::new(),
        };
        editor.reload();
        editor.refresh();
        editor
    }

    /// The settings program, while it gives the settings (nothing can be written)
    #[must_use]
    pub fn read_only(&self) -> Option<&Path> {
        match &self.source {
            Source::Program(path) => Some(path),
            Source::Json(_) => None,
        }
    }

    /// Reads the file again (eg after it changed on disk); keeps the last read if it can't
    pub fn reload(&mut self) {
        if let Ok(user) = self.read() {
            self.user = user;
            self.refresh();
        }
    }

    /// The fields again, for the current values (a variant's fields follow its kind)
    fn refresh(&mut self) {
        let sections = sections(&settings::schema(), &|path| self.value(path));
        self.sections = sections;
    }

    /// A field's value: the setting's, else the schema's default for it
    #[must_use]
    pub fn field_value(&self, field: &Field) -> Value {
        match self.value(&field.path) {
            Value::Null => field.default.clone().unwrap_or(Value::Null),
            value => value,
        }
    }

    /// Switches a variant field to its kind `name`
    pub fn commit_variant(&mut self, field: &Field, name: &str) -> Result<(), String> {
        let FieldKind::Variant(variants) = &field.kind else {
            return Err(format!("{} has no kinds", field.label));
        };
        let (_, value) = variants
            .iter()
            .find(|(variant, _)| variant == name)
            .ok_or_else(|| format!("{} has no kind {name}", field.label))?;
        self.commit(
            std::slice::from_ref(&field.path),
            std::slice::from_ref(value),
        )
    }

    /// The setting at `path`: the user's, else the default
    #[must_use]
    pub fn value(&self, path: &[String]) -> Value {
        settings_file::value_at(&self.user, &keys(path))
    }

    /// Sets each of `paths` to its value in `values`, on the file as it is now, and writes it if it's valid
    pub fn commit(&mut self, paths: &[Vec<String>], values: &[Value]) -> Result<(), String> {
        let path = self.json_path()?.to_owned();
        let mut user = self.read()?;
        settings_file::migrate(&mut user);
        for (at, value) in paths.iter().zip(values) {
            settings_file::set(&mut user, &keys(at), value.clone());
        }
        settings_file::check(&user).map_err(|err| err.to_string())?;
        settings_file::write(&path, Some(&user)).map_err(|err| err.to_string())?;
        self.user = user;
        self.refresh();
        Ok(())
    }

    /// Commits `text` typed into `field`: numbers clamped to its range, colors checked, empty text `null` where
    /// allowed
    pub fn commit_text(&mut self, field: &Field, text: &str) -> Result<(), String> {
        let text = text.trim();
        let value = match &field.kind {
            FieldKind::Number { min, max, integer } => {
                let mut number: f64 = text
                    .parse()
                    .ok()
                    .filter(|number: &f64| number.is_finite())
                    .ok_or_else(|| format!("{text:?} isn't a number"))?;
                number = number.clamp(min.unwrap_or(f64::MIN), max.unwrap_or(f64::MAX));
                if *integer {
                    Value::from(number.round() as i64)
                } else {
                    Value::from(number)
                }
            }
            FieldKind::Color => {
                text.parse::<Color>()
                    .map_err(|_| format!("{text:?} isn't a color like #a0cafd"))?;
                Value::from(text)
            }
            FieldKind::Text { nullable: true } if text.is_empty() => Value::Null,
            FieldKind::Text { .. } | FieldKind::Path => Value::from(text),
            _ => return Err(format!("{} can't be typed", field.label)),
        };
        self.commit(std::slice::from_ref(&field.path), &[value])
    }

    /// Puts the file back as it was when the editor opened
    pub fn revert(&mut self) -> Result<(), String> {
        let path = self.json_path()?.to_owned();
        let baseline = self.baseline.clone()?;
        settings_file::write(&path, baseline.as_ref()).map_err(|err| err.to_string())?;
        self.user = baseline.unwrap_or_else(|| Value::Object(serde_json::Map::new()));
        self.refresh();
        Ok(())
    }

    /// Sets the seven colors to `pastel(hue)`, in one write
    pub fn generate(&mut self, hue: f32) -> Result<(), String> {
        let (paths, values): (Vec<Vec<String>>, Vec<Value>) = pastel(hue)
            .into_iter()
            .map(|(path, color)| {
                (
                    path.iter().map(|&key| key.to_owned()).collect(),
                    Value::from(hex(color)),
                )
            })
            .unzip();
        self.commit(&paths, &values)
    }

    fn json_path(&self) -> Result<&Path, String> {
        match &self.source {
            Source::Json(path) => Ok(path),
            Source::Program(path) => {
                Err(format!("Settings come from a program: {}", path.display()))
            }
        }
    }

    /// The file now, or `{}` if there is none
    fn read(&self) -> Result<Value, String> {
        let path = self.json_path()?;
        settings_file::read(path)
            .map(|user| user.unwrap_or_else(|| Value::Object(serde_json::Map::new())))
            .map_err(|err| format!("{}: {err}", path.display()))
    }
}

/// `#rrggbb`, or `#rrggbbaa` if it isn't opaque
#[must_use]
pub fn hex(Color([r, g, b, a]): Color) -> String {
    if a == 0xFF {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

fn keys(path: &[String]) -> Vec<&str> {
    path.iter().map(String::as_str).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{fs, path::PathBuf};

    /// A fresh config dir for one test
    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sonamu-editor-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn file(dir: &Path) -> Value {
        serde_json::from_str(&fs::read_to_string(dir.join("settings.json")).unwrap()).unwrap()
    }

    fn field(editor: &Editor, path: &str) -> Field {
        editor
            .sections
            .iter()
            .flat_map(|section| &section.fields)
            .find(|field| field.path.join(".") == path)
            .unwrap()
            .clone()
    }

    #[test]
    fn commit_writes_diff() {
        let dir = temp_dir("diff");
        let mut editor = Editor::open(&dir);
        editor
            .commit(&[vec!["font_size".to_owned()]], &[json!(30.0)])
            .unwrap();
        assert_eq!(file(&dir), json!({"font_size": 30.0}));
    }

    #[test]
    fn commit_rereads_file() {
        let dir = temp_dir("reread");
        let mut editor = Editor::open(&dir);
        fs::write(dir.join("settings.json"), r#"{"gap": 4.0}"#).unwrap();
        editor
            .commit(&[vec!["font_size".to_owned()]], &[json!(30.0)])
            .unwrap();
        assert_eq!(file(&dir), json!({"gap": 4.0, "font_size": 30.0}));
    }

    #[test]
    fn revert_restores_baseline() {
        let dir = temp_dir("revert");
        let mut editor = Editor::open(&dir);
        editor
            .commit(&[vec!["gap".to_owned()]], &[json!(3.0)])
            .unwrap();
        editor.revert().unwrap();
        assert!(!dir.join("settings.json").exists());
    }

    #[test]
    fn script_source_is_read_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("script");
        let program = dir.join("settings");
        fs::write(&program, "#!/bin/sh\necho {}").unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        let mut editor = Editor::open(&dir);
        assert_eq!(editor.read_only(), Some(program.as_path()));
        assert!(
            editor
                .commit(&[vec!["gap".to_owned()]], &[json!(3.0)])
                .is_err()
        );
        assert!(!dir.join("settings.json").exists());
    }

    #[test]
    fn broken_file_is_not_overwritten() {
        let dir = temp_dir("broken");
        fs::write(dir.join("settings.json"), "{oops").unwrap();
        let mut editor = Editor::open(&dir);
        assert!(
            editor
                .commit(&[vec!["gap".to_owned()]], &[json!(3.0)])
                .is_err()
        );
        assert!(editor.revert().is_err());
        assert_eq!(
            fs::read_to_string(dir.join("settings.json")).unwrap(),
            "{oops"
        );
    }

    #[test]
    fn number_commit_clamps() {
        let dir = temp_dir("clamp");
        let mut editor = Editor::open(&dir);
        let font = field(&editor, "font_size");
        editor.commit_text(&font, "200").unwrap();
        assert_eq!(file(&dir), json!({"font_size": 64.0}));
        assert!(editor.commit_text(&font, "big").is_err());
        let width = field(&editor, "treeview.width");
        assert!(editor.commit_text(&width, "nan").is_err());
        assert!(editor.commit_text(&width, "inf").is_err());
        let width = field(&editor, "treeview.width");
        editor.commit_text(&width, "33.6").unwrap();
        assert_eq!(file(&dir)["treeview"]["width"], json!(34));
    }

    #[test]
    fn text_commits() {
        let dir = temp_dir("text");
        let mut editor = Editor::open(&dir);
        editor
            .commit_text(&field(&editor, "terminal"), "foot")
            .unwrap();
        assert_eq!(file(&dir)["terminal"], json!("foot"));
        editor.commit_text(&field(&editor, "terminal"), "").unwrap();
        assert_eq!(file(&dir), json!({}));
        assert!(
            editor
                .commit_text(&field(&editor, "palette.bg"), "red")
                .is_err()
        );
        editor
            .commit_text(&field(&editor, "palette.bg"), "#102030")
            .unwrap();
        assert_eq!(file(&dir)["palette"]["bg"], json!("#102030"));
    }

    /// A file from before keybind groups still takes edits: its flat list becomes one group
    #[test]
    fn flat_keybinds_migrate_on_commit() {
        let dir = temp_dir("flat");
        fs::write(
            dir.join("settings.json"),
            r#"{"keybinds": [{"key": "x", "shell": "true"}]}"#,
        )
        .unwrap();
        let mut editor = Editor::open(&dir);
        editor
            .commit(&[vec!["gap".to_owned()]], &[json!(3.0)])
            .unwrap();
        assert_eq!(
            file(&dir),
            json!({"keybinds": {"Keybinds": [{"key": "x", "shell": "true"}]}, "gap": 3.0})
        );
    }

    #[test]
    fn switching_kind_writes_it_and_shows_its_fields() {
        let dir = temp_dir("variant");
        let mut editor = Editor::open(&dir);
        let wallpaper = field(&editor, "wallpaper");
        editor.commit_variant(&wallpaper, "color").unwrap();
        assert_eq!(file(&dir), json!({"wallpaper": {"color": "#000000"}}));
        editor
            .commit_text(&field(&editor, "wallpaper.color"), "#102030")
            .unwrap();
        assert_eq!(file(&dir), json!({"wallpaper": {"color": "#102030"}}));
        editor.commit_variant(&wallpaper, "none").unwrap();
        assert_eq!(file(&dir), json!({}));
    }

    /// A field left out of a variant's value shows the schema's default (eg a shader's fps)
    #[test]
    fn field_values_fall_back_to_schema_defaults() {
        let dir = temp_dir("defaults");
        fs::write(
            dir.join("settings.json"),
            r#"{"wallpaper": {"shader": {"path": "x.glsl"}}}"#,
        )
        .unwrap();
        let editor = Editor::open(&dir);
        assert_eq!(
            editor.field_value(&field(&editor, "wallpaper.shader.fps")),
            json!(30)
        );
        assert_eq!(
            editor.field_value(&field(&editor, "wallpaper.shader.scale")),
            json!(1.0)
        );
        assert_eq!(
            editor.field_value(&field(&editor, "wallpaper.shader.path")),
            json!("x.glsl")
        );
    }

    #[test]
    fn generate_writes_seven_colors() {
        let dir = temp_dir("generate");
        let mut editor = Editor::open(&dir);
        editor.generate(200.0).unwrap();
        let written = file(&dir);
        assert_eq!(written["palette"].as_object().unwrap().len(), 4);
        assert_eq!(written["border"].as_object().unwrap().len(), 3);
    }
}
