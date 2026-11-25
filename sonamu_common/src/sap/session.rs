//! What a session is saved as, to be restored on the next start (see `.claude/SESSIONS.md`)

use crate::{command::NodeUid, utils::tree::world_tree::WorldTreePath};
use serde::{Deserialize, Deserializer, Serialize, de::IgnoredAny};
use std::{collections::BTreeMap, path::PathBuf};

/// The whole tree, where focus was, and the pinned side view
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub tree: SavedTree,
    /// Each monitor's focus (by `ScreenArea::name`): the focused node's id, then its ancestors' (#158)
    #[serde(default)]
    pub monitor_focus: BTreeMap<String, Vec<NodeUid>>,
    /// Each monitor's brightness, 0 to 1 (#158)
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub brightness: BTreeMap<String, f32>,
    /// Before monitors had names, each screen's; the first is used if `monitor_focus` is empty
    #[serde(default, skip_serializing)]
    pub screen_focus: Vec<Vec<NodeUid>>,
    /// Focus before #158, used if both are empty: a path of the tree; the root if it can't be read
    /// (eg saved before #83)
    #[serde(default, deserialize_with = "path_or_root", skip_serializing)]
    pub focus: WorldTreePath,
    /// The view always shown beside the panes
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side_view: Option<SideView>,
    /// Before `side_view`: whether the treeview was pinned. Only read (`pinned`).
    #[serde(default, skip_serializing)]
    pub treeview_pinned: bool,
}
impl Session {
    /// The pinned side view, also from a session saved before `side_view`
    pub fn pinned(&self) -> Option<SideView> {
        self.side_view
            .or(self.treeview_pinned.then_some(SideView::Tree))
    }
}

/// What the root's side column can show, pinned (#162)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideView {
    Tree,
    History,
}

fn path_or_root<'de, D: Deserializer<'de>>(deserializer: D) -> Result<WorldTreePath, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum PathOrOther {
        Path(WorldTreePath),
        Other(IgnoredAny),
    }
    Ok(match PathOrOther::deserialize(deserializer)? {
        PathOrOther::Path(path) => path,
        PathOrOther::Other(_) => WorldTreePath::root(),
    })
}

/// A node, like a `DetachedTree` but saved
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedTree {
    /// Kept by the restored node (`None`: saved before #158, or not a node, eg a file manager's document)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<NodeUid>,
    /// `None` for containers (a `layout` key from before splits were removed is ignored)
    pub main: Option<SavedApplet>,
    #[serde(default)]
    pub children: Vec<Self>,
    /// Half width (#144)
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub half: bool,
    /// A container's name; only a project's inner root has one (the project's name)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// A project's working directory for programs started inside; only a project's inner root has one
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub directory: Option<PathBuf>,
    /// A project's icon in the treeview (`None`: the default); only a project's inner root has one
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// A project's color in the treeview (#76); only a project's inner root has one
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<sonamu_ui::color::Color>,
}
impl SavedTree {
    /// New nodes don't take its ids, also those inside its applets (eg a hibernating project's)
    pub fn reserve_ids(&self) {
        if let Some(id) = self.id {
            id.reserve();
        }
        if let Some(SavedApplet::Spawner {
            inner: Some(inner), ..
        }) = &self.main
        {
            inner.reserve_ids();
        }
        for child in &self.children {
            child.reserve_ids();
        }
    }
}

/// How to make a node's main again
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SavedApplet {
    /// Made by the registered applet spawner with this name, from these args (like the command hub does),
    /// and its inner tree if it saves one (eg projects)
    Spawner {
        name: String,
        args: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        inner: Option<Box<SavedTree>>,
    },
    /// A wl client's toplevel
    Wl(SavedWl),
}

/// A wl client's toplevel, to start the client again
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedWl {
    /// The client's command line
    pub argv: Vec<String>,
    /// Where it was working: its first child's (eg a terminal's shell), else its own
    pub cwd: Option<PathBuf>,
    pub app_id: Option<String>,
    pub title: String,
    /// The program running in its terminal (the foreground job of its first child's tty), if any
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub running: Option<SavedRunning>,
}
impl SavedWl {
    pub fn url(&self) -> Option<&str> {
        split_title_url(&self.title).map(|(url, _)| url)
    }

    /// A Chromium web app's (PWA's) id and profile directory, from its app id
    /// `chrome-<id>-<profile directory, spaces as _>`
    pub fn web_app(&self) -> Option<(&str, String)> {
        let (id, profile) = self
            .app_id
            .as_deref()?
            .strip_prefix("chrome-")?
            .split_once('-')?;
        (id.len() == 32 && id.bytes().all(|b| (b'a'..=b'p').contains(&b)))
            .then(|| (id, profile.replace('_', " ")))
    }
}

/// A program running in a terminal, to run it again in the restored terminal
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedRunning {
    pub argv: Vec<String>,
    pub cwd: Option<PathBuf>,
    /// The shell it ran in (the terminal's, which may not be `$SHELL`), left once it exits
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
}

/// A url and the rest of the title: at the start before ` | ` (eg firefox with `extras/firefox`),
/// else at the end after ` | ` (eg qutebrowser with `window.title_format = '{current_title} | {current_url}'`)
pub fn split_title_url(title: &str) -> Option<(&str, &str)> {
    let is_url = |s: &str| (s.contains("://") || s.starts_with("about:")) && !s.contains(' ');
    let (start, rest) = title.split_once(" | ")?;
    if is_url(start) {
        return Some((start, rest));
    }
    let (rest, end) = title.rsplit_once(" | ")?;
    is_url(end).then_some((end, rest))
}

/// The title as shown to the user, without its url
pub fn title_without_url(title: &str) -> &str {
    split_title_url(title).map_or(title, |(_, rest)| rest)
}
