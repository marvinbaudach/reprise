//! Tag editor dialog orchestration.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::tag_edit::{summarize, summarize_values, EditableTags, TrackEditPatch};
use rusqlite::Connection;

use crate::ui::tag_editor_form::{EditorMode, TagEditorForm};
pub use crate::ui::tag_editor_state::NavigateDirection;

pub(in crate::ui) const STAR_FILLED: &str = "\u{2605}";
pub(in crate::ui) const STAR_OUTLINE: &str = "\u{2606}";

pub fn present(
    parent: &adw::ApplicationWindow,
    conn: &Rc<RefCell<Connection>>,
    tracks: &[(i64, PathBuf)],
    tags: &[EditableTags],
    ratings: &[i32],
    on_apply: impl Fn(TrackEditPatch) + Clone + 'static,
    on_navigate: impl Fn(NavigateDirection) -> bool + 'static,
) {
    let Some(mode) = EditorMode::new(tracks.len()) else {
        tracing::warn!("tag editor called with empty track list");
        return;
    };
    let track_count = mode.track_count();
    let summary = summarize(tags).unwrap();
    let rating_summary = summarize_values(ratings).unwrap();
    let form = TagEditorForm::build(mode, conn, tracks, &summary, &rating_summary);
    let crate::ui::tag_editor_dirty::DirtyState {
        flags: dirty,
        update: update_save_state,
    } = crate::ui::tag_editor_dirty::wire(mode, &form, &summary, &rating_summary);

    crate::ui::tag_editor_lookup::wire(
        mode.is_multi(),
        crate::ui::tag_editor_lookup::LookupWidgets {
            button: &form.mb_btn,
            hint: &form.mb_hint,
            year: &form.year_row,
            artist: &form.artist_ac,
            album: &form.album_ac,
            album_artist: &form.album_artist_ac,
            genre: &form.genre_ac,
        },
        &update_save_state,
    );
    crate::ui::tag_editor_save::wire(
        crate::ui::tag_editor_save::SaveWidgets {
            dialog: &form.dialog,
            save_button: &form.save_btn,
            cancel_button: &form.cancel_btn,
            previous_button: &form.prev_btn,
            next_button: &form.next_btn,
            title: &form.title_row,
            artist: &form.artist_ac,
            album: &form.album_ac,
            album_artist: &form.album_artist_ac,
            genre: &form.genre_ac,
            year: &form.year_row,
            track_number: &form.track_no_row,
            rating: &form.rating_value,
            error_label: &form.error_label,
        },
        &dirty,
        on_apply,
        on_navigate,
    );

    form.dialog.present(Some(parent));
    tracing::debug!(
        track_count,
        is_multi = mode.is_multi(),
        "redesigned tag editor presented"
    );
}

#[cfg(test)]
#[path = "tag_editor_tests.rs"]
mod tests;
