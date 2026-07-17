//! Session-backed field wiring (F0/TAG-2/TAG-4) and the review footer
//! (F1/TAG-5): every field's GTK "changed"/click signal writes straight
//! into the shared `TagEditSession` — the single state truth. The old
//! per-field `Cell<bool>` dirty array is gone: it duplicated exactly what
//! the session already tracks (which fields differ from their original
//! value) and, being private to this module, was the reason Package C
//! could arm a Mixed field's *display* but never wire its in-field ↺
//! revert (a revert from outside this module could clear the visible text
//! without clearing the flag, silently writing an empty string over
//! genuinely different per-track values).
//!
//! The `update` callback this module returns is the single place that
//! recomputes every session-derived UI surface after any field edit:
//! Save-button sensitivity/label/tooltip, the per-field reserved "was: …"
//! lines, and the review footer's summary line + "Review changes" expander
//! (TAG-5). `interacted` tracks a narrower thing than the session does —
//! not "which fields differ" (the session's job) but "has the user ever
//! armed or reverted anything in this dialog" — the fact P-2's two disabled
//! tooltips ("No changes yet" vs "No effective changes") need to tell
//! apart and that no session query can answer, since a field armed and
//! then reverted back to its original value leaves zero effective diff
//! either way.

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
use crate::ui::tag_editor_form::{
    apply_mixed_field_presentation, mixed_field_presentation, EditorMode, TagEditorForm,
};
use crate::ui::tag_editor_state::{number_patch, ParseFieldError};
use crate::ui::tag_editor_widgets::wire_star_clicks;

pub(in crate::ui) type UpdateCallback = Rc<dyn Fn()>;

pub(in crate::ui) struct DirtyState {
    pub(in crate::ui) update: UpdateCallback,
}

#[derive(Clone)]
struct FieldWiring {
    scope: PendingScope,
    session: Rc<RefCell<TagEditSession>>,
    update: UpdateCallback,
    interacted: Rc<Cell<bool>>,
    suppress_changes: Rc<Cell<bool>>,
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

/// TAG-5: whether the "Review changes" expander should be shown. Multi
/// mode shows it whenever anything is effectively pending; SingleNav only
/// once pending reaches beyond the current track (`> 1` — a lone track can
/// only ever contribute 0 or 1, so this is really "did browsing pending
/// accumulate", Package G's territory).
pub(in crate::ui) fn review_expander_visible(mode: SessionMode, tracks_affected: usize) -> bool {
    match mode {
        SessionMode::Multi => tracks_affected > 0,
        SessionMode::SingleNav => tracks_affected > 1,
    }
}

/// TAG-5's Save-button label from the effective diff currency (tracks):
/// Multi always carries the count ("Save 30"); SingleNav stays plain
/// ("Save") unless pending has scattered onto more than the current track
/// ("Save · 2 tracks") — the same threshold as [`review_expander_visible`].
pub(in crate::ui) fn save_label(mode: SessionMode, tracks_affected: usize) -> String {
    match mode {
        SessionMode::Multi if tracks_affected > 0 => strings::tag_save_count(tracks_affected),
        SessionMode::SingleNav if tracks_affected > 1 => {
            strings::tag_save_scattered(tracks_affected)
        }
        _ => strings::text(strings::TAG_SAVE),
    }
}

/// P-2's disabled-Save-button reason: `interacted` distinguishes a
/// never-touched session ("No changes yet") from one where something was
/// armed or reverted but landed on zero effective diff ("No effective
/// changes") — see this module's doc comment for why the session itself
/// can't answer this.
pub(in crate::ui) fn save_disabled_tooltip(interacted: bool) -> &'static str {
    if interacted {
        strings::TAG_REVIEW_NO_EFFECTIVE_CHANGES
    } else {
        strings::TAG_SAVE_NO_CHANGES_YET
    }
}

fn field_display_name(field: TagField) -> String {
    match field {
        TagField::Title => strings::text(strings::TAG_TITLE),
        TagField::Artist => strings::text(strings::TAG_ARTIST),
        TagField::Album => strings::text(strings::TAG_ALBUM),
        TagField::AlbumArtist => strings::text(strings::TAG_ALBUM_ARTIST),
        TagField::Genre => strings::text(strings::TAG_GENRE),
        TagField::Year => strings::text(strings::TAG_YEAR),
        TagField::TrackNo => strings::text(strings::TAG_TRACK_NUMBER),
        TagField::Rating => strings::text(strings::RATING),
    }
}

