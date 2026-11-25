//! The history as a side view (#162): the closed nodes, newest first

use super::history::{self, Closed};
use crate::{
    session::saved_label,
    ui::{self, PAD, Rows},
};
use sonamu_common::settings::Settings;
use sonamu_ui::ui_element::{Style, UIElement, UITree};

/// A closed node's name
fn label(closed: &Closed) -> String {
    closed
        .tree
        .main
        .as_ref()
        .map(saved_label)
        .or_else(|| closed.tree.label.clone())
        .unwrap_or_else(|| "Container".to_owned())
}

/// A row per entry, newest first; a group is its newest's row and how many more
fn rows(entries: &[Closed]) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    let mut seen = Vec::new();
    for entry in entries.iter().rev() {
        let Some(group) = entry.group else {
            rows.push(label(entry));
            continue;
        };
        if !seen.contains(&group) {
            seen.push(group);
            let more = entries
                .iter()
                .filter(|other| other.group == Some(group))
                .count()
                - 1;
            rows.push(if more == 0 {
                label(entry)
            } else {
                format!("{} +{more}", label(entry))
            });
        }
    }
    rows
}

/// The title and as many rows as fit in a `[width, height]` box (the history isn't scrolled yet)
pub fn view(settings: &Settings, [width, height]: [f32; 2]) -> UITree {
    let palette = &settings.palette;
    let layout = Rows::new(0.0);
    let row = |index: usize, text: UIElement| {
        layout.row(index, width, None, vec![layout.text(text, PAD, width)])
    };
    let shown = layout.fitting(height);
    let mut children = vec![row(0, ui::text("History", palette.accent))];
    children.extend(
        rows(&history::entries())
            .into_iter()
            .take(shown.saturating_sub(1))
            .enumerate()
            .map(|(index, label)| row(index + 1, ui::text(label, palette.fg))),
    );
    UITree::new_styled(
        UIElement::Backgrounded(ui::panel_bg(palette)),
        Style::full_size(),
        children,
    )
}
