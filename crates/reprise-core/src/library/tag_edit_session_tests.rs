//! Test suite for `tag_edit_session.rs`, split into its own file per this
//! crate's 800-line-per-file rule (mirroring `scanner_tests.rs`).

use super::*;
use std::path::PathBuf;

fn track(id: i64, artist: &str, album: &str, genre: &str) -> SessionTrack {
    SessionTrack {
        id,
        path: PathBuf::from(format!("/music/{id}.flac")),
        tags: EditableTags {
            title: format!("Title {id}"),
            artist: artist.into(),
            album: album.into(),
            album_artist: artist.into(),
            year: Some(2020),
            track_no: Some(1),
            genre: genre.into(),
        },
        rating: 0,
    }
}

fn session_with_genres(genres: &[&str]) -> TagEditSession {
    let tracks = genres
        .iter()
        .enumerate()
        .map(|(index, genre)| track(index as i64 + 1, "Artist", "Album", genre))
        .collect();
    TagEditSession::new(tracks, SessionMode::Multi)
}

#[test]
fn tag_2_placeholder_lists_two_distinct_values_including_empty() {
    let session = session_with_genres(&["Deathcore", ""]);
    let placeholder = session.mixed_placeholder(TagField::Genre).unwrap();
    assert_eq!(placeholder.label, "Mixed — Deathcore, empty");
    assert_eq!(placeholder.distinct_count, 2);
}

#[test]
fn tag_2_placeholder_counts_three_or_more() {
    let session = session_with_genres(&["Ambient", "Post-Rock", "Jazz"]);
    let placeholder = session.mixed_placeholder(TagField::Genre).unwrap();
    assert_eq!(placeholder.label, "Mixed — 3 different values");
    assert_eq!(placeholder.distinct_count, 3);
}

#[test]
fn placeholder_is_none_when_every_track_agrees() {
    let session = session_with_genres(&["Ambient", "Ambient"]);
    assert!(session.mixed_placeholder(TagField::Genre).is_none());
}

#[test]
fn tag_2_clear_for_all_is_a_normal_pending_change() {
    let mut session = session_with_genres(&["Ambient", "Post-Rock"]);
    session.set_pending(
        PendingScope::AllTracks,
        TagField::Genre,
        &FieldValue::Text(String::new()),
    );

    // Now uniformly empty: no longer "mixed", a normal (if blank) value.
    assert!(session.mixed_placeholder(TagField::Genre).is_none());

    let batch = session.write_batch();
    assert_eq!(batch.len(), 2, "both tracks had a non-empty genre before");
    for write in &batch {
        assert_eq!(write.patch.tags.genre, Some(String::new()));
    }

    let summary = session.summary();
    assert_eq!(summary.fields, 1);
    assert_eq!(summary.tracks_affected, 2);
}

#[test]
fn tag_4_pending_survives_track_switch() {
    let mut session = TagEditSession::new(
        vec![
            track(1, "Artist A", "Album A", "Rock"),
            track(2, "Artist B", "Album B", "Jazz"),
        ],
        SessionMode::SingleNav,
    );
    session.set_current_track(1);
    session.set_pending(
        PendingScope::CurrentTrack,
        TagField::Title,
        &FieldValue::Text("New Title".into()),
    );

    // Browse away to track 2, then back to track 1 — the pending edit made
    // while track 1 was current must still be there.
    session.set_current_track(2);
    assert_eq!(
        session.effective_display(2, TagField::Title),
        Some("Title 2".into()),
        "track 2 was never edited"
    );
    session.set_current_track(1);
    assert_eq!(
        session.effective_display(1, TagField::Title),
        Some("New Title".into()),
        "pending edit on track 1 must survive the round trip"
    );
}

#[test]
fn tag_5_summary_counts_fields_and_affected_tracks() {
    let mut session = TagEditSession::new(
        vec![
            track(1, "Same Artist", "Album", "Rock"),
            track(2, "Same Artist", "Album", "Jazz"),
            track(3, "Same Artist", "Album", "Rock"),
        ],
        SessionMode::Multi,
    );
    // Genre bulk-set to "Metal": tracks 1 and 3 (currently "Rock") really
    // change; track 2 (already... no, track 2 is "Jazz") also changes.
    session.set_pending(
        PendingScope::AllTracks,
        TagField::Genre,
        &FieldValue::Text("Metal".into()),
    );
    // Artist bulk-set to the value all three already share: a no-op field.
    session.set_pending(
        PendingScope::AllTracks,
        TagField::Artist,
        &FieldValue::Text("Same Artist".into()),
    );

    let summary = session.summary();
    assert_eq!(summary.fields, 1, "only Genre has a real change");
    assert_eq!(summary.tracks_affected, 3, "every track's genre changed");
}

#[test]
fn tag_5_review_lines_count_only_real_changes() {
    let mut session = TagEditSession::new(
        vec![
            track(1, "Artist", "Album", "Ambient"),
            track(2, "Artist", "Album", "Post-Rock"),
            track(3, "Artist", "Album", "Techno"),
        ],
        SessionMode::Multi,
    );
    session.set_pending(
        PendingScope::AllTracks,
        TagField::Genre,
        &FieldValue::Text("Techno".into()),
    );

    let lines = session.review_lines();
    assert_eq!(lines.len(), 1);
    let line = &lines[0];
    assert_eq!(line.field, TagField::Genre);
    assert_eq!(line.new_display, "Techno");
    // Track 3 already was "Techno" — only tracks 1 and 2 really changed.
    assert_eq!(line.tracks_affected, 2);
    assert_eq!(line.old_display, "Ambient, Post-Rock");
}

