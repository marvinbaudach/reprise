//! Which next track is pre-fed when a CUE track is involved (decisions 1
//! and 2 of the CUE surfaces plan): only the next track of the same file,
//! starting where the playing one ends, plays through — it is armed for the
//! boundary probe, never put in the URI slot. Every other pairing with a CUE
//! track on either side is not pre-fed at all; the playing track finishes
//! and the frontend starts the next one. Whole file to whole file is pre-fed
//! as always.

use super::segment_support::{count, cue_item, gain_element, linear, write_regions_wav, Harness};
use super::*;

const HANG_GUARD: Duration = Duration::from_secs(20);
const PAST_THE_FILE_END: Duration = Duration::from_secs(3);
const FIRST: (i64, i64) = (1_000, 2_500);
const FOLLOWING: (i64, i64) = (2_500, 4_000);
const ELSEWHERE: (i64, i64) = (4_000, 5_000);

fn queued(player: &Player) -> Option<QueuedTrack> {
    player
        .next_uri
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

fn finished(event: &PlayerEvent) -> bool {
    matches!(event, PlayerEvent::TrackFinished)
}

fn advanced(event: &PlayerEvent) -> bool {
    matches!(event, PlayerEvent::AdvancedToNext)
}

fn album(directory: &tempfile::TempDir, name: &str) -> std::path::PathBuf {
    let path = directory.path().join(name);
    write_regions_wav(&path, &[(6_000, true)]);
    path
}

#[test]
fn play_23a_a_whole_file_after_a_cue_track_is_not_prefed() {
    let harness = Harness::new();
    let directory = tempfile::tempdir().unwrap();
    let cue_file = album(&directory, "album.wav");
    let whole = album(&directory, "single.wav");

    harness
        .player
        .play(cue_item(&cue_file, FIRST, 0.0))
        .unwrap();
    harness.player.set_next(Some(item(whole.to_str().unwrap())));

    assert!(queued(&harness.player).is_none());
}

#[test]
fn play_23a_a_cue_track_after_a_whole_file_is_not_prefed() {
    let harness = Harness::new();
    let directory = tempfile::tempdir().unwrap();
    let cue_file = album(&directory, "album.wav");
    let whole = album(&directory, "single.wav");

    harness.player.play(item(whole.to_str().unwrap())).unwrap();
    harness
        .player
        .set_next(Some(cue_item(&cue_file, FIRST, 0.0)));

    assert!(queued(&harness.player).is_none());
}

#[test]
fn play_23a_a_whole_file_after_a_whole_file_is_still_prefed() {
    let harness = Harness::new();
    let directory = tempfile::tempdir().unwrap();
    let first = album(&directory, "first.wav");
    let second = album(&directory, "second.wav");

    harness.player.play(item(first.to_str().unwrap())).unwrap();
    harness
        .player
        .set_next(Some(item(second.to_str().unwrap())));

    let queued = queued(&harness.player).expect("a whole file is pre-fed");
    assert!(queued.uri.ends_with("second.wav"));
    assert_eq!(queued.segment, None);
}

/// A track of the same file that does not start where the playing one ends
/// is neither armed nor pre-fed: the playing track finishes at its end, once,
/// and nothing takes over by itself — not even at the file's end.
#[test]
fn play_23a_a_cue_track_that_does_not_follow_on_is_started_afresh() {
    let harness = Harness::new();
    let directory = tempfile::tempdir().unwrap();
    let cue_file = album(&directory, "album.wav");

    harness
        .player
        .play(cue_item(&cue_file, FIRST, 0.0))
        .unwrap();
    harness
        .player
        .set_next(Some(cue_item(&cue_file, ELSEWHERE, 0.0)));
    assert!(queued(&harness.player).is_none());
    let mut events = harness.pump_until(HANG_GUARD, |events| count(events, finished) > 0);
    events.extend(harness.pump_for(PAST_THE_FILE_END));

    assert_eq!(count(&events, finished), 1);
    assert_eq!(count(&events, advanced), 0);
}

/// A live ReplayGain change re-feeds the track the frontend still sees as
/// next. When the boundary probe has already handed over to it, the new gain
/// must reach the track that is now playing.
#[test]
fn play_23a_a_refed_gain_reaches_the_track_already_handed_over() {
    const FIRST_GAIN_DB: f64 = -6.0;
    const STALE_GAIN_DB: f64 = 3.0;
    const REFRESHED_GAIN_DB: f64 = -2.0;
    let harness = Harness::new();
    let directory = tempfile::tempdir().unwrap();
    let cue_file = album(&directory, "album.wav");

    harness
        .player
        .play(cue_item(&cue_file, FIRST, FIRST_GAIN_DB))
        .unwrap();
    harness
        .player
        .set_next(Some(cue_item(&cue_file, FOLLOWING, STALE_GAIN_DB)));
    let events = harness.pump_until(HANG_GUARD, |events| count(events, advanced) > 0);
    assert_eq!(count(&events, advanced), 1, "expected the hand-off");

    harness
        .player
        .set_next(Some(cue_item(&cue_file, FOLLOWING, REFRESHED_GAIN_DB)));

    let volume = gain_element(&harness.player).property::<f64>("volume");
    assert!(
        (volume - linear(REFRESHED_GAIN_DB)).abs() < 1e-6,
        "the playing track must take the re-fed gain, got {volume}"
    );
    assert!(queued(&harness.player).is_none());
}
