use super::*;

use reprise_core::playback::PlaybackState;

const MARK: Option<TrackMark> = Some(TrackMark {
    track_id: 7,
    playing: true,
});

#[test]
fn pausing_keeps_the_track_marked_and_only_freezes_it() {
    assert_eq!(
        mark_for_state(MARK, PlaybackState::Paused),
        Some(TrackMark {
            track_id: 7,
            playing: false,
        })
    );
}

#[test]
fn resuming_marks_the_same_track_as_running_again() {
    let paused = mark_for_state(MARK, PlaybackState::Paused);
    assert_eq!(mark_for_state(paused, PlaybackState::Playing), MARK);
}

#[test]
fn stopping_clears_the_mark() {
    assert_eq!(mark_for_state(MARK, PlaybackState::Stopped), None);
    // An external podcast or radio session arrives as Stopped too, which is
    // exactly how the music rows lose the marker to it.
    assert_eq!(mark_for_state(None, PlaybackState::Stopped), None);
}

#[test]
fn a_state_change_without_a_loaded_track_marks_nothing() {
    assert_eq!(mark_for_state(None, PlaybackState::Playing), None);
    assert_eq!(mark_for_state(None, PlaybackState::Paused), None);
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
    // The card hands its rows to `SongRowPlayback` and never compares a track
    // id against the mark itself — one predicate, one place.
    assert!(card.contains("SongRowPlayback::new"));
    assert!(!card.contains("mark.track_id =="));
}