#[test]
fn tag_5_exact_compare_no_trim() {
    let mut session = TagEditSession::new(
        vec![track(1, "Artist", "Album", "Rock")],
        SessionMode::SingleNav,
    );
    session.set_pending(
        PendingScope::CurrentTrack,
        TagField::Genre,
        &FieldValue::Text("Rock ".into()),
    );
    let batch = session.write_batch();
    assert_eq!(
        batch.len(),
        1,
        "trailing whitespace must count as a real change, not be trimmed away"
    );
    assert_eq!(batch[0].patch.tags.genre, Some("Rock ".into()));

    let mut casing_session = TagEditSession::new(
        vec![track(2, "Artist", "Album", "rock")],
        SessionMode::SingleNav,
    );
    casing_session.set_pending(
        PendingScope::CurrentTrack,
        TagField::Genre,
        &FieldValue::Text("Rock".into()),
    );
    assert_eq!(
        casing_session.write_batch().len(),
        1,
        "a mere case difference must count as a real change, not be folded away"
    );
}

#[test]
fn tag_5_all_pending_but_zero_effective_yields_empty_batch() {
    let mut session = TagEditSession::new(
        vec![track(1, "Artist", "Album", "Rock")],
        SessionMode::SingleNav,
    );
    // Armed, but set back to the exact original value: zero effective change.
    session.set_pending(
        PendingScope::CurrentTrack,
        TagField::Genre,
        &FieldValue::Text("Rock".into()),
    );
    session.set_pending(
        PendingScope::CurrentTrack,
        TagField::Artist,
        &FieldValue::Text("Artist".into()),
    );

    assert!(session.write_batch().is_empty());
    let summary = session.summary();
    assert_eq!(summary.fields, 0);
    assert_eq!(summary.tracks_affected, 0);
}

#[test]
fn mb_uniformity_uses_effective_values() {
    let mut session = TagEditSession::new(
        vec![
            track(1, "Suicide", "The Same Album", "Rock"),
            track(2, "Suicide Silence", "The Same Album", "Rock"),
        ],
        SessionMode::Multi,
    );
    assert_eq!(
        session.mb_uniform_artist_album(),
        None,
        "original artists differ"
    );

    session.set_pending(
        PendingScope::AllTracks,
        TagField::Artist,
        &FieldValue::Text("Suicide Silence".into()),
    );
    assert_eq!(
        session.mb_uniform_artist_album(),
        Some(("Suicide Silence".into(), "The Same Album".into()))
    );
}

#[test]
fn mb_uniformity_is_none_when_album_is_empty() {
    let session = TagEditSession::new(vec![track(1, "Artist", "", "Rock")], SessionMode::SingleNav);
    assert!(session.mb_uniform_artist_album().is_none());
}

#[test]
fn old_value_line_is_none_when_nothing_changed() {
    let session = TagEditSession::new(
        vec![track(1, "Artist", "Album", "Rock")],
        SessionMode::SingleNav,
    );
    assert!(session
        .old_value_line(PendingScope::CurrentTrack, TagField::Genre)
        .is_none());
}

#[test]
fn old_value_line_shows_the_original_once_armed() {
    let mut session = TagEditSession::new(
        vec![track(1, "Artist", "Album", "Rock")],
        SessionMode::SingleNav,
    );
    session.set_pending(
        PendingScope::CurrentTrack,
        TagField::Genre,
        &FieldValue::Text("Metal".into()),
    );
    assert_eq!(
        session.old_value_line(PendingScope::CurrentTrack, TagField::Genre),
        Some("Rock".into())
    );
}

#[test]
fn revert_clears_pending_and_restores_original_effective_value() {
    let mut session = TagEditSession::new(
        vec![track(1, "Artist", "Album", "Rock")],
        SessionMode::SingleNav,
    );
    session.set_pending(
        PendingScope::CurrentTrack,
        TagField::Genre,
        &FieldValue::Text("Metal".into()),
    );
    assert_eq!(session.pending_track_count(), 1);

    session.revert(PendingScope::CurrentTrack, TagField::Genre);
    assert_eq!(session.pending_track_count(), 0);
    assert_eq!(
        session.effective_display(1, TagField::Genre),
        Some("Rock".into())
    );
}

#[test]
fn rating_pending_participates_in_write_batch_and_summary() {
    let mut session = TagEditSession::new(
        vec![track(1, "Artist", "Album", "Rock")],
        SessionMode::SingleNav,
    );
    session.set_pending(
        PendingScope::CurrentTrack,
        TagField::Rating,
        &FieldValue::Rating(5),
    );
    let batch = session.write_batch();
    assert_eq!(batch.len(), 1);
    assert_eq!(batch[0].patch.rating, Some(5));
    assert_eq!(batch[0].patch.tags, TagPatch::default());
    assert_eq!(session.summary().tracks_affected, 1);
}

#[test]
fn mismatched_field_value_pairing_is_ignored_not_panicking() {
    let mut session = TagEditSession::new(
        vec![track(1, "Artist", "Album", "Rock")],
        SessionMode::SingleNav,
    );
    session.set_pending(
        PendingScope::CurrentTrack,
        TagField::Genre,
        &FieldValue::Rating(5),
    );
    assert!(session.write_batch().is_empty());
}
