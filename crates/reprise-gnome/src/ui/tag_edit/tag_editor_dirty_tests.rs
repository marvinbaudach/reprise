//! Test suite for `tag_editor_dirty.rs`, split into its own file per this
//! crate's 800-line-per-file rule (mirroring `tag_edit_session_tests.rs`).

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

/// The exact shape of the bug that deleted years off disk: three albums with
/// three different years selected together, the Year field therefore Mixed
/// (and, per TAG-2, blank), and the only edit the user made is the rating.
/// Saving must not turn that blank field into "clear the year everywhere" —
/// no track's patch may carry a `year` at all, or `tag_mutation::
/// set_patch_fields` calls `remove_date()` and the DATE/YEAR tags are gone.
#[test]
fn blank_mixed_year_is_not_committed_when_only_the_rating_changed() {
    let mut burn = track(1, "No Escape");
    burn.tags.year = Some(2026);
    let mut omni = track(2, "WAKE UP");
    omni.tags.year = Some(2024);
    let mut overlord = track(3, "OVERLORD");
    overlord.tags.year = Some(2025);
    let mut session = TagEditSession::new(vec![burn, omni, overlord], SessionMode::Multi);
    let scope = PendingScope::AllTracks;
    assert!(
        session.mixed_placeholder(TagField::Year).is_some(),
        "precondition: three different years must render the field as Mixed"
    );

    // The user only touches the stars; the Year field is never armed.
    session.set_pending(scope, TagField::Rating, &FieldValue::Rating(5));
    commit_number_field_on_save(&mut session, scope, TagField::Year, "", None);

    let batch = session.write_batch();
    assert_eq!(batch.len(), 3);
    for write in &batch {
        assert_eq!(
            write.patch.tags.year, None,
            "a blank Mixed year must stay untouched, not be cleared on disk"
        );
        assert!(
            write.patch.tags.is_empty(),
            "a rating-only edit must write no tag fields at all (file stays untouched)"
        );
        assert_eq!(write.patch.rating, Some(5));
    }
}

/// The other half of the guard: clearing a year on purpose must still work.
/// The live "changed" wiring arms the field when the user empties it, so the
/// blank text is a real instruction here — not the Mixed placeholder's silence.
#[test]
fn deliberately_cleared_year_still_commits_as_a_removal() {
    let mut session = TagEditSession::new(vec![track(1, "A"), track(2, "B")], SessionMode::Multi);
    let scope = PendingScope::AllTracks;
    // Both tracks share year 2020, so the field shows a real value that the
    // user then deletes — the live wiring pushes that blank straight away.
    session.set_pending(scope, TagField::Year, &FieldValue::Number(None));

    commit_number_field_on_save(&mut session, scope, TagField::Year, "", None);

    let batch = session.write_batch();
    assert_eq!(batch.len(), 2);
    for write in &batch {
        assert_eq!(write.patch.tags.year, Some(None));
    }
}

/// Typing a year into a Mixed field is a genuine bulk edit and must reach
/// every track — the guard keys on blankness, not on "the field was Mixed".
#[test]
fn typed_year_commits_across_a_mixed_selection() {
    let mut early = track(1, "A");
    early.tags.year = Some(1999);
    let mut late = track(2, "B");
    late.tags.year = Some(2024);
    let mut session = TagEditSession::new(vec![early, late], SessionMode::Multi);
    let scope = PendingScope::AllTracks;

    commit_number_field_on_save(&mut session, scope, TagField::Year, "2030", Some(2030));

    let batch = session.write_batch();
    assert_eq!(batch.len(), 2);
    for write in &batch {
        assert_eq!(write.patch.tags.year, Some(Some(2030)));
    }
}

