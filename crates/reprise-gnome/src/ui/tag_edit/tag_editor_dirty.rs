//! Session-backed field wiring (F0/TAG-2/TAG-4): every field's GTK
//! "changed"/click signal writes straight into the shared `TagEditSession`
//! — the single state truth. The old per-field `Cell<bool>` dirty array is
//! gone: it duplicated exactly what the session already tracks (which
//! fields differ from their original value) and, being private to this
//! module, was the reason Package C could arm a Mixed field's *display* but
//! never wire its in-field ↺ revert (a revert from outside this module
//! could clear the visible text without clearing the flag, silently
//! writing an empty string over genuinely different per-track values).
//!
//! The `update` callback this module returns is the single place that
//! recomputes every session-derived UI surface after any field edit; F0
//! only recomputes Save-button sensitivity, F1 extends the same callback
//! with the review footer.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::tag_edit_session::{
    FieldValue, PendingScope, SessionMode, TagEditSession, TagField,
};

use crate::ui::autocomplete_entry::AutocompleteEntry;
use crate::ui::strings;
use crate::ui::tag_editor_form::{EditorMode, TagEditorForm};
use crate::ui::tag_editor_state::{number_patch, ParseFieldError};
use crate::ui::tag_editor_widgets::wire_star_clicks;

pub(in crate::ui) type UpdateCallback = Rc<dyn Fn()>;

pub(in crate::ui) struct DirtyState {
    pub(in crate::ui) update: UpdateCallback,
}

/// The `PendingScope` every field write in this dialog resolves to for its
/// whole lifetime: `AllTracks` for Multi's bulk edit, `CurrentTrack` for
/// SingleNav (today always the session's one and only track; Package G's
/// browse snapshot is what makes `CurrentTrack` actually move between
/// tracks). There is no third case — `TagEditSession::mode()` decides it
/// once, not per call.
pub(in crate::ui) fn session_scope(mode: SessionMode) -> PendingScope {
    match mode {
        SessionMode::Multi => PendingScope::AllTracks,
        SessionMode::SingleNav => PendingScope::CurrentTrack,
    }
}

/// TAG-2's field-revert: clears `field`'s pending value for `scope` and
/// returns what the field should now display (its original value, or the
/// still-mixed placeholder's own display text) — text and pending always
/// reset together as one atomic step, which is exactly the guarantee
/// Package C's removed click-to-unlock could not make from outside this
/// module. Also the named API Package E's Esc-cascade stage 2 calls
/// directly (no GTK dependency, so it works for any widget shape).
pub(in crate::ui) fn revert_field(
    session: &mut TagEditSession,
    scope: PendingScope,
    field: TagField,
) -> String {
    session.revert(scope, field);
    let track_id = session.current_track_id();
    session
        .effective_display(track_id, field)
        .unwrap_or_default()
}

/// Best-effort number parse for Year/Track-number's live wiring: a
/// non-parseable interim keystroke (e.g. a lone "-") is simply not pushed
/// to the session rather than surfaced as an error — the authoritative gate
/// is `tag_editor_save.rs`'s save-time validation, which still sees
/// whatever the field's final text is. Delegates to `tag_editor_state::
/// number_patch` (forcing `dirty: true`, since this is only ever called on
/// a real "changed" signal) rather than re-implementing its trim/parse/
/// zero-check rules a second time.
pub(in crate::ui) fn parse_number_field(text: &str) -> Result<Option<u32>, ParseFieldError> {
    number_patch(true, text).map(|value| value.unwrap_or(None))
}

