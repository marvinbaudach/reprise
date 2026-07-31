use super::*;

#[test]
fn the_loaded_row_toggles_and_every_other_row_starts() {
    let running = Some(TrackMark {
        track_id: 7,
        playing: true,
    });
    let paused = Some(TrackMark {
        track_id: 7,
        playing: false,
    });

    assert_eq!(activation_for(running, 7), Activation::TogglePause);
    assert_eq!(activation_for(paused, 7), Activation::TogglePause);
    assert_eq!(activation_for(running, 8), Activation::Start);
    assert_eq!(activation_for(None, 7), Activation::Start);
}

#[test]
fn stats_18_rows_mark_through_the_shared_marker() {
    let source = include_str!("stats_songs_playback.rs");
    assert!(source.contains("playing_marker::build"));
    assert!(source.contains("playing_marker::set_playing"));
    // The animated equaliser is only ever reached through the shared marker,
    // never rebuilt locally — see `playing_marker.rs`'s NAV-10a test.
    assert!(!source.contains("eq_bars::build"));
}

#[test]
fn stats_18_the_card_owns_no_second_playing_predicate() {
    let card = include_str!("stats_songs_card.rs");
    assert!(card.contains("stats_songs_playback::activation_for"));
    // The glyph and the click must not be decided independently: the card
    // asks `activation_for` and never compares track ids against the mark
    // itself.
    assert!(!card.contains("mark.track_id =="));
}