/// Rebuilds the review footer's contents from scratch on every `update()`
/// call — the same "clear children, re-render" approach the pre-F0 pending
/// bar used, now driven entirely by session queries instead of dirty flags.
fn render_review_footer(
    review_box: &gtk4::Box,
    session: &TagEditSession,
    mode: SessionMode,
    interacted: bool,
) {
    while let Some(child) = review_box.first_child() {
        review_box.remove(&child);
    }

    let summary = session.summary();
    if summary.tracks_affected == 0 {
        if !interacted {
            review_box.set_visible(false);
            return;
        }
        let label = gtk4::Label::builder()
            .label(strings::text(strings::TAG_REVIEW_NO_EFFECTIVE_CHANGES))
            .xalign(0.0)
            .build();
        label.add_css_class("reprise-tag-review-summary");
        review_box.append(&label);
        review_box.set_visible(true);
        return;
    }

    review_box.set_visible(true);
    let summary_label = gtk4::Label::builder()
        .label(strings::tag_review_summary(
            summary.fields,
            summary.tracks_affected,
        ))
        .xalign(0.0)
        .build();
    summary_label.add_css_class("reprise-tag-review-summary");
    review_box.append(&summary_label);

    if !review_expander_visible(mode, summary.tracks_affected) {
        return;
    }

    let expander = gtk4::Expander::builder()
        .label(strings::text(strings::TAG_REVIEW_EXPANDER))
        .build();
    let list = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    for line in session.review_lines() {
        let text = strings::tag_review_line(
            &field_display_name(line.field),
            &line.old_display,
            &line.new_display,
            line.tracks_affected,
        );
        let label = gtk4::Label::builder()
            .label(&text)
            .xalign(0.0)
            .wrap(true)
            .build();
        list.append(&label);
    }
    expander.set_child(Some(&list));
    review_box.append(&expander);
}

