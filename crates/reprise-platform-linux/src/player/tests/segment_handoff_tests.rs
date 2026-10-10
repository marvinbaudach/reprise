//! The next track of a CUE file takes over when it is heard (PLAY-23a): the
//! boundary probe meets the boundary about a second ahead of the audio sink, so
//! the hand-off it detects is announced only once the sink renders it.
//! Headless against a real `playbin3` whose sink renders on the clock behind an
//! emulated ring buffer, so the gap between "seen" and "heard" is real.

use std::sync::atomic::AtomicI64;

use super::segment_support::{
    count, cue_item, linear, record_heard, slow_sink_into, ticks, write_regions_wav, Harness,
    HeardLog, RenderedLog,
};
use super::*;

/// Generous: under a loaded parallel test run the pipeline can take a while.
const HANG_GUARD: Duration = Duration::from_secs(20);
const FIRST: (i64, i64) = (1_000, 3_000);
const SECOND: (i64, i64) = (3_000, 5_500);
const FIRST_GAIN_DB: f64 = -6.0;
const SECOND_GAIN_DB: f64 = 6.0;
/// How far ahead of the rendered boundary the hand-off may be announced: the
/// sink renders whole buffers, so its log trails the clock by up to one.
const EARLY_TOLERANCE_MS: i64 = 100;
const NOT_YET_RENDERED: i64 = -1;

/// How often a scenario that must act inside the staged window is tried
/// afresh when a loaded machine let the window pass before the test acted.
const WINDOW_ATTEMPTS: usize = 3;

fn advanced(event: &PlayerEvent) -> bool {
    matches!(event, PlayerEvent::AdvancedToNext)
}

fn finished(event: &PlayerEvent) -> bool {
    matches!(event, PlayerEvent::TrackFinished)
}

/// A harness on the slow sink that notes how far the sink had rendered at the
/// instant each `AdvancedToNext` was emitted.
fn harness_noting_the_render_at_hand_off() -> (Harness, Arc<AtomicI64>) {
    let rendered = RenderedLog::default();
    let at_hand_off = Arc::new(AtomicI64::new(NOT_YET_RENDERED));
    let harness = Harness::observing({
        let rendered = rendered.clone();
        let at_hand_off = at_hand_off.clone();
        move |event| {
            if advanced(event) {
                at_hand_off.store(rendered.end_ms(), Ordering::SeqCst);
            }
        }
    });
    slow_sink_into(&harness.player, &rendered);
    (harness, at_hand_off)
}

fn play_two_contiguous_tracks(harness: &Harness, album: &std::path::Path) {
    harness
        .player
        .play(cue_item(album, FIRST, FIRST_GAIN_DB))
        .unwrap();
    harness
        .player
        .set_next(Some(cue_item(album, SECOND, SECOND_GAIN_DB)));
}

#[test]
fn play_23a_the_next_cue_track_takes_over_when_its_first_sample_is_heard() {
    let (harness, at_hand_off) = harness_noting_the_render_at_hand_off();
    let directory = tempfile::tempdir().unwrap();
    let album = directory.path().join("album.wav");
    write_regions_wav(&album, &[(7_000, true)]);

    play_two_contiguous_tracks(&harness, &album);
    let mut events = harness.pump_until(HANG_GUARD, |events| count(events, advanced) > 0);
    events.extend(harness.pump_for(Duration::from_millis(1_200)));

    assert_eq!(count(&events, advanced), 1, "exactly one hand-off");
    let rendered_ms = at_hand_off.load(Ordering::SeqCst);
    assert!(
        rendered_ms >= FIRST.1 - EARLY_TOLERANCE_MS,
        "the hand-off was announced with the sink having rendered only up to \
         {rendered_ms} ms of the {} ms boundary",
        FIRST.1
    );
    let handoff = events.iter().position(advanced).unwrap();
    let after = ticks(&events[handoff..]);
    let zeros = after.iter().filter(|&&(position_ms, _)| position_ms == 0);
    assert!(
        zeros.count() <= 1,
        "the next track's clock must run from the moment it takes over, \
         not stand at zero while the previous one is still heard: {after:?}"
    );
}

/// One run that got into the window between the probe staging the hand-off
/// and the sink rendering it, ready for the scenario to act in.
struct StagedWindow {
    harness: Harness,
    at_hand_off: Arc<AtomicI64>,
    heard: HeardLog,
    album: std::path::PathBuf,
    _directory: tempfile::TempDir,
}

/// Plays the first track with the second armed and returns once the probe
/// has met the boundary but the hand-off is still unheard — tried afresh up
/// to [`WINDOW_ATTEMPTS`] times, because the window is only as long as the
/// sink's lead and a loaded machine can let the test thread miss it. Only a
/// hand-off announced as soon as the probe meets the boundary misses it
/// every time.
fn staged_window() -> StagedWindow {
    for _ in 0..WINDOW_ATTEMPTS {
        let (harness, at_hand_off) = harness_noting_the_render_at_hand_off();
        let directory = tempfile::tempdir().unwrap();
        let album = directory.path().join("album.wav");
        write_regions_wav(&album, &[(7_000, true)]);
        let heard = record_heard(&harness.player);
        play_two_contiguous_tracks(&harness, &album);
        let early = harness.pump_until(HANG_GUARD, |events| {
            count(events, advanced) > 0
                || heard
                    .buffers()
                    .iter()
                    .any(|buffer| buffer.start_ms >= SECOND.0)
        });
        if count(&early, advanced) == 0 && at_hand_off.load(Ordering::SeqCst) == NOT_YET_RENDERED {
            return StagedWindow {
                harness,
                at_hand_off,
                heard,
                album,
                _directory: directory,
            };
        }
    }
    panic!(
        "the hand-off was announced before the test could act between the probe \
         meeting the boundary and the sink rendering it, {WINDOW_ATTEMPTS} times"
    );
}

