//! The arrow's popup: the side views to pin, the pinned one marked

use super::{
    Lines,
    widgets::{Button, Cell},
};
use sonamu_common::{sap::session::SideView, settings::Settings};
use sonamu_ui::ui_element::UITree;

const VIEWS: [(SideView, &str); 2] = [(SideView::Tree, "Tree"), (SideView::History, "History")];
const COLUMNS: usize = 12;

pub fn size() -> [f32; 2] {
    Lines::size(VIEWS.len(), COLUMNS)
}

pub fn draw(settings: &Settings, pinned: Option<SideView>, hover: Option<Cell>) -> Vec<UITree> {
    let palette = &settings.palette;
    let lines = Lines::new();
    VIEWS
        .iter()
        .enumerate()
        .flat_map(|(line, &(view, name))| {
            let mark = if pinned == Some(view) { "✓" } else { " " };
            Button::row(line, COLUMNS).draw(
                &lines,
                palette,
                vec![(format!("{mark} {name}"), palette.fg)],
                false,
                hover,
            )
        })
        .collect()
}

/// The view whose line is at `cell`
pub fn click((line, _): Cell) -> Option<SideView> {
    VIEWS.get(line).map(|&(view, _)| view)
}
