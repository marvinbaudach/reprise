//! FB-3 failure-details dialog for the tag editor's save path (Task F2):
//! individual failures are never toasted one by one — the caller collects
//! them into a single "N failed [Details]" toast (`tag_edit_flow.rs`), and
//! this module is what "Details" opens: filename + classified reason per
//! failure (`WriteErrorKind::user_message()`, never raw Lofty text), plus
//! an "Edit failed tracks…" button that reopens the editor on exactly
//! those tracks, pending-fresh.

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::tag_edit::TagWriteFailure;

use crate::ui::strings;

/// One row the details dialog shows — a pure projection of
/// `TagWriteFailure` so the mapping is exercised headlessly, without a
/// display. Deliberately doesn't carry `id`: the dialog's own "Edit failed
/// tracks…" button reads ids from [`failed_track_ids`] instead, straight
/// off the original failures — one projection per concern.
pub(in crate::ui) struct FailureRow {
    pub(in crate::ui) file_name: String,
    pub(in crate::ui) reason: &'static str,
}

pub(in crate::ui) fn failure_rows(failures: &[TagWriteFailure]) -> Vec<FailureRow> {
    failures
        .iter()
        .map(|failure| FailureRow {
            file_name: failure.path.file_name().map_or_else(
                || failure.path.to_string_lossy().into_owned(),
                |name| name.to_string_lossy().into_owned(),
            ),
            reason: failure.kind.user_message(),
        })
        .collect()
}

/// The exact track ids "Edit failed tracks…" reopens the editor on (FB-3:
/// "öffnet den Editor mit genau diesen Tracks") — never the whole original
/// batch, and never the tracks that actually succeeded.
pub(in crate::ui) fn failed_track_ids(failures: &[TagWriteFailure]) -> Vec<i64> {
    failures.iter().map(|failure| failure.id).collect()
}

/// Presents the FB-3 details dialog over `parent` (Level 1 again by this
/// point — the tag editor dialog that owned the save is already closed).
pub(in crate::ui) fn present(
    parent: &adw::ApplicationWindow,
    failures: &[TagWriteFailure],
    on_edit_failed: impl Fn(Vec<i64>) + 'static,
) {
    let rows = failure_rows(failures);
    let ids = failed_track_ids(failures);

    let list = gtk4::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk4::SelectionMode::None);
    for row in &rows {
        let action_row = adw::ActionRow::builder()
            .title(&row.file_name)
            .subtitle(row.reason)
            .build();
        list.append(&action_row);
    }
    let scrolled = gtk4::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .build();

    let edit_button = gtk4::Button::with_label(&strings::text(strings::TAG_EDIT_FAILED_TRACKS));
    edit_button.add_css_class("suggested-action");
    edit_button.set_halign(gtk4::Align::End);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.append(&scrolled);
    content.append(&edit_button);

    let header = adw::HeaderBar::new();
    let title_widget =
        adw::WindowTitle::new(&strings::text(strings::TAG_SAVE_FAILURE_DIALOG_TITLE), "");
    header.set_title_widget(Some(&title_widget));

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&content));

    let dialog = adw::Dialog::builder()
        .child(&toolbar)
        .content_width(420)
        .content_height(480)
        .build();

    let dialog_for_click = dialog.clone();
    edit_button.connect_clicked(move |_| {
        dialog_for_click.close();
        on_edit_failed(ids.clone());
    });

    dialog.present(Some(parent));
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::library::tag_edit::WriteErrorKind;
    use std::path::PathBuf;

    fn failure(id: i64, name: &str, kind: WriteErrorKind) -> TagWriteFailure {
        TagWriteFailure {
            id,
            path: PathBuf::from(format!("/music/{name}")),
            kind,
            error: "raw lofty text should never surface in the dialog".into(),
        }
    }

    #[test]
    fn fb_3_failures_collected_into_single_toast() {
        let failures = vec![
            failure(1, "a.flac", WriteErrorKind::PermissionDenied),
            failure(2, "b.flac", WriteErrorKind::NotFound),
        ];

        // One aggregate toast string — counts only, never one message per
        // failure (FB-3: "Einzelfehler im Lauf werden gesammelt, nie
        // einzeln getoastet").
        let toast = strings::tag_save_result_toast_with_failures(30, failures.len());
        assert_eq!(toast, "Tags updated \u{b7} 30 tracks \u{b7} 2 failed");

        // The per-failure detail still exists — just inside the details
        // dialog's row list, not as separate toasts.
        let rows = failure_rows(&failures);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].file_name, "a.flac");
        assert_eq!(
            rows[0].reason,
            WriteErrorKind::PermissionDenied.user_message()
        );
        assert_eq!(rows[1].file_name, "b.flac");
        assert_eq!(rows[1].reason, WriteErrorKind::NotFound.user_message());
    }

    #[test]
    fn fb_3_details_reopens_editor_with_failed_tracks() {
        let failures = vec![
            failure(2, "b.flac", WriteErrorKind::NotFound),
            failure(5, "e.flac", WriteErrorKind::UnreadableTags),
        ];

        assert_eq!(failed_track_ids(&failures), vec![2, 5]);
    }
}
