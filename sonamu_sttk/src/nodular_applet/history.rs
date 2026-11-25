//! Closed nodes, to restore (#162, see `.claude/SESSIONS.md`)

use crate::session;
use serde::{Deserialize, Serialize};
use sonamu_common::{
    command::NodeUid,
    sap::session::SavedTree,
    settings::{self, HistoryMode},
};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, PoisonError},
};

/// A closed node, to restore
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct Closed {
    /// Unique among entries (set by `record`)
    pub id: u64,
    /// The node without its children; its `id` is its uid, kept on restore
    pub tree: SavedTree,
    /// Parent first, up to the root, across worlds (`complete_ancestors`)
    pub ancestors: Vec<NodeUid>,
    /// The sibling before it; `None`: it was the first child
    pub prev: Option<NodeUid>,
    /// Its children, which its parent adopted
    pub children: Vec<NodeUid>,
    /// A dissolved project's contents (its inner root's children), to wrap back in
    #[serde(default)]
    pub contents: Vec<NodeUid>,
    /// Closed together (Close with children); restored together
    #[serde(default)]
    pub group: Option<u64>,
}

struct History {
    /// Newest last
    entries: Vec<Closed>,
    /// Being restored: taken by the world of the node it goes under
    pending: BTreeMap<u64, Closed>,
    next_id: u64,
    /// Nodes about to be closed together to their group (not saved)
    grouping: BTreeMap<NodeUid, u64>,
    /// Since it was last saved
    changed: bool,
    /// Bumped on every change (`revision`)
    revision: u64,
}
impl History {
    fn touch(&mut self) {
        self.changed = true;
        self.revision += 1;
    }
}

static HISTORY: Mutex<History> = Mutex::new(History {
    entries: Vec::new(),
    pending: BTreeMap::new(),
    next_id: 1,
    grouping: BTreeMap::new(),
    changed: false,
    revision: 0,
});

fn history() -> MutexGuard<'static, History> {
    HISTORY.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Makes the closes of these nodes one group
pub fn group(uids: impl IntoIterator<Item = NodeUid>) {
    let mut history = history();
    let group = history.next_id;
    history.next_id += 1;
    history
        .grouping
        .extend(uids.into_iter().map(|uid| (uid, group)));
}

/// Remembers the closed node, unless the mode is `none`; the oldest go past `limit`
pub fn record(mut closed: Closed) {
    let settings = settings::get().history;
    if settings.mode == HistoryMode::None {
        return;
    }
    let mut history = history();
    closed.group = closed.tree.id.and_then(|uid| history.grouping.remove(&uid));
    closed.id = history.next_id;
    history.next_id += 1;
    history.entries.push(closed);
    let excess = history.entries.len().saturating_sub(settings.limit);
    history.entries.drain(..excess);
    history.touch();
}

/// The newest entry and the rest of its group, removed; newest first (the order to restore in)
pub fn pop_newest() -> Vec<Closed> {
    let mut history = history();
    history.touch();
    take_newest(&mut history.entries)
}

fn take_newest(entries: &mut Vec<Closed>) -> Vec<Closed> {
    entries
        .len()
        .checked_sub(1)
        .map_or_else(Vec::new, |newest| take_at(entries, newest))
}

/// The entry at `index` and the rest of its group, removed; newest first
fn take_at(entries: &mut Vec<Closed>, index: usize) -> Vec<Closed> {
    let Some(group) = entries[index].group else {
        return vec![entries.remove(index)];
    };
    let (mut same, rest): (Vec<_>, Vec<_>) = std::mem::take(entries)
        .into_iter()
        .partition(|entry| entry.group == Some(group));
    *entries = rest;
    same.reverse();
    same
}

/// The entry `id` and the rest of its group, removed; newest first (the order to restore in)
pub fn take(id: u64) -> Vec<Closed> {
    let mut history = history();
    let Some(index) = history.entries.iter().position(|entry| entry.id == id) else {
        return Vec::new();
    };
    history.touch();
    take_at(&mut history.entries, index)
}

/// Forgets the entry `id` and the rest of its group
pub fn remove(id: u64) {
    drop(take(id));
}

/// Forgets every entry
pub fn clear() {
    let mut history = history();
    history.entries.clear();
    history.touch();
}

/// Holds a popped entry until its world `take`s it
pub fn hold(entry: Closed) {
    history().pending.insert(entry.id, entry);
}

/// A held entry
pub fn take_pending(id: u64) -> Option<Closed> {
    history().pending.remove(&id)
}

/// Changes with every change of the entries, to tell when to redraw
pub fn revision() -> u64 {
    history().revision
}

pub fn entries() -> Vec<Closed> {
    history().entries.clone()
}