/// Same guard, same reasoning, for the second save-time number field: a Multi
/// selection's track numbers are always Mixed, so a blank one must never be
/// committed as "renumber every track to nothing".
#[test]
fn blank_mixed_track_number_is_not_committed() {
    let mut first = track(1, "A");
    first.tags.track_no = Some(1);
    let mut second = track(2, "B");
    second.tags.track_no = Some(2);
    let mut session = TagEditSession::new(vec![first, second], SessionMode::Multi);
    let scope = PendingScope::AllTracks;

    session.set_pending(scope, TagField::Rating, &FieldValue::Rating(4));
    commit_number_field_on_save(&mut session, scope, TagField::TrackNo, "", None);

    for write in &session.write_batch() {
        assert_eq!(write.patch.tags.track_no, None);
    }
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
    let mut ambient = track(1, "A");
    ambient.tags.genre = "Ambient".into();
    let mut post_rock = track(2, "B");
    post_rock.tags.genre = "Post-Rock".into();
    let mut session = TagEditSession::new(vec![ambient, post_rock], SessionMode::Multi);
    session.set_pending(
        PendingScope::AllTracks,
        TagField::Genre,
        &FieldValue::Text("Metal".into()),
    );
    assert!(session
        .old_value_line(PendingScope::AllTracks, TagField::Genre)
        .is_some());

    let _ = revert_field(&mut session, PendingScope::AllTracks, TagField::Genre);

    let presentation = mixed_field_presentation(&session, TagField::Genre).unwrap();
    assert_eq!(presentation.entry_placeholder, "Mixed — Ambient, Post-Rock");
    assert_eq!(presentation.annotation, "2 values");
    assert!(session
        .old_value_line(PendingScope::AllTracks, TagField::Genre)
        .is_none());
}

#[test]
fn tag_5_summary_line_and_expander_track_currency() {
    let mut session = TagEditSession::new(
        vec![track(1, "A"), track(2, "B"), track(3, "C")],
        SessionMode::Multi,
    );
    session.set_pending(
        PendingScope::AllTracks,
        TagField::Genre,
        &FieldValue::Text("Metal".into()),
    );

    let summary = session.summary();
    assert_eq!(
        strings::tag_review_summary(summary.fields, summary.tracks_affected),
        "1 field \u{b7} 3 tracks affected"
    );
    // The save label and the expander-visibility gate read the exact same
    // `tracks_affected` currency the summary line just printed — TAG-5's
    // "tracks = real file writes" promise, checked end to end rather than
    // trusting three separate call sites to agree by convention.
    assert_eq!(
        save_label(SessionMode::Multi, summary.tracks_affected),
        "Save 3"
    );
    assert!(review_expander_visible(
        SessionMode::Multi,
        summary.tracks_affected
    ));

    // A lone SingleNav track can only ever contribute 0 or 1 to
    // `tracks_affected` — never enough to cross the "> 1" scattered-pending
    // threshold that would otherwise show the expander / "Save · N tracks".
    let mut single = TagEditSession::new(vec![track(1, "A")], SessionMode::SingleNav);
    single.set_pending(
        PendingScope::CurrentTrack,
        TagField::Genre,
        &FieldValue::Text("Metal".into()),
    );
    let single_affected = single.pending_track_count();
    assert_eq!(single_affected, 1);
    assert_eq!(save_label(SessionMode::SingleNav, single_affected), "Save");
    assert!(!review_expander_visible(
        SessionMode::SingleNav,
        single_affected
    ));
}

#[test]
fn tag_5_save_disabled_with_tooltip_when_zero_effective() {
    // Never touched: the honest reason is "nothing happened yet".
    assert_eq!(save_disabled_tooltip(false), "No changes yet");

    // Touched, but landed back on zero effective diff (armed then
    // reverted) — a *different* honest reason, not "nothing happened".
    let mut session = TagEditSession::new(vec![track(1, "A"), track(2, "B")], SessionMode::Multi);
    session.set_pending(
        PendingScope::AllTracks,
        TagField::Genre,
        &FieldValue::Text("Metal".into()),
    );
    session.set_pending(
        PendingScope::AllTracks,
        TagField::Genre,
        &FieldValue::Text("Rock".into()), // both tracks' original genre
    );
    assert_eq!(session.pending_track_count(), 0);
    assert_eq!(save_disabled_tooltip(true), "No effective changes");
}

#[test]
fn browsing_without_editing_keeps_the_untouched_tooltip() {
    let interacted = interaction_after_change(false, true);

    assert!(!interacted);
    assert_eq!(save_disabled_tooltip(interacted), "No changes yet");
}
