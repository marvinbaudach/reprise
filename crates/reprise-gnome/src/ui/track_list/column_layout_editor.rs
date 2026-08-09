//! Music-table entry points for the shared column editor.

use std::rc::Rc;

use libadwaita as adw;

use crate::ui::column_layout::{self, ColumnId, ColumnLayout};
use crate::ui::strings;
use crate::ui::table_columns::{self, ColumnDescriptor, EditorModel};
use crate::ui::track_list::TrackList;

pub(in crate::ui) const SMOKE_ENV: &str = "REPRISE_SMOKE_COLUMN_LAYOUT_EDITOR";

struct MusicEditorModel {
    track_list: std::rc::Weak<TrackList>,
}

fn editor_lists_column(id: ColumnId) -> bool {
    id != ColumnId::Cover
}

impl MusicEditorModel {
    fn apply(&self, next: &ColumnLayout) {
        let Some(track_list) = self.track_list.upgrade() else {
            return;
        };
        if let Err(error) = track_list.apply_column_layout(next) {
            tracing::warn!(%error, "could not save edited column layout");
            track_list.toast(&strings::text(strings::COLUMN_LAYOUT_SAVE_FAILED));
        }
    }
}

impl EditorModel for MusicEditorModel {
    fn title(&self) -> String {
        strings::text(strings::EDIT_COLUMN_LAYOUT)
    }

    fn columns(&self) -> Vec<ColumnDescriptor> {
        let Some(track_list) = self.track_list.upgrade() else {
            return Vec::new();
        };
        track_list
            .current_column_layout()
            .order
            .into_iter()
            .filter(|id| editor_lists_column(*id))
            .map(|id| ColumnDescriptor {
                id: id.as_str().to_owned(),
                label: column_layout::column_label(id),
            })
            .collect()
    }

    fn is_visible(&self, id: &str) -> bool {
        let Some(track_list) = self.track_list.upgrade() else {
            return false;
        };
        ColumnId::parse(id)
            .is_some_and(|id| track_list.current_column_layout().visible.contains(&id))
    }

    fn set_visible(&self, id: &str, visible: bool) {
        let Some(track_list) = self.track_list.upgrade() else {
            return;
        };
        let Some(id) = ColumnId::parse(id) else {
            return;
        };
        let layout = track_list.current_column_layout();
        self.apply(&column_layout::set_column_visible(&layout, id, visible));
    }

    fn move_column(&self, id: &str, target: &str, after: bool) {
        let Some(track_list) = self.track_list.upgrade() else {
            return;
        };
        let (Some(id), Some(target)) = (ColumnId::parse(id), ColumnId::parse(target)) else {
            return;
        };
        let layout = track_list.current_column_layout();
        let next = if after {
            column_layout::move_column_after(&layout, id, target)
        } else {
            column_layout::move_column(&layout, id, target)
        };
        self.apply(&next);
    }

    fn reset(&self) {
        let Some(track_list) = self.track_list.upgrade() else {
            return;
        };
        self.apply(&ColumnLayout::default());
        track_list.reset_column_widths();
    }
}

pub(in crate::ui) fn model(track_list: &Rc<TrackList>) -> Rc<dyn EditorModel> {
    Rc::new(MusicEditorModel {
        track_list: Rc::downgrade(track_list),
    })
}

pub(in crate::ui) fn css() -> String {
    table_columns::editor_dnd::css()
}

pub(in crate::ui) fn build_navigation_page(track_list: &Rc<TrackList>) -> adw::NavigationPage {
    table_columns::editor::build_navigation_page(&model(track_list))
}

/// Installs the right-click-on-header gesture that opens the editor popover.
pub(in crate::ui) fn install_header_popover(track_list: &Rc<TrackList>) {
    table_columns::header_popover::install_header_popover(
        track_list.column_view_widget(),
        &model(track_list),
    );
}

pub(in crate::ui) fn present(window: &adw::ApplicationWindow, track_list: &Rc<TrackList>) {
    table_columns::editor::present_dialog(window, &model(track_list));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_is_excluded_from_the_editor_but_other_columns_are_listed() {
        assert!(!editor_lists_column(ColumnId::Cover));
        for id in [ColumnId::Title, ColumnId::Artist, ColumnId::Added] {
            assert!(editor_lists_column(id), "{id:?} should be listed");
        }
    }
}