/// A seek back into the playing track after the probe met the boundary, but
/// before it was heard, keeps that track: its own gain, its own clock, and the
/// hand-off once playback reaches the boundary again.
#[test]
fn play_23a_a_seek_before_the_boundary_is_heard_keeps_the_playing_track() {
    const SEEK_TO_MS: i64 = 500;
    let window = staged_window();
    let (harness, heard) = (&window.harness, &window.heard);

    let segments = heard.segments();
    harness.player.seek_to(SEEK_TO_MS).unwrap();
    let mut events = harness.pump_until(HANG_GUARD, |_| heard.heard_after(segments + 1));
    events.extend(harness.pump_until(HANG_GUARD, |events| count(events, advanced) > 0));
    events.extend(harness.pump_for(Duration::from_millis(600)));

    assert_eq!(count(&events, advanced), 1, "exactly one hand-off");
    let rendered_ms = window.at_hand_off.load(Ordering::SeqCst);
    assert!(
        rendered_ms >= FIRST.1 - EARLY_TOLERANCE_MS,
        "the hand-off after the seek was announced with the sink at {rendered_ms} ms"
    );
    let handoff = events.iter().position(advanced).unwrap();
    let before = ticks(&events[..handoff]);
    assert!(
        before
            .iter()
            .all(|&(_, duration_ms)| duration_ms == FIRST.1 - FIRST.0),
        "after the seek the playing track's clock runs on: {before:?}"
    );
    let buffers = heard.buffers();
    assert!(
        buffers
            .first()
            .is_some_and(|first| first.start_ms < FIRST.0 + SEEK_TO_MS + EARLY_TOLERANCE_MS),
        "the seek must land in the playing track: {buffers:?}"
    );
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
        wrong_gain.is_empty(),
        "after the seek every buffer carries its own track's gain: {wrong_gain:?}"
    );
}

/// A different next track named while the hand-off is staged but unheard
/// withdraws it: the staged track is never announced, and the playing one
/// finishes — exactly once — so the frontend starts the newly named track.
#[test]
fn play_23a_a_different_next_track_named_before_the_boundary_is_heard_withdraws_the_hand_off() {
    const ELSEWHERE: (i64, i64) = (5_500, 6_500);
    let window = staged_window();
    let harness = &window.harness;

    harness
        .player
        .set_next(Some(cue_item(&window.album, ELSEWHERE, 0.0)));
    let mut events = harness.pump_until(HANG_GUARD, |events| count(events, finished) > 0);
    events.extend(harness.pump_for(Duration::from_millis(600)));

    assert_eq!(
        count(&events, advanced),
        0,
        "the withdrawn hand-off is never announced"
    );
    assert_eq!(
        count(&events, finished),
        1,
        "the playing track finishes once"
    );
    let ticks = ticks(&events);
    assert!(
        ticks
            .iter()
            .all(|&(_, duration_ms)| duration_ms == FIRST.1 - FIRST.0),
        "the playing track keeps its own clock to its end: {ticks:?}"
    );
}

/// A successor shorter than the sink's lead ends at the probe before the
/// frontend — which arms the track after it only once it takes over — can arm
/// anything. It still takes over, then finishes, once each and in that order,
/// so the frontend shows it and then starts the next track afresh — down to a
/// successor barely longer than one buffer.
#[test]
fn play_23a_a_successor_shorter_than_the_sink_lead_takes_over_then_finishes() {
    for short_ms in [400, 20] {
        let (harness, _) = harness_noting_the_render_at_hand_off();
        let directory = tempfile::tempdir().unwrap();
        let album = directory.path().join("album.wav");
        write_regions_wav(&album, &[(7_000, true)]);
        let short = (SECOND.0, SECOND.0 + short_ms);
        harness
            .player
            .play(cue_item(&album, FIRST, FIRST_GAIN_DB))
            .unwrap();
        harness
            .player
            .set_next(Some(cue_item(&album, short, SECOND_GAIN_DB)));

        let mut events = harness.pump_until(HANG_GUARD, |events| count(events, finished) > 0);
        events.extend(harness.pump_for(Duration::from_millis(600)));

        assert_eq!(
            (count(&events, advanced), count(&events, finished)),
            (1, 1),
            "a {short_ms} ms successor takes over and finishes once each: {events:?}"
        );
        let handoff = events.iter().position(advanced).unwrap();
        let finish = events.iter().position(finished).unwrap();
        assert!(
            handoff < finish,
            "a {short_ms} ms successor must take over before it finishes: {events:?}"
        );
    }
}
