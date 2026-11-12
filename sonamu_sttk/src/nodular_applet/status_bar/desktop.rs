//! Apps from `.desktop` files (`applications/` in the data dirs), how to start them, and matching them to a query

use super::icons::data_dirs;
use std::{collections::HashSet, fs, path::Path};

#[derive(Debug, Clone)]
pub struct App {
    pub name: String,
    /// Icon name or path
    pub icon: String,
    /// The program and its arguments, field codes dropped
    pub exec: Vec<String>,
    pub terminal: bool,
}

/// Every shown app, by name (earlier data dirs win, like `$XDG_DATA_HOME` over the system's)
pub fn apps() -> Vec<App> {
    let mut seen = HashSet::new();
    let mut apps = Vec::new();
    for dir in data_dirs() {
        let Ok(entries) = fs::read_dir(dir.join("applications")) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(id) = path
                .file_name()
                .and_then(|name| name.to_str()?.strip_suffix(".desktop"))
                .map(str::to_string)
            else {
                continue;
            };
            if !seen.insert(id.clone()) {
                continue;
            }
            if let Some(app) = parse(&path, id) {
                apps.push(app);
            }
        }
    }
    apps.sort_by_key(|app| app.name.to_lowercase());
    apps
}

fn parse(path: &Path, id: String) -> Option<App> {
    let text = fs::read_to_string(path).ok()?;
    let mut in_entry = false;
    let (mut name, mut icon, mut exec, mut terminal, mut hidden, mut app) =
        (None, String::new(), None, false, false, false);
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "Name" => name = Some(value.trim().to_string()),
            "Icon" => icon = value.trim().to_string(),
            "Exec" => exec = Some(value.trim().to_string()),
            "Terminal" => terminal = value.trim() == "true",
            "NoDisplay" | "Hidden" => hidden |= value.trim() == "true",
            "Type" => app = value.trim() == "Application",
            _ => {}
        }
    }
    if hidden || !app {
        return None;
    }
    let exec = split_exec(&exec?);
    (!exec.is_empty()).then(|| App {
        name: name.unwrap_or(id),
        icon,
        exec,
        terminal,
    })
}

/// `Exec`'s words (quotes and backslashes undone), its `%` field codes dropped (no files are passed)
fn split_exec(exec: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut chars = exec.chars();
    let mut has_word = false;
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                has_word = true;
            }
            '\\' => word.extend(chars.next()),
            ' ' | '\t' if !quoted => {
                if has_word || !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
                has_word = false;
            }
            // %f, %u, %i, %c, ...: nothing to pass
            '%' => {
                if chars.next() == Some('%') {
                    word.push('%');
                }
            }
            c => word.push(c),
        }
    }
    if has_word || !word.is_empty() {
        words.push(word);
    }
    words.retain(|word| !word.is_empty());
    words
}

/// How well `name` matches `query` (higher is better); `None` if it doesn't: its start, a word's start, inside it,
/// or its letters in order
pub fn score(name: &str, query: &str) -> Option<u32> {
    if query.is_empty() {
        return Some(0);
    }
    let (name, query) = (name.to_lowercase(), query.to_lowercase());
    if name.starts_with(&query) {
        return Some(4);
    }
    if name
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word.starts_with(&query))
    {
        return Some(3);
    }
    if name.contains(&query) {
        return Some(2);
    }
    let mut letters = name.chars();
    query.chars().all(|q| letters.any(|c| c == q)).then_some(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_drops_field_codes_and_quotes() {
        assert_eq!(split_exec("firefox %u"), ["firefox"]);
        assert_eq!(
            split_exec(r#""/opt/My App/app" --flag=%% -x"#),
            ["/opt/My App/app", "--flag=%", "-x"]
        );
    }

    #[test]
    fn scores_prefix_over_word_over_inside_over_letters() {
        assert_eq!(score("Firefox", "fir"), Some(4));
        assert_eq!(score("GNU Image Manipulation", "ima"), Some(3));
        assert_eq!(score("Thunderbird", "derb"), Some(2));
        assert_eq!(score("File Manager", "fmgr"), Some(1));
        assert_eq!(score("Firefox", "zz"), None);
    }
}