pub(in crate::ui) fn wire(
    mode: EditorMode,
    form: &TagEditorForm,
    session: &Rc<RefCell<TagEditSession>>,
) -> DirtyState {
    let scope = session_scope(if mode.is_multi() {
        SessionMode::Multi
    } else {
        SessionMode::SingleNav
    });

    let update: UpdateCallback = {
        let save_button = form.save_btn.clone();
        let session = session.clone();
        Rc::new(move || {
            let pending = session.borrow().pending_track_count();
            save_button.set_sensitive(pending > 0);
        })
    };

    wire_text_field(&form.title_row, TagField::Title, scope, session, &update);
    wire_number_field(&form.year_row, TagField::Year, scope, session, &update);
    wire_number_field(
        &form.track_no_row,
        TagField::TrackNo,
        scope,
        session,
        &update,
    );
    wire_autocomplete_field(&form.artist_ac, TagField::Artist, scope, session, &update);
    wire_autocomplete_field(&form.album_ac, TagField::Album, scope, session, &update);
    wire_autocomplete_field(
        &form.album_artist_ac,
        TagField::AlbumArtist,
        scope,
        session,
        &update,
    );
    wire_autocomplete_field(&form.genre_ac, TagField::Genre, scope, session, &update);
    wire_rating(
        &form.rating_value,
        &form.rating_box,
        scope,
        session,
        &update,
    );

    update();

    DirtyState { update }
}

fn build_revert_button() -> gtk4::Button {
    let button = gtk4::Button::from_icon_name("edit-undo-symbolic");
    button.add_css_class("flat");
    button.add_css_class("reprise-tag-field-revert");
    button.set_valign(gtk4::Align::Center);
    button.set_visible(false);
    button.set_tooltip_text(Some(&strings::text(strings::TAG_REVERT)));
    button
}

fn field_is_armed(session: &TagEditSession, scope: PendingScope, field: TagField) -> bool {
    session.old_value_line(scope, field).is_some()
}

fn wire_text_field(
    row: &adw::EntryRow,
    field: TagField,
    scope: PendingScope,
    session: &Rc<RefCell<TagEditSession>>,
    update: &UpdateCallback,
) {
    if !row.is_editable() {
        // TAG-3: a per-track-locked field (Title/Track-number in Multi) is
        // never wired into the session — nothing can arm it from here.
        return;
    }
    let revert_btn = build_revert_button();
    row.add_suffix(&revert_btn);

    {
        let session = session.clone();
        let update = update.clone();
        let revert_btn = revert_btn.clone();
        row.connect_changed(move |entry| {
            let text = entry.text().to_string();
            session
                .borrow_mut()
                .set_pending(scope, field, &FieldValue::Text(text));
            let armed = field_is_armed(&session.borrow(), scope, field);
            revert_btn.set_visible(armed);
            update();
        });
    }
    {
        let session = session.clone();
        let update = update.clone();
        let row = row.clone();
        revert_btn.connect_clicked(move |button| {
            let text = {
                let mut session_mut = session.borrow_mut();
                revert_field(&mut session_mut, scope, field)
            };
            row.set_text(&text);
            button.set_visible(false);
            update();
        });
    }
}

fn wire_number_field(
    row: &adw::EntryRow,
    field: TagField,
    scope: PendingScope,
    session: &Rc<RefCell<TagEditSession>>,
    update: &UpdateCallback,
) {
    if !row.is_editable() {
        return;
    }
    let revert_btn = build_revert_button();
    row.add_suffix(&revert_btn);

    {
        let session = session.clone();
        let update = update.clone();
        let revert_btn = revert_btn.clone();
        row.connect_changed(move |entry| {
            if let Ok(value) = parse_number_field(&entry.text()) {
                session
                    .borrow_mut()
                    .set_pending(scope, field, &FieldValue::Number(value));
            }
            let armed = field_is_armed(&session.borrow(), scope, field);
            revert_btn.set_visible(armed);
            update();
        });
    }
    {
        let session = session.clone();
        let update = update.clone();
        let row = row.clone();
        revert_btn.connect_clicked(move |button| {
            let text = {
                let mut session_mut = session.borrow_mut();
                revert_field(&mut session_mut, scope, field)
            };
            row.set_text(&text);
            button.set_visible(false);
            update();
        });
    }
}

