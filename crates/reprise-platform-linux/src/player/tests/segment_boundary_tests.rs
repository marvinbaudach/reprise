//! A CUE track ends at its own end (PLAY-22), the last track of a file plays
//! to the file's end, and the next track of the same file takes over inside
//! the file with its own gain (PLAY-23). Headless against a real `playbin3` on
//! `fakesink`, judged by what leaves the gain element and what the frontend
//! receives.

use super::segment_support::{
    count, cue_item, linear, record_heard, ticks, write_regions_wav, Harness,
};
use super::*;
use reprise_core::library::settings::TrackTransition;

/// Generous: under a loaded parallel test run the pipeline can take a while.
const HANG_GUARD: Duration = Duration::from_secs(20);
/// Long enough for a file's remaining seconds to run out after a boundary.
const PAST_THE_FILE_END: Duration = Duration::from_secs(3);

fn finished(event: &PlayerEvent) -> bool {
    matches!(event, PlayerEvent::TrackFinished)
}

fn advanced(event: &PlayerEvent) -> bool {
    matches!(event, PlayerEvent::AdvancedToNext)
}

#[test]
fn play_22_a_cue_track_is_not_heard_past_its_end() {
    const START_MS: i64 = 1_000;
    const END_MS: i64 = 2_500;
    let harness = Harness::new();
    let directory = tempfile::tempdir().unwrap();
    let album = directory.path().join("album.wav");
    // The tone runs on past the track's end: anything heard there is a leak.
    write_regions_wav(&album, &[(1_000, false), (3_000, true)]);
    let heard = record_heard(&harness.player);

    harness
        .player
        .play(cue_item(&album, (START_MS, END_MS), 0.0))
        .unwrap();
    let mut events = harness.pump_until(HANG_GUARD, |events| count(events, finished) > 0);
    events.extend(harness.pump_for(PAST_THE_FILE_END));

    assert_eq!(
        count(&events, finished),
        1,
        "the track must finish exactly once, at its end and not again at the file's"
    );
    let buffers = heard.buffers();
    let last = buffers.last().expect("expected buffers to be heard");
    assert!(
        buffers.iter().all(|buffer| buffer.start_ms < END_MS),
        "a buffer past the track's end was heard: {last:?}"
    );
    assert!(
        last.start_ms >= END_MS - 100,
        "the track must be heard up to its end, last heard {last:?}"
    );
    let length_ms = END_MS - START_MS;
    assert!(ticks(&events)
        .iter()
        .all(|&(position_ms, duration_ms)| duration_ms == length_ms && position_ms <= length_ms));
}

#[test]
fn play_22_the_last_track_of_a_file_plays_to_the_file_end() {
    const START_MS: i64 = 2_000;
    // 400 ms short of the file's 4 s, as a probed metadata duration can be.
    const END_MS: i64 = 3_600;
    const FILE_MS: i64 = 4_000;
    let harness = Harness::new();
    let directory = tempfile::tempdir().unwrap();
    let album = directory.path().join("album.wav");
    write_regions_wav(&album, &[(FILE_MS as u32, true)]);
    let heard = record_heard(&harness.player);

    harness
        .player
        .play(cue_item(&album, (START_MS, END_MS), 0.0))
        .unwrap();
    let mut events = harness.pump_until(HANG_GUARD, |events| count(events, finished) > 0);
    events.extend(harness.pump_for(Duration::from_millis(500)));

    assert_eq!(count(&events, finished), 1);
    let last = *heard
        .buffers()
        .last()
        .expect("expected buffers to be heard");
    assert!(
        last.start_ms >= END_MS + 200,
        "the file's last track must play past its metadata end to the file's end, \
         last heard {last:?}"
    );
    let ticks = ticks(&events);
    assert!(!ticks.is_empty());
    assert!(
        ticks
            .iter()
            .all(|&(_, duration_ms)| duration_ms == FILE_MS - START_MS),
        "an open-ended track lasts to the file's end: {ticks:?}"
    );
}

#[test]
fn play_23_contiguous_tracks_of_one_file_hand_over_inside_it() {
    assert_contiguous_hand_over(TrackTransition::Gapless);
}

/// Crossfade mode does not fade between two tracks of one file: they play
/// through exactly as in Gapless mode.
#[test]
fn play_23_contiguous_tracks_play_through_in_crossfade_mode_too() {
    assert_contiguous_hand_over(TrackTransition::Crossfade);
}

fn assert_contiguous_hand_over(transition: TrackTransition) {
    const FIRST: (i64, i64) = (1_000, 3_000);
    const SECOND: (i64, i64) = (3_000, 5_500);
    const FIRST_GAIN_DB: f64 = -6.0;
    const SECOND_GAIN_DB: f64 = 6.0;
    const CROSSFADE_SECONDS: u8 = 1;
    let harness = Harness::new();
    harness.player.set_transition(transition, CROSSFADE_SECONDS);
    let directory = tempfile::tempdir().unwrap();
    let album = directory.path().join("album.wav");
    write_regions_wav(&album, &[(7_000, true)]);
    let heard = record_heard(&harness.player);

    harness
        .player
        .play(cue_item(&album, FIRST, FIRST_GAIN_DB))
        .unwrap();
    harness
        .player
        .set_next(Some(cue_item(&album, SECOND, SECOND_GAIN_DB)));
    let mut events = harness.pump_until(HANG_GUARD, |_| heard.heard_after(2));
    let stream_starts = heard.stream_starts();
    let already_advanced = count(&events, advanced) > 0;
    events.extend(harness.pump_until(HANG_GUARD, |events| {
        already_advanced || count(events, advanced) > 0
    }));
    events.extend(harness.pump_for(Duration::from_millis(1_200)));

    assert_eq!(count(&events, advanced), 1, "exactly one hand-off");
    assert!(
        harness
            .player
            .incoming
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_none(),
        "no second pipeline may start between two tracks of one file"
    );
    assert_eq!(count(&events, finished), 0, "a hand-off is not a finish");
    assert_eq!(
        heard.stream_starts(),
        stream_starts,
        "the hand-off stays inside the stream, it starts no new one"
    );

    let buffers = heard.buffers();
    assert!(buffers
        .iter()
        .any(|buffer| buffer.start_ms >= SECOND.0 + 500));
    let wrong_gain: Vec<_> = buffers
        .iter()
        .filter(|buffer| {
            let expected = if buffer.start_ms < SECOND.0 {
                linear(FIRST_GAIN_DB)
            } else {
                linear(SECOND_GAIN_DB)
            };
            (buffer.linear_gain - expected).abs() > 1e-6
        })
        .collect();
    assert!(
        wrong_gain.len() <= 1,
        "every buffer must carry its own track's gain, give or take the one at the \
         boundary: {wrong_gain:?}"
    );

    let handoff = events.iter().position(advanced).unwrap();
    let (before, after) = (ticks(&events[..handoff]), ticks(&events[handoff..]));
    assert!(
        !after.is_empty(),
        "expected ticks after the hand-off: {events:?}"
    );
    assert!(
        before
            .iter()
            .all(|&(_, duration_ms)| duration_ms == FIRST.1 - FIRST.0),
        "ticks before the hand-off describe the first track: {before:?}"
    );
    assert!(
        after
            .iter()
            .all(|&(_, duration_ms)| duration_ms == SECOND.1 - SECOND.0),
        "no tick computed against the first track may follow the hand-off: {after:?}"
    );
}
