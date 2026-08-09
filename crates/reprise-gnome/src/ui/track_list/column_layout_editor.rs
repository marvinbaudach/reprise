//! Music-table entry points for the shared column editor.

use std::rc::Rc;

use gtk4::prelude::*;
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

fn model(track_list: &Rc<TrackList>) -> Rc<dyn EditorModel> {
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

/// True when a click at vertical offset `y` (relative to the ColumnView) landed
/// on the header row. The header is always the ColumnView's first child and sits
/// flush at the top, so its height defines the band.
fn is_header_click(y: f64, header_height: i32) -> bool {
    header_height > 0 && y <= f64::from(header_height)
}

fn build_header_popover(track_list: &Rc<TrackList>) -> (gtk4::Popover, gtk4::ListBox) {
    let surface = table_columns::editor::build_surface(&model(track_list), false);
    let content = gtk4::Frame::builder()
        .width_request(360)
        .height_request(440)
        .child(&surface.toolbar)
        .build();
    let popover = gtk4::Popover::builder()
        .autohide(true)
        .has_arrow(true)
        .child(&content)
        .build();
    popover.add_css_class("menu");
    popover.add_css_class("reprise-column-header-popover");
    (popover, surface.list)
}

/// Installs the right-click-on-header gesture that opens the editor popover.
pub(in crate::ui) fn install_header_popover(track_list: &Rc<TrackList>) {
    let column_view = track_list.column_view_widget().clone();
    // input-parity: ACC-8 keyboard=column-editor
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gtk4::gdk::BUTTON_SECONDARY);
    // Capture phase, claiming on press: GtkColumnViewTitle's own click
    // gesture claims EVERY press (any button) at the target, so a
    // bubble-phase ancestor gesture loses the sequence before its handler
    // can run — the exact claim race that also breaks GTK's native column
    // drag (see `column_header_dnd`'s module doc). At capture this gesture
    // runs first, and its claim below keeps the title's own gesture (and
    // GTK's plain visibility menu) from ever seeing header right-clicks.
    gesture.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let track_list_weak = Rc::downgrade(track_list);
    let view = column_view.clone();
    gesture.connect_pressed(move |gesture, _, x, y| {
        let header_height = view.first_child().map_or(0, |header| header.height());
        if !is_header_click(y, header_height) {
            return;
        }
        let Some(track_list) = track_list_weak.upgrade() else {
            return;
        };
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        let (popover, initial_focus) = build_header_popover(&track_list);
        popover.set_parent(&view);
        popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        let focus_guard = crate::ui::transient_focus::TransientFocusGuard::capture(&view);
        focus_guard.bind_popover(&popover, &initial_focus);
        crate::ui::popover_lifecycle::unparent_after_actions(&popover);
        popover.popup();
        tracing::debug!("column header popover opened");
    });
    column_view.add_controller(gesture);
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

    #[test]
    fn header_hit_test_matches_only_the_header_band() {
        assert!(is_header_click(0.0, 25));
        assert!(is_header_click(25.0, 25));
        assert!(!is_header_click(25.1, 25));
        assert!(!is_header_click(200.0, 25));
        assert!(!is_header_click(0.0, 0));
    }
}
