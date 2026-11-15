//! Picks a file by walking folders: line 0 goes up, then the folder's subfolders and files (hidden ones left out),
//! each a list row of the applet showing it

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChooser {
    pub dir: PathBuf,
    /// Subfolders, then files, each by name
    entries: Vec<(String, bool)>,
}
impl FileChooser {
    /// In `start`'s folder if it's a file, else in `start`, else its nearest existing ancestor
    #[must_use]
    pub fn new(start: &Path) -> Self {
        let mut dir = if start.is_file() {
            start.parent().unwrap_or(start)
        } else {
            start
        };
        while !dir.is_dir()
            && let Some(parent) = dir.parent()
        {
            dir = parent;
        }
        let mut chooser = Self {
            dir: dir.to_owned(),
            entries: Vec::new(),
        };
        chooser.list();
        chooser
    }

    fn list(&mut self) {
        let mut entries: Vec<(String, bool)> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                (!name.starts_with('.')).then(|| (name, entry.path().is_dir()))
            })
            .collect();
        entries.sort_by(|(a, a_dir), (b, b_dir)| b_dir.cmp(a_dir).then_with(|| a.cmp(b)));
        self.entries = entries;
    }

    /// Its lines: up, then the entries
    #[must_use]
    pub fn lines(&self) -> usize {
        self.entries.len() + 1
    }

    /// Line `index` as shown: `..` with the folder it's in, folders ending in `/`
    #[must_use]
    pub fn label(&self, index: usize) -> String {
        match index
            .checked_sub(1)
            .and_then(|entry| self.entries.get(entry))
        {
            None => format!("‹ ..  ({})", self.dir.display()),
            Some((name, true)) => format!("{name}/"),
            Some((name, false)) => name.clone(),
        }
    }

    /// Enter on line `index`: goes into a folder (or up), or gives the file
    pub fn choose(&mut self, index: usize) -> Option<PathBuf> {
        match index
            .checked_sub(1)
            .and_then(|entry| self.entries.get(entry))
        {
            None => {
                if let Some(parent) = self.dir.parent() {
                    self.dir = parent.to_owned();
                }
            }
            Some((name, true)) => self.dir = self.dir.join(name),
            Some((name, false)) => return Some(self.dir.join(name)),
        }
        self.list();
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_folders_and_gives_files() {
        let dir = std::env::temp_dir().join(format!("sonamu-chooser-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        for file in ["b.png", "a.glsl", ".hidden", "sub/c.png"] {
            std::fs::write(dir.join(file), "").unwrap();
        }
        let mut chooser = FileChooser::new(&dir.join("a.glsl"));
        assert_eq!(chooser.dir, dir);
        let labels: Vec<String> = (1..chooser.lines())
            .map(|line| chooser.label(line))
            .collect();
        assert_eq!(labels, ["sub/", "a.glsl", "b.png"]);
        assert_eq!(chooser.choose(2), Some(dir.join("a.glsl")));
        assert_eq!(chooser.choose(1), None);
        assert_eq!(chooser.choose(1), Some(dir.join("sub/c.png")));
        chooser.choose(0);
        assert_eq!(chooser.dir, dir);
        // a missing file starts in its nearest folder
        assert_eq!(FileChooser::new(&dir.join("gone/x.png")).dir, dir);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
