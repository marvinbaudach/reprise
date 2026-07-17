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
    let mut session = TagEditSession::new(vec![track(1, "A"), track(2, "B")], SessionMode::Multi);
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