pub(in crate::ui) fn wire(
    mode: EditorMode,
    form: &TagEditorForm,
    session: &Rc<RefCell<TagEditSession>>,
) -> DirtyState {
    let session_mode = if mode.is_multi() {
        SessionMode::Multi
    } else {
        SessionMode::SingleNav
    };
    let scope = session_scope(session_mode);
    let interacted = Rc::new(Cell::new(false));
    let suppress_changes = Rc::new(Cell::new(false));

    let update: UpdateCallback = {
        let save_button = form.save_btn.clone();
        let session = session.clone();
        let review_box = form.review_box.clone();
        let old_value_labels = form.old_value_labels.clone();
        let interacted = interacted.clone();
        Rc::new(move || {
            let session_ref = session.borrow();
            let tracks_affected = session_ref.pending_track_count();

            save_button.set_sensitive(tracks_affected > 0);
            save_button.set_label(&save_label(session_mode, tracks_affected));
            if tracks_affected == 0 {
                save_button.set_tooltip_text(Some(save_disabled_tooltip(interacted.get())));
            } else {
                save_button.set_tooltip_text(None);
            }

            for (field, label) in &old_value_labels {
                match session_ref.old_value_line(scope, *field) {
                    Some(old_text) => label.set_text(&strings::tag_old_value_line(&old_text)),
                    None => label.set_text(""),
                }
            }

            render_review_footer(&review_box, &session_ref, session_mode, interacted.get());
        })
    };

    let wiring = FieldWiring {
        scope,
        session: session.clone(),
        update: update.clone(),
        interacted,
        suppress_changes,
    };

    wire_text_field(&form.title_row, TagField::Title, &wiring, None);
    wire_number_field(
        &form.year_row,
        TagField::Year,
        &wiring,
        form.year_annotation.as_ref(),
    );
    wire_number_field(&form.track_no_row, TagField::TrackNo, &wiring, None);
    wire_autocomplete_field(
        &form.artist_ac,
        TagField::Artist,
        &wiring,
        form.artist_annotation.as_ref(),
    );
    wire_autocomplete_field(
        &form.album_ac,
        TagField::Album,
        &wiring,
        form.album_annotation.as_ref(),
    );
    wire_autocomplete_field(
        &form.album_artist_ac,
        TagField::AlbumArtist,
        &wiring,
        form.album_artist_annotation.as_ref(),
    );
    wire_autocomplete_field(
        &form.genre_ac,
        TagField::Genre,
        &wiring,
        form.genre_annotation.as_ref(),
    );
    wire_rating(
        &form.rating_value,
        &form.rating_box,
        &wiring,
        form.rating_annotation.as_ref(),
        mode.track_count(),
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
    wiring: &FieldWiring,
    mixed_annotation: Option<&gtk4::Label>,
) {
    if !row.is_editable() {
        // TAG-3: a per-track-locked field (Title/Track-number in Multi) is
        // never wired into the session — nothing can arm it from here.
        return;
    }
    let revert_btn = build_revert_button();
    row.add_suffix(&revert_btn);

    {
        let wiring = wiring.clone();
        let revert_btn = revert_btn.clone();
        row.connect_changed(move |entry| {
            if wiring.suppress_changes.get() {
                return;
            }
            wiring.interacted.set(true);
            let text = entry.text().to_string();
            wiring
                .session
                .borrow_mut()
                .set_pending(wiring.scope, field, &FieldValue::Text(text));
            let armed = field_is_armed(&wiring.session.borrow(), wiring.scope, field);
            revert_btn.set_visible(armed);
            (wiring.update)();
        });
    }
    {
        let wiring = wiring.clone();
        let row = row.clone();
        let mixed_annotation = mixed_annotation.cloned();
        revert_btn.connect_clicked(move |button| {
            wiring.interacted.set(true);
            let (text, presentation) = {
                let mut session_mut = wiring.session.borrow_mut();
                let text = revert_field(&mut session_mut, wiring.scope, field);
                let presentation = mixed_field_presentation(&session_mut, field);
                (text, presentation)
            };
            wiring.suppress_changes.set(true);
            apply_mixed_field_presentation(&row, mixed_annotation.as_ref(), presentation.as_ref());
            row.set_text(if presentation.is_some() { "" } else { &text });
            wiring.suppress_changes.set(false);
            button.set_visible(false);
            (wiring.update)();
        });
    }
}

fn wire_number_field(
    row: &adw::EntryRow,
    field: TagField,
    wiring: &FieldWiring,
    mixed_annotation: Option<&gtk4::Label>,
) {
    if !row.is_editable() {
        return;
    }
    let revert_btn = build_revert_button();
    row.add_suffix(&revert_btn);

    {
        let wiring = wiring.clone();
        let revert_btn = revert_btn.clone();
        row.connect_changed(move |entry| {
            if wiring.suppress_changes.get() {
                return;
            }
            wiring.interacted.set(true);
            if let Ok(value) = parse_number_field(&entry.text()) {
                wiring.session.borrow_mut().set_pending(
                    wiring.scope,
                    field,
                    &FieldValue::Number(value),
                );
            }
            let armed = field_is_armed(&wiring.session.borrow(), wiring.scope, field);
            revert_btn.set_visible(armed);
            (wiring.update)();
        });
    }
    {
        let wiring = wiring.clone();
        let row = row.clone();
        let mixed_annotation = mixed_annotation.cloned();
        revert_btn.connect_clicked(move |button| {
            wiring.interacted.set(true);
            let (text, presentation) = {
                let mut session_mut = wiring.session.borrow_mut();
                let text = revert_field(&mut session_mut, wiring.scope, field);
                let presentation = mixed_field_presentation(&session_mut, field);
                (text, presentation)
            };
            wiring.suppress_changes.set(true);
            apply_mixed_field_presentation(&row, mixed_annotation.as_ref(), presentation.as_ref());
            row.set_text(if presentation.is_some() { "" } else { &text });
            wiring.suppress_changes.set(false);
            button.set_visible(false);
            (wiring.update)();
        });
    }
}

fn wire_autocomplete_field(
    ac: &Rc<AutocompleteEntry>,
    field: TagField,
    wiring: &FieldWiring,
    mixed_annotation: Option<&gtk4::Label>,
) {
    let revert_btn = build_revert_button();
    ac.row().add_suffix(&revert_btn);

    {
        let wiring = wiring.clone();
        let ac_for_read = ac.clone();
        let revert_btn = revert_btn.clone();
        ac.connect_changed(move || {
            if wiring.suppress_changes.get() {
                return;
            }
            wiring.interacted.set(true);
            wiring.session.borrow_mut().set_pending(
                wiring.scope,
                field,
                &FieldValue::Text(ac_for_read.text()),
            );
            let armed = field_is_armed(&wiring.session.borrow(), wiring.scope, field);
            revert_btn.set_visible(armed);
            (wiring.update)();
        });
    }
    {
        let wiring = wiring.clone();
        let ac = ac.clone();
        let mixed_annotation = mixed_annotation.cloned();
        revert_btn.connect_clicked(move |button| {
            wiring.interacted.set(true);
            let (text, presentation) = {
                let mut session_mut = wiring.session.borrow_mut();
                let text = revert_field(&mut session_mut, wiring.scope, field);
                let presentation = mixed_field_presentation(&session_mut, field);
                (text, presentation)
            };
            wiring.suppress_changes.set(true);
            apply_mixed_field_presentation(
                ac.row(),
                mixed_annotation.as_ref(),
                presentation.as_ref(),
            );
            ac.set_text(if presentation.is_some() { "" } else { &text });
            wiring.suppress_changes.set(false);
            button.set_visible(false);
            (wiring.update)();
        });
    }
}

fn wire_rating(
    rating_value: &Rc<Cell<i32>>,
    rating_box: &gtk4::Box,
    wiring: &FieldWiring,
    annotation: Option<&gtk4::Label>,
    track_count: usize,
) {
    let wiring = wiring.clone();
    let rating_value_for_read = rating_value.clone();
    let annotation = annotation.cloned();
    let on_changed: UpdateCallback = {
        Rc::new(move || {
            wiring.interacted.set(true);
            if let Some(label) = &annotation {
                label.set_text(&strings::tag_will_apply(track_count));
                label.add_css_class("accent");
            }
            let value = rating_value_for_read.get();
            wiring.session.borrow_mut().set_pending(
                wiring.scope,
                TagField::Rating,
                &FieldValue::Rating(value),
            );
            (wiring.update)();
        })
    };
    wire_star_clicks(rating_box, rating_value, &on_changed);
}

#[cfg(test)]
#[path = "tag_editor_dirty_tests.rs"]
mod tests;
