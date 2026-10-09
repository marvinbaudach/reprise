//! A CUE track starts at its own start, and its clock and seeks are its own
//! (PLAY-22). Headless against a real `playbin3` on `fakesink`; the file is
//! silence with a tone exactly where the track starts, so the first buffer
//! heard proves where the decoder really is.

use super::segment_support::{cue_item, record_heard, ticks, write_regions_wav, Harness};
use super::*;
use crate::player::segment::Cut;

/// How far the first sample heard may lie from the track's start.
const START_TOLERANCE_MS: i64 = 20;
const TRACK_START_MS: i64 = 2_000;
const TRACK_END_MS: i64 = 4_000;
const TRACK_LENGTH_MS: i64 = TRACK_END_MS - TRACK_START_MS;
/// Generous: under a loaded parallel test run the pipeline can take a while.
const SETTLE: Duration = Duration::from_secs(20);
/// The preroll at the file's start, then the seek to the track's start.
const SEGMENTS_AFTER_START: usize = 2;

/// Silence, the track's tone, silence: six seconds in all.
fn tone_in_the_middle(directory: &tempfile::TempDir) -> std::path::PathBuf {
    let path = directory.path().join("album.wav");
    write_regions_wav(&path, &[(2_000, false), (2_000, true), (2_000, false)]);
    path
}

#[test]
fn play_22_a_cue_track_is_heard_from_its_own_start() {
    let harness = Harness::new();
    let directory = tempfile::tempdir().unwrap();
    let album = tone_in_the_middle(&directory);
    let heard = record_heard(&harness.player);

    harness
        .player
        .play(cue_item(&album, (TRACK_START_MS, TRACK_END_MS), 0.0))
        .unwrap();
    harness.pump_until(SETTLE, |_| heard.heard_after(SEGMENTS_AFTER_START));

    let first = heard.first();
    assert!(
        (first.start_ms - TRACK_START_MS).abs() <= START_TOLERANCE_MS,
        "the first sample heard must be the track's own start, got {first:?}"
    );
    assert!(
        first.audible,
        "the first buffer heard must carry the track's tone"
    );
}

#[test]
fn play_22_the_clock_of_a_cue_track_is_its_own() {
    let harness = Harness::new();
    let directory = tempfile::tempdir().unwrap();
    let album = tone_in_the_middle(&directory);

    harness
        .player
        .play(cue_item(&album, (TRACK_START_MS, TRACK_END_MS), 0.0))
        .unwrap();
    let events = harness.pump_for(Duration::from_millis(2_800));

    let ticks = ticks(&events);
    assert!(ticks.len() >= 4, "expected several ticks, got {ticks:?}");
    for &(position_ms, duration_ms) in &ticks {
        assert_eq!(duration_ms, TRACK_LENGTH_MS, "ticks were {ticks:?}");
        assert!(
            (0..=TRACK_LENGTH_MS).contains(&position_ms),
            "a tick left the track: {ticks:?}"
        );
    }
    assert!(
        ticks[0].0 < TRACK_START_MS / 2,
        "the first tick must count from the track's start, not the file's: {ticks:?}"
    );
}

#[test]
fn play_22_a_seek_lands_inside_the_cue_track() {
    let harness = Harness::new();
    let directory = tempfile::tempdir().unwrap();
    let album = tone_in_the_middle(&directory);
    let heard = record_heard(&harness.player);

    harness
        .player
        .play(cue_item(&album, (TRACK_START_MS, TRACK_END_MS), 0.0))
        .unwrap();
    // A seek before the start-seek's own `ASYNC_DONE` only retargets that
    // start, so the cases below begin once the track is really playing.
    harness.pump_until(SETTLE, |events| {
        heard.heard_after(SEGMENTS_AFTER_START)
            && events
                .iter()
                .any(|event| matches!(event, PlayerEvent::StateChanged(PlaybackState::Playing)))
    });

    let cases = [
        (1_000, TRACK_START_MS + 1_000),
        (-500, TRACK_START_MS),
        (60_000, TRACK_END_MS - 1),
    ];
    for (requested_ms, expected_file_ms) in cases {
        let before = heard.segments();
        harness.player.seek_to(requested_ms).unwrap();
        harness.pump_until(SETTLE, |_| heard.heard_after(before + 1));
        let first = heard.first();
        assert!(
            (first.start_ms - expected_file_ms).abs() <= START_TOLERANCE_MS,
            "a seek to {requested_ms} must land at file position {expected_file_ms}, \
             got {first:?}"
        );
        assert!(
            first.start_ms < TRACK_END_MS,
            "a seek must never land past the track's end, got {first:?}"
        );
    }
}

/// The start finishes on the bus, so a seek can arrive while the file is still
/// prerolling. There is nothing to seek in yet: it must not fail, and the track
/// must then start where the seek asked, not at its start.
#[test]
fn play_22_a_seek_before_the_start_has_finished_moves_the_start() {
    let harness = Harness::new();
    let directory = tempfile::tempdir().unwrap();
    let album = tone_in_the_middle(&directory);
    let heard = record_heard(&harness.player);

    harness
        .player
        .play(cue_item(&album, (TRACK_START_MS, TRACK_END_MS), 0.0))
        .unwrap();
    harness.player.seek_to(1_000).unwrap();
    harness.pump_until(SETTLE, |_| heard.heard_after(SEGMENTS_AFTER_START));

    let first = heard.first();
    assert!(
        (first.start_ms - (TRACK_START_MS + 1_000)).abs() <= START_TOLERANCE_MS,
        "the track must start at the sought position, got {first:?}"
    );
}

#[test]
fn play_22_a_whole_file_still_reports_and_seeks_in_file_time() {
    let harness = Harness::new();
    let directory = tempfile::tempdir().unwrap();
    let album = tone_in_the_middle(&directory);

    harness.player.play(item(album.to_str().unwrap())).unwrap();
    harness.pump_until(SETTLE, |events| !ticks(events).is_empty());
    harness.player.seek_to(3_000).unwrap();
    let events = harness.pump_for(Duration::from_millis(1_200));

    let ticks = ticks(&events);
    assert!(!ticks.is_empty());
    for &(position_ms, duration_ms) in &ticks {
        assert!(
            (5_900..=6_100).contains(&duration_ms),
            "ticks were {ticks:?}"
        );
        assert!(position_ms >= 2_500, "ticks were {ticks:?}");
    }
}

#[test]
fn a_cut_reports_relative_to_its_start_and_clamps_into_itself() {
    let cut = Cut::new(10_000, 20_000, Some(60_000));
    assert!(!cut.open_end);
    assert_eq!(cut.relative(9_800, Some(60_000)), (0, 10_000));
    assert_eq!(cut.relative(15_000, Some(60_000)), (5_000, 10_000));
    assert_eq!(cut.relative(20_300, Some(60_000)), (10_000, 10_000));
    assert_eq!(cut.seek_target_ms(-1, Some(60_000)), 10_000);
    assert_eq!(cut.seek_target_ms(99_999, Some(60_000)), 19_999);
}

#[test]
fn a_cut_ending_near_the_file_end_is_open_and_runs_to_it() {
    let cut = Cut::new(50_000, 59_600, Some(60_000));
    assert!(cut.open_end);
    assert_eq!(cut.relative(59_900, Some(60_000)), (9_900, 10_000));
    assert!(!Cut::new(50_000, 58_900, Some(60_000)).open_end);
    assert!(!Cut::new(50_000, 59_600, None).open_end);
}
