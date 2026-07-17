//! Tag editor dialog orchestration.
//!
//! F0: the single wiring point (`present()`) builds the [`TagEditSession`]
//! that is now the dialog's only state truth and threads it through
//! `TagEditorForm::build`, `tag_editor_dirty::wire`, and
//! `tag_editor_save::wire` — no more `Vec<Rc<Cell<bool>>>` dirty array
//! running in parallel.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::tag_edit::TrackWrite;
use reprise_core::library::tag_edit_session::{SessionMode, SessionTrack, TagEditSession};
use rusqlite::Connection;

use crate::ui::tag_editor_form::{EditorMode, TagEditorForm};
pub use crate::ui::tag_editor_state::NavigateDirection;

pub(in crate::ui) const STAR_FILLED: &str = "\u{2605}";
pub(in crate::ui) const STAR_OUTLINE: &str = "\u{2606}";

pub fn present(
    parent: &adw::ApplicationWindow,
    conn: &Rc<RefCell<Connection>>,
    tracks: Vec<SessionTrack>,
    bitrates: &[Option<u32>],
    on_save: impl Fn(Vec<TrackWrite>) + Clone + 'static,
    on_navigate: impl Fn(NavigateDirection) -> bool + 'static,
) {
    let Some(mode) = EditorMode::new(tracks.len()) else {
        tracing::warn!("tag editor called with empty track list");
        return;
    };
    let track_count = mode.track_count();
    let track_paths: Vec<(i64, PathBuf)> = tracks
        .iter()
        .map(|track| (track.id, track.path.clone()))
        .collect();
    let session_mode = if mode.is_multi() {
        SessionMode::Multi
    } else {
        SessionMode::SingleNav
    };
    let session = Rc::new(RefCell::new(TagEditSession::new(tracks, session_mode)));

    let form = TagEditorForm::build(mode, conn, &track_paths, bitrates, &session);
    let crate::ui::tag_editor_dirty::DirtyState {
        update: update_save_state,
    } = crate::ui::tag_editor_dirty::wire(mode, &form, &session);

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

    let dialog_for_save = form.dialog.clone();
    crate::ui::tag_editor_save::wire(
        crate::ui::tag_editor_save::SaveWidgets {
            dialog: &form.dialog,
            save_button: &form.save_btn,
            cancel_button: &form.cancel_btn,
            previous_button: &form.prev_btn,
            next_button: &form.next_btn,
            year: &form.year_row,
            track_number: &form.track_no_row,
            error_label: &form.error_label,
        },
        &session,
        move |batch| {
            on_save(batch);
            dialog_for_save.close();
        },
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
