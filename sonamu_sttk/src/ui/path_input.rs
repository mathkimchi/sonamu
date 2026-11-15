//! A path being typed (absolute or from `~`) with what it could complete to: the project overview's directory,
//! the settings editor's files (`file_chooser`)

use sonamu_ui::ui_event::{KeyModifiers, KeySymbol};
use std::path::{Path, PathBuf};

/// The home directory, which paths show as `~`
#[must_use]
pub fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// The path, with the home directory as `~`
#[must_use]
pub fn tilde(path: &Path) -> String {
    match home().and_then(|home| path.strip_prefix(home).ok().map(Path::to_path_buf)) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// The typed path with `~` expanded; `None` if it is relative
#[must_use]
pub fn expand(path: &str) -> Option<PathBuf> {
    if path == "~" {
        home()
    } else if let Some(rest) = path.strip_prefix("~/") {
        home().map(|home| home.join(rest))
    } else {
        path.starts_with('/').then(|| PathBuf::from(path))
    }
}

/// What the typed path could complete to: a path as typed
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathOption {
    pub path: String,
    pub dir: bool,
}
impl PathOption {
    /// Its last part
    #[must_use]
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
}

/// A typed path isn't an absolute directory
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotADirectory;

/// What a key did to the input
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKey {
    Ignored,
    Changed,
    Enter,
    Cancel,
}

#[derive(Debug, Clone)]
pub struct PathInput {
    pub path: String,
    /// What's in `path`'s parent and starts with its last part: directories, then (if it takes files) files
    pub options: Vec<PathOption>,
    /// The last Enter was on a path that isn't what it takes
    pub invalid: bool,
    /// The option picked with the arrows, which Tab fills in
    pub selected: Option<usize>,
    files: bool,
}
impl PathInput {
    /// Takes a directory
    #[must_use]
    pub fn directory(path: String) -> Self {
        Self::new(path, false)
    }

    /// Takes a file; directories are on the way
    #[must_use]
    pub fn file(path: String) -> Self {
        Self::new(path, true)
    }

    fn new(path: String, files: bool) -> Self {
        let mut input = Self {
            path: String::new(),
            options: Vec::new(),
            invalid: false,
            selected: None,
            files,
        };
        input.set(path);
        input
    }

    pub fn set(&mut self, path: String) {
        self.options = path
            .rsplit_once('/')
            .and_then(|(parent, prefix)| {
                // only the root's parent is empty
                let parent = if parent.is_empty() { "/" } else { parent };
                let join = if parent == "/" { "" } else { "/" };
                let entries = std::fs::read_dir(expand(parent)?).ok()?;
                let mut options: Vec<PathOption> = entries
                    .flatten()
                    .filter_map(|entry| {
                        let dir = entry.path().is_dir();
                        let name = entry.file_name().into_string().ok()?;
                        // hidden ones only once a `.` is typed
                        let shown = name.starts_with(prefix)
                            && (prefix.starts_with('.') || !name.starts_with('.'))
                            && (dir || self.files);
                        shown.then(|| PathOption {
                            path: format!("{parent}{join}{name}"),
                            dir,
                        })
                    })
                    .collect();
                options.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.path.cmp(&b.path)));
                Some(options)
            })
            .unwrap_or_default();
        self.path = path;
        self.invalid = false;
        self.selected = None;
    }

    /// Up to the parent directory, typed with a trailing `/`
    pub fn up(&mut self) {
        let trimmed = self.path.trim_end_matches('/');
        let parent = match trimmed.rsplit_once('/') {
            Some(("", _)) => "/".to_owned(),
            Some((parent, _)) => format!("{parent}/"),
            None => return,
        };
        self.set(parent);
    }

    /// Fills in the selected option, else completes to the options' longest common prefix,
    /// or into the only option once it is reached
    pub fn complete(&mut self) {
        if let Some(option) = self.selected.and_then(|index| self.options.get(index)) {
            self.set(option.path.clone());
            return;
        }
        let Some((first, rest)) = self.options.split_first() else {
            return;
        };
        if rest.is_empty() && first.path == self.path {
            if first.dir {
                self.set(format!("{}/", first.path));
            }
            return;
        }
        let common = rest.iter().fold(first.path.as_str(), |common, option| {
            let len = common
                .char_indices()
                .zip(option.path.chars())
                .take_while(|((_, a), b)| a == b)
                .last()
                .map_or(0, |((index, a), _)| index + a.len_utf8());
            &common[..len]
        });
        if common.len() > self.path.len() {
            self.set(common.to_string());
        }
    }

    /// The directory typed (`Ok(None)` if empty), or `Err` if the path isn't an absolute directory
    pub fn parse_directory(&self) -> Result<Option<PathBuf>, NotADirectory> {
        let path = self.path.trim();
        if path.is_empty() {
            return Ok(None);
        }
        expand(path)
            .and_then(|path| std::fs::canonicalize(path).ok())
            .filter(|path| path.is_dir())
            .map(Some)
            .ok_or(NotADirectory)
    }

    /// The file typed, if the path is an absolute file
    #[must_use]
    pub fn parse_file(&self) -> Option<PathBuf> {
        expand(self.path.trim()).filter(|path| path.is_file())
    }

    /// Typing, Backspace, Tab (complete), Enter, Esc; arrows are the caller's (they move `selected`)
    pub fn handle_key(&mut self, key: KeySymbol, modifiers: KeyModifiers) -> PathKey {
        match key {
            KeySymbol::Enter => return PathKey::Enter,
            KeySymbol::Escape => return PathKey::Cancel,
            KeySymbol::Backspace => {
                let mut path = self.path.clone();
                path.pop();
                self.set(path);
            }
            KeySymbol::Char('\t') if modifiers == KeyModifiers::NONE => self.complete(),
            KeySymbol::Char(key_char) if !key_char.is_control() => {
                let mut path = self.path.clone();
                path.push(key_char);
                self.set(path);
            }
            _ => return PathKey::Ignored,
        }
        PathKey::Changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sonamu-path-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        for file in ["sea.png", "sky.glsl", ".hidden"] {
            std::fs::write(dir.join(file), "").unwrap();
        }
        dir
    }

    #[test]
    fn files_and_directories() {
        let dir = temp("kinds");
        let typed = format!("{}/", dir.display());
        let names = |input: &PathInput| -> Vec<String> {
            input
                .options
                .iter()
                .map(|option| option.name().to_owned())
                .collect()
        };
        assert_eq!(names(&PathInput::directory(typed.clone())), ["sub"]);
        let mut files = PathInput::file(typed.clone());
        assert_eq!(names(&files), ["sub", "sea.png", "sky.glsl"]);
        files.set(format!("{typed}s"));
        files.complete();
        assert_eq!(files.path, typed.clone() + "s");
        files.set(format!("{typed}se"));
        files.complete();
        assert_eq!(files.parse_file(), Some(dir.join("sea.png")));
        files.up();
        assert_eq!(files.path, typed);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn completes_into_a_directory() {
        let dir = temp("into");
        let mut input = PathInput::file(format!("{}/su", dir.display()));
        input.complete();
        assert_eq!(input.path, format!("{}/sub", dir.display()));
        input.complete();
        assert_eq!(input.path, format!("{}/sub/", dir.display()));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
