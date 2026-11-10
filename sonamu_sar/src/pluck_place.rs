//! The server holds what was plucked until it's placed.

use crate::runner::AppletRunner;
use sonamu_common::{
    sap::{
        detached::DetachedTree,
        packets::{SpawnPosition, StandardEvent},
    },
    utils::tree::world_tree::{WorldTree, WorldTreePath},
};
use sonamu_ui::{
    color::Color,
    ui_element::{Length, Style, UIElement, UITree, font_size},
};
use std::fmt::Write;

/// One pluck or place in flight at a time
#[derive(Default)]
pub(crate) enum PluckPlace {
    #[default]
    Idle,
    /// Waiting for `Plucked`
    Plucking {
        /// A drag can be dropped before the pluck arrives
        queued_place: Option<(WorldTreePath, SpawnPosition)>,
    },
    /// Waiting for `Placed`
    Placing,
}

const GHOST_OFFSET: f32 = 16.0;
const GHOST_PADDING: f32 = 10.0;

impl AppletRunner {
    /// Alt+P: pluck at the root's focus, or place there if something is plucked
    pub(crate) fn toggle_pluck_place(&mut self) {
        if !matches!(self.pluck_place, PluckPlace::Idle) {
            return;
        }
        if let Some(plucked) = self.plucked.take() {
            self.place(None, SpawnPosition::AppendChild, plucked);
        } else {
            self.pluck(None);
        }
    }

    pub(crate) fn pluck(&mut self, path: Option<WorldTreePath>) {
        if matches!(self.pluck_place, PluckPlace::Idle) && self.plucked.is_none() {
            self.pluck_place = PluckPlace::Plucking { queued_place: None };
            self.root_client.send_event(StandardEvent::Pluck(path));
        }
    }

    fn place(
        &mut self,
        path: Option<WorldTreePath>,
        position: SpawnPosition,
        plucked: DetachedTree,
    ) {
        self.pluck_place = PluckPlace::Placing;
        self.root_client
            .send_event(StandardEvent::Place(path, position, plucked));
        self.needs_redraw = true;
    }

    /// `RequestPlace` from the root (eg a treeview drop)
    pub(crate) fn request_place(&mut self, path: WorldTreePath, position: SpawnPosition) {
        match &mut self.pluck_place {
            PluckPlace::Plucking { queued_place } => *queued_place = Some((path, position)),
            PluckPlace::Placing => {}
            PluckPlace::Idle => {
                if let Some(plucked) = self.plucked.take() {
                    self.place(Some(path), position, plucked);
                }
            }
        }
    }

    pub(crate) fn plucked(&mut self, plucked: Option<DetachedTree>) {
        let PluckPlace::Plucking { queued_place } = std::mem::take(&mut self.pluck_place) else {
            eprintln!("Unexpected `Plucked`");
            self.plucked = plucked;
            return;
        };
        self.plucked = plucked;
        if let Some((path, position)) = queued_place
            && let Some(plucked) = self.plucked.take()
        {
            self.place(Some(path), position, plucked);
        }
        self.needs_redraw = true;
    }

    pub(crate) fn placed(&mut self, given_back: Option<DetachedTree>) {
        if !matches!(self.pluck_place, PluckPlace::Placing) {
            eprintln!("Unexpected `Placed`");
        }
        self.pluck_place = PluckPlace::Idle;
        // REVIEW: the root always places, so this shouldn't happen
        if given_back.is_some() {
            self.plucked = given_back;
        }
        self.needs_redraw = true;
    }

    /// What's plucked, as a box following the pointer (like a drag icon)
    pub(crate) fn plucked_ghost(&self) -> Option<UITree> {
        let plucked = self.plucked.as_ref()?;
        let treeview = plucked.peek_treeview();
        let mut label = treeview.get_root_value().clone();
        let descendants = count_nodes(&treeview) - 1;
        if descendants > 0 {
            let _ = write!(label, " +{descendants}");
        }

        let [x, y] = self.pointer_position.map(|coord| coord as f32);
        let font_size = font_size();
        let size = [
            // rough monospace advance
            label.chars().count() as f32 * font_size * 0.6 + GHOST_PADDING * 2.0,
            font_size + GHOST_PADDING * 2.0,
        ];
        let settings = sonamu_common::settings::get();
        let Color([red, green, blue, _]) = settings.palette.raised(10);
        let text = UITree::leaf_styled(
            UIElement::Text(vec![(label, UIElement::glyphon_attr(settings.palette.fg))]),
            Style::absolute([GHOST_PADDING, GHOST_PADDING], [size[0], font_size]),
        );
        Some(UITree::new_styled(
            UIElement::Backgrounded(Color([red, green, blue, 0xE0])),
            Style::absolute([x + GHOST_OFFSET, y + GHOST_OFFSET], size),
            vec![UITree::new_styled(
                UIElement::border(settings.border.focused),
                Style {
                    border: Length::uniform_rect(Length::Px(2.0)),
                    ..Style::full_size()
                },
                vec![text],
            )],
        ))
    }
}

fn count_nodes(treeview: &WorldTree<String>) -> usize {
    use sonamu_common::utils::tree::tree_node_path::TraversableTree;
    match treeview {
        WorldTree::Base(_) => 1,
        WorldTree::World(root) => root.iter_paths_dfs().count(),
    }
}