fn wire_autocomplete_field(
    ac: &Rc<AutocompleteEntry>,
    field: TagField,
    scope: PendingScope,
    session: &Rc<RefCell<TagEditSession>>,
    update: &UpdateCallback,
) {
    let revert_btn = build_revert_button();
    ac.row().add_suffix(&revert_btn);

    {
        let session = session.clone();
        let update = update.clone();
        let ac_for_read = ac.clone();
        let revert_btn = revert_btn.clone();
        ac.connect_changed(move || {
            session
                .borrow_mut()
                .set_pending(scope, field, &FieldValue::Text(ac_for_read.text()));
            let armed = field_is_armed(&session.borrow(), scope, field);
            revert_btn.set_visible(armed);
            update();
        });
    }
    {
        let session = session.clone();
        let update = update.clone();
        let ac = ac.clone();
        revert_btn.connect_clicked(move |button| {
            let text = {
                let mut session_mut = session.borrow_mut();
                revert_field(&mut session_mut, scope, field)
            };
            ac.set_text(&text);
            button.set_visible(false);
            update();
        });
    }
}

fn wire_rating(
    rating_value: &Rc<Cell<i32>>,
    rating_box: &gtk4::Box,
    scope: PendingScope,
    session: &Rc<RefCell<TagEditSession>>,
    update: &UpdateCallback,
) {
    let session = session.clone();
    let rating_value_for_read = rating_value.clone();
    let on_changed: UpdateCallback = {
        let update = update.clone();
        Rc::new(move || {
            let value = rating_value_for_read.get();
            session
                .borrow_mut()
                .set_pending(scope, TagField::Rating, &FieldValue::Rating(value));
            update();
        })
    };
    wire_star_clicks(rating_box, rating_value, &on_changed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::library::tag_edit::EditableTags;
    use reprise_core::library::tag_edit_session::SessionTrack;
    use std::path::PathBuf;

    fn track(id: i64, title: &str) -> SessionTrack {
        SessionTrack {
            id,
            path: PathBuf::from(format!("/music/{id}.flac")),
            tags: EditableTags {
                title: title.into(),
                artist: "Artist".into(),
                album: "Album".into(),
                album_artist: "Artist".into(),
                year: Some(2020),
                track_no: Some(1),
                genre: "Rock".into(),
            },
            rating: 0,
        }
    }

    #[test]
    fn session_scope_maps_mode_one_to_one() {
        assert_eq!(session_scope(SessionMode::Multi), PendingScope::AllTracks);
        assert_eq!(
            session_scope(SessionMode::SingleNav),
            PendingScope::CurrentTrack
        );
    }

    #[test]
    fn parse_number_field_accepts_blank_rejects_zero_and_garbage() {
        assert_eq!(parse_number_field(""), Ok(None));
        assert_eq!(parse_number_field("  "), Ok(None));
        assert_eq!(parse_number_field("42"), Ok(Some(42)));
        assert!(parse_number_field("0").is_err());
        assert!(parse_number_field("abc").is_err());
    }

    #[test]
    fn tag_2_in_field_revert_clears_text_and_pending_together() {
        let mut session = TagEditSession::new(vec![track(1, "Original")], SessionMode::SingleNav);
        session.set_pending(
            PendingScope::CurrentTrack,
            TagField::Genre,
            &FieldValue::Text("Jazz".into()),
        );
        assert!(session
            .old_value_line(PendingScope::CurrentTrack, TagField::Genre)
            .is_some());

        let text = revert_field(&mut session, PendingScope::CurrentTrack, TagField::Genre);

        assert_eq!(text, "Rock");
        assert!(session
            .old_value_line(PendingScope::CurrentTrack, TagField::Genre)
            .is_none());
        assert!(session.write_batch().is_empty());
    }

    #[test]
    fn tag_2_in_field_revert_on_mixed_field_returns_to_placeholder_value() {
        let mut session =
            TagEditSession::new(vec![track(1, "A"), track(2, "B")], SessionMode::Multi);
        session.set_pending(
            PendingScope::AllTracks,
            TagField::Genre,
            &FieldValue::Text("Metal".into()),
        );
        assert!(session
            .old_value_line(PendingScope::AllTracks, TagField::Genre)
            .is_some());

        let _ = revert_field(&mut session, PendingScope::AllTracks, TagField::Genre);

        // Both tracks' genres started as "Rock" (see `track` fixture), so a
        // revert lands back on the uniform original — no longer mixed.
        assert!(session.mixed_placeholder(TagField::Genre).is_none());
        assert!(session
            .old_value_line(PendingScope::AllTracks, TagField::Genre)
            .is_none());
    }
}