/// Extends the ancestors that stop at a world's root (eg a project's inner root) with those above it, up to `root`
/// (the root tree's), found in the root tree's saved form. Ones not found there (closed since) go straight to `root`.
pub fn complete_ancestors(root: NodeUid, tree: impl FnOnce() -> SavedTree) {
    let mut guard = history();
    let history = &mut *guard;
    let open = |entry: &Closed| entry.ancestors.last().is_some_and(|&last| last != root);
    if !history.entries.iter().any(open) {
        return;
    }
    let tree = tree();
    for entry in history.entries.iter_mut().filter(|entry| open(entry)) {
        let last = *entry.ancestors.last().unwrap();
        let mut above = uid_ancestors(&tree, last).unwrap_or_default();
        if above.last() != Some(&root) {
            above.push(root);
        }
        entry.ancestors.extend(above);
        history.changed = true;
        history.revision += 1;
    }
}

/// The uids above `uid`, nearest first, through applets' inner trees too; `None` if it isn't in the tree
fn uid_ancestors(tree: &SavedTree, uid: NodeUid) -> Option<Vec<NodeUid>> {
    if tree.id == Some(uid) {
        return Some(Vec::new());
    }
    let inner = match &tree.main {
        Some(sonamu_common::sap::session::SavedApplet::Spawner {
            inner: Some(inner), ..
        }) => Some(&**inner),
        _ => None,
    };
    inner.into_iter().chain(&tree.children).find_map(|child| {
        let mut above = uid_ancestors(child, uid)?;
        above.extend(tree.id);
        Some(above)
    })
}

/// `history.json` beside the session (`None`: no session file)
fn path() -> Option<PathBuf> {
    Some(session::session_path()?.with_file_name("history.json"))
}

/// Loads the file (`path`) if the mode is `full`, reserving the uids in it. One that can't be read is moved aside (`.bad`).
pub fn load() {
    let Some(path) = path() else {
        return;
    };
    if settings::get().history.mode != HistoryMode::Full {
        return;
    }
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return,
        Err(err) => {
            log::warn!("Couldn't read the history {}: {err}", path.display());
            return;
        }
    };
    let entries: Vec<Closed> = match serde_json::from_str(&text) {
        Ok(entries) => entries,
        Err(err) => {
            log::warn!("Couldn't parse the history {}: {err}", path.display());
            let _ = fs::rename(&path, path.with_extension("json.bad"));
            return;
        }
    };
    for entry in &entries {
        entry.tree.reserve_ids();
        for uid in entry
            .ancestors
            .iter()
            .chain(&entry.prev)
            .chain(&entry.children)
        {
            uid.reserve();
        }
    }
    let mut history = history();
    history.next_id = entries
        .iter()
        .map(|entry| entry.id)
        .max()
        .map_or(1, |max| max + 1);
    history.entries = entries;
    history.changed = false;
    history.revision += 1;
}

/// Writes the file (`path`) if the mode is `full` and the history changed, atomically (through a temporary file)
pub fn save() {
    let Some(path) = path() else {
        return;
    };
    if settings::get().history.mode != HistoryMode::Full {
        return;
    }
    let mut history = history();
    if !history.changed {
        return;
    }
    match write(&path, &history.entries) {
        Ok(()) => history.changed = false,
        Err(err) => log::warn!("Couldn't save the history to {}: {err}", path.display()),
    }
}

fn write(path: &Path, entries: &[Closed]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, serde_json::to_string_pretty(entries)?)?;
    fs::rename(temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sonamu_common::sap::session::SavedApplet;

    fn node(id: NodeUid, main: Option<SavedApplet>, children: Vec<SavedTree>) -> SavedTree {
        SavedTree {
            id: Some(id),
            main,
            children,
            ..SavedTree::default()
        }
    }

    /// Ancestors go through a spawner's inner tree too, nearest first
    #[test]
    fn uid_ancestors_goes_through_inner() {
        let [root, project, inner_root, leaf, other] = [(); 5].map(|()| NodeUid::new());
        let inner = node(inner_root, None, vec![node(leaf, None, Vec::new())]);
        let spawner = SavedApplet::Spawner {
            name: "project".to_string(),
            args: Vec::new(),
            inner: Some(Box::new(inner)),
        };
        let tree = node(root, None, vec![node(project, Some(spawner), Vec::new())]);
        assert_eq!(uid_ancestors(&tree, root), Some(Vec::new()));
        assert_eq!(
            uid_ancestors(&tree, leaf),
            Some(vec![inner_root, project, root])
        );
        assert_eq!(uid_ancestors(&tree, other), None);
    }

    fn entry(id: u64, group: Option<u64>) -> Closed {
        Closed {
            id,
            tree: SavedTree::default(),
            ancestors: Vec::new(),
            prev: None,
            children: Vec::new(),
            contents: Vec::new(),
            group,
        }
    }

    /// A group comes out together, newest first; others stay
    #[test]
    fn take_newest_takes_the_group() {
        let mut entries = vec![
            entry(1, Some(9)),
            entry(2, None),
            entry(3, Some(9)),
            entry(4, Some(9)),
        ];
        let ids = |entries: &[Closed]| entries.iter().map(|entry| entry.id).collect::<Vec<_>>();
        assert_eq!(ids(&take_newest(&mut entries)), [4, 3, 1]);
        assert_eq!(ids(&entries), [2]);
        assert_eq!(ids(&take_newest(&mut entries)), [2]);
        assert!(take_newest(&mut entries).is_empty());
    }
}
