//! PLAY-26 (proposed): a crossfade never promotes an incoming track that
//! has not started. The secondary is made to hang, or to start late, by a
//! probe on its own gain element; the primary plays on a `fakesink`.

use super::crossfade_transition_tests::{
    crossfade_over_when_hung, engine_of, trouble_on_secondary, Trouble, HANG_GUARD,
};
use super::segment_support::{count, ticks, write_regions_wav, Harness};
use super::*;
use reprise_core::library::settings::TrackTransition;
use std::time::Instant;

/// How long the slow secondary holds its first buffer.
const SLOW_FIRST_BUFFER: Duration = Duration::from_secs(1);

fn finished(event: &PlayerEvent) -> bool {
    matches!(event, PlayerEvent::TrackFinished)
}

fn advanced(event: &PlayerEvent) -> bool {
    matches!(event, PlayerEvent::AdvancedToNext)
}

fn failed(event: &PlayerEvent) -> bool {
    matches!(event, PlayerEvent::Error(_))
}

fn playing(event: &PlayerEvent) -> bool {
    matches!(event, PlayerEvent::StateChanged(PlaybackState::Playing))
}

fn tone(directory: &tempfile::TempDir, name: &str, ms: u32) -> String {
    let path = directory.path().join(name);
    write_regions_wav(&path, &[(ms, true)]);
    path.to_str().unwrap().to_owned()
}

fn primary_volume(player: &Player) -> f64 {
    player
        .playbin
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .property::<f64>("volume")
}

fn current_uri(player: &Player) -> Option<String> {
    player
        .playbin
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .property::<Option<String>>("current-uri")
}

#[test]
fn play_26_a_crossfade_whose_incoming_track_hangs_falls_back_to_a_plain_track_change() {
    let harness = Harness::new();
    let player = &harness.player;
    let directory = tempfile::tempdir().unwrap();
    let first = tone(&directory, "first.wav", 4_000);
    let second = tone(&directory, "second.wav", 3_000);
    player.set_transition(TrackTransition::Crossfade, 2);
    let trouble = trouble_on_secondary(Trouble::Hang);
    let user_volume = *player
        .user_volume
        .lock()
        .unwrap_or_else(PoisonError::into_inner);

    harness.start(|| {
        player.play(item(&first)).unwrap();
        player.set_next(Some(item(&second)));
    });
    let lowest_volume = std::cell::Cell::new(f64::MAX);
    let saw_crossfading = std::cell::Cell::new(false);
    let mut events = harness.pump_until(HANG_GUARD, |events| {
        lowest_volume.set(lowest_volume.get().min(primary_volume(player)));
        if player.crossfading.load(Ordering::SeqCst) {
            saw_crossfading.set(true);
        }
        count(events, finished) + count(events, advanced) > 0
    });

    assert_eq!(
        count(&events, advanced),
        0,
        "a secondary that never started was promoted"
    );
    assert!(saw_crossfading.get(), "the crossfade never started");
    assert_eq!(trouble.installed(), 1, "the secondary was not hooked");
    assert!(
        lowest_volume.get() >= user_volume - 1e-9,
        "the outgoing track faded out over a silent secondary: {}",
        lowest_volume.get()
    );
    assert_eq!(
        count(&events, finished),
        1,
        "the outgoing track must play to its end"
    );
    assert!(current_uri(player).is_some_and(|uri| uri.ends_with("first.wav")));
    assert!(!player.crossfading.load(Ordering::SeqCst));
    assert!(player
        .incoming
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .is_none());

    // The ordinary next-track start, as the frontend makes it on TrackFinished.
    harness.start(|| {
        player.play(item(&second)).unwrap();
    });
    events.extend(harness.pump_until(HANG_GUARD, |events| {
        count(events, playing) > 0 && ticks(events).iter().any(|&(position, _)| position > 0)
    }));
    assert!(
        count(&events, playing) > 0,
        "the queued track never started"
    );
    assert!(current_uri(player).is_some_and(|uri| uri.ends_with("second.wav")));
    assert_eq!(count(&events, failed), 0, "events: {events:?}");
}

/// When the fade started and when it ended, as the player reported them.
#[derive(Default)]
struct FadeTimeline {
    /// When the outgoing track will end, worked out from the first position
    /// tick inside the fade window.
    outgoing_ends_at: Option<Instant>,
    promoted_at: Option<Instant>,
}

/// The fade of the slow-preroll test, longer than the first buffer's delay.
const SLOW_FADE_SECONDS: u8 = 4;

/// How much later than the outgoing track's end the promotion may come: one
/// ramp step, and the position ticker's granularity.
const PROMOTION_SLACK: Duration = Duration::from_millis(400);

/// Guards against an over-eager fallback: a secondary that starts late but
/// inside the bound still fades and is promoted, as before. The fade it gets
/// is the time that is left: the ramp starts after the wait, and it ends when
/// the outgoing track does, not `SLOW_FADE_SECONDS` after it started.
#[test]
fn play_26_a_crossfade_whose_incoming_track_prerolls_slowly_still_fades() {
    let timeline = Arc::new(Mutex::new(FadeTimeline::default()));
    let observed = timeline.clone();
    let harness = Harness::observing(move |event| {
        let mut timeline = observed.lock().unwrap_or_else(PoisonError::into_inner);
        match event {
            PlayerEvent::Position {
                position_ms,
                duration_ms,
            } if timeline.outgoing_ends_at.is_none()
                && *position_ms >= duration_ms - i64::from(SLOW_FADE_SECONDS) * 1_000 =>
            {
                timeline.outgoing_ends_at = Some(
                    Instant::now() + Duration::from_millis((duration_ms - position_ms) as u64),
                );
            }
            PlayerEvent::AdvancedToNext if timeline.promoted_at.is_none() => {
                timeline.promoted_at = Some(Instant::now());
            }
            _ => {}
        }
    });
    let player = &harness.player;
    let directory = tempfile::tempdir().unwrap();
    let first = tone(&directory, "first.wav", 7_000);
    let second = tone(&directory, "second.wav", 4_000);
    player.set_transition(TrackTransition::Crossfade, SLOW_FADE_SECONDS);
    let trouble = trouble_on_secondary(Trouble::Late(SLOW_FIRST_BUFFER));

    let (events, _) = crossfade_over_when_hung(
        &harness,
        &trouble,
        || {
            *timeline.lock().unwrap_or_else(PoisonError::into_inner) = FadeTimeline::default();
            player.play(item(&first)).unwrap();
            player.set_next(Some(item(&second)));
        },
        |events| count(events, advanced) > 0,
    );

    assert_eq!(trouble.reached(), 1, "the secondary was not delayed");
    assert_eq!(count(&events, advanced), 1, "the slow secondary must fade");
    assert_eq!(count(&events, finished), 0);
    assert_eq!(count(&events, failed), 0, "events: {events:?}");
    assert!(current_uri(player).is_some_and(|uri| uri.ends_with("second.wav")));
    assert!(!player.crossfading.load(Ordering::SeqCst));
    let timeline = timeline.lock().unwrap_or_else(PoisonError::into_inner);
    let (Some(ends_at), Some(promoted_at)) = (timeline.outgoing_ends_at, timeline.promoted_at)
    else {
        panic!("the fade was not timed");
    };
    assert!(
        promoted_at <= ends_at + PROMOTION_SLACK,
        "the ramp outlasted the outgoing track by {:?}: it was cut off mid-fade",
        promoted_at.saturating_duration_since(ends_at)
    );
}

/// A one-second fade whose incoming track holds its first buffer this long
/// prerolls with too little of the outgoing track left to fade over.
const LATE_FOR_A_ONE_SECOND_FADE: Duration = Duration::from_millis(600);

/// A secondary that prerolls just inside the bound can leave a ramp of a few
/// milliseconds: a full-to-silent step instead of a fade. The fade must give
/// way to a plain change instead. The ticker's part is played by hand, so that
/// the outgoing track really has seconds left and the test needs no late tick.
#[test]
fn play_26_a_crossfade_with_too_little_time_left_to_ramp_gives_way_instead_of_cutting() {
    let harness = Harness::new();
    let player = &harness.player;
    let directory = tempfile::tempdir().unwrap();
    let first = tone(&directory, "first.wav", 4_000);
    let second = tone(&directory, "second.wav", 3_000);
    player.set_transition(TrackTransition::Crossfade, 1);
    let trouble = trouble_on_secondary(Trouble::Late(LATE_FOR_A_ONE_SECOND_FADE));
    let user_volume = *player
        .user_volume
        .lock()
        .unwrap_or_else(PoisonError::into_inner);

    harness.start(|| {
        player.play(item(&first)).unwrap();
    });
    player.set_next(Some(item(&second)));
    engine_of(player).maybe_start(0, 1_000);
    let lowest_volume = std::cell::Cell::new(f64::MAX);
    let events = harness.pump_until(HANG_GUARD, |events| {
        lowest_volume.set(lowest_volume.get().min(primary_volume(player)));
        !player.crossfading.load(Ordering::SeqCst) || count(events, advanced) > 0
    });

    assert_eq!(trouble.installed(), 1, "the secondary was not hooked");
    assert_eq!(count(&events, advanced), 0, "a ramp of a few ms was run");
    assert!(
        lowest_volume.get() >= user_volume - 1e-9,
        "the outgoing track was cut: {}",
        lowest_volume.get()
    );
    assert_eq!(count(&events, failed), 0, "events: {events:?}");
}

/// The volume the user drags to while the crossfade waits for its incoming
/// track.
const VOLUME_DURING_WAIT: f64 = 0.4;

/// How long the volume may trail the guard before the test calls it lost. The
/// restore is a single property write after the guard goes down.
const VOLUME_SETTLE: Duration = Duration::from_secs(5);

/// `set_volume` only stores the target while a fade runs, because the ramp
/// applies it. A fade that gave way never ran the ramp, so giving way must
/// apply the stored volume to the outgoing track itself.
#[test]
fn play_26_a_crossfade_that_gives_way_applies_the_volume_set_while_it_waited() {
    let harness = Harness::new();
    let player = &harness.player;
    let directory = tempfile::tempdir().unwrap();
    let first = tone(&directory, "first.wav", 4_000);
    let second = tone(&directory, "second.wav", 3_000);
    player.set_transition(TrackTransition::Crossfade, 2);
    let _trouble = trouble_on_secondary(Trouble::Hang);

    harness.start(|| {
        player.play(item(&first)).unwrap();
        player.set_next(Some(item(&second)));
    });
    let saw_crossfading = std::cell::Cell::new(false);
    let changed = std::cell::Cell::new(false);
    let events = harness.pump_until(HANG_GUARD, |events| {
        if player.crossfading.load(Ordering::SeqCst) {
            saw_crossfading.set(true);
            if !changed.replace(true) {
                player.set_volume(VOLUME_DURING_WAIT);
            }
        }
        saw_crossfading.get() && !player.crossfading.load(Ordering::SeqCst)
            || count(events, finished) > 0
    });

    assert!(
        changed.get(),
        "the volume was never changed during the wait"
    );
    assert_eq!(count(&events, advanced), 0);
    // `give_way` clears the guard before it restores the volume, and the pump
    // stops at the guard: the restore may be a moment behind.
    let deadline = Instant::now() + VOLUME_SETTLE;
    while (primary_volume(player) - VOLUME_DURING_WAIT).abs() >= 1e-6 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        (primary_volume(player) - VOLUME_DURING_WAIT).abs() < 1e-6,
        "the outgoing track kept the old volume: {}",
        primary_volume(player)
    );
}

/// The real length of the track that ends while its fade still waits.
const SHORT_TRACK_MS: u32 = 2_000;

/// What the position ticker is told about the track: more than it really has,
/// as a VBR file without a length header or a stale duration does.
const OVERSTATED_DURATION_MS: i64 = 3_500;

/// One round of the scenario below: the outgoing track ends during the wait,
/// the incoming one hangs, and the fade gives way. Returns the events up to a
/// second after the report.
fn outgoing_track_ends_during_the_wait(
    harness: &Harness,
    first: &str,
    second: &str,
) -> Vec<PlayerEvent> {
    let player = &harness.player;
    harness.start(|| {
        player.play(item(first)).unwrap();
    });
    // The ticker's part, played by hand so that its numbers can be wrong: the
    // real ticker would report the true duration and give up in time.
    player.set_next(Some(item(second)));
    player.set_transition(TrackTransition::Crossfade, 4);
    engine_of(player).maybe_start(0, OVERSTATED_DURATION_MS);

    let mut events = harness.pump_until(Duration::from_secs(10), |events| {
        count(events, finished) > 0
    });
    // A second report would be as wrong as none.
    events.extend(harness.pump_for(Duration::from_secs(1)));
    events
}

/// The bus watch of the outgoing pipeline drops its end-of-stream while a fade
/// runs. A duration that is longer than the track lets that end come before
/// the fade gives way, and the track change that follows must not lose it.
#[test]
fn play_26_a_crossfade_that_gives_way_after_the_outgoing_track_ended_still_finishes_it() {
    let harness = Harness::new();
    let directory = tempfile::tempdir().unwrap();
    let first = tone(&directory, "first.wav", SHORT_TRACK_MS);
    let second = tone(&directory, "second.wav", 3_000);
    let trouble = trouble_on_secondary(Trouble::Hang);

    let events = outgoing_track_ends_during_the_wait(&harness, &first, &second);

    assert_eq!(trouble.installed(), 1, "the secondary was not hooked");
    assert_eq!(count(&events, advanced), 0);
    assert_eq!(
        count(&events, finished),
        1,
        "the outgoing track ended during the wait and was never reported: {events:?}"
    );
    assert_eq!(count(&events, failed), 0, "events: {events:?}");
    assert!(!harness.player.crossfading.load(Ordering::SeqCst));
}

/// A manual `play()` bumps the generation. If it lands after a fade that gives
/// way has checked the generation, the fade must not report the end of the track
/// that `play()` just replaced. The hook is that `play()`, at the one moment
/// where it can no longer be seen by the check.
#[test]
fn play_26_a_crossfade_that_gives_way_does_not_report_a_track_a_play_replaced() {
    let harness = Harness::new();
    let player = &harness.player;
    let directory = tempfile::tempdir().unwrap();
    let first = tone(&directory, "first.wav", SHORT_TRACK_MS);
    let second = tone(&directory, "second.wav", 3_000);
    let trouble = trouble_on_secondary(Trouble::Hang);
    let generation = engine_of(player).generation;
    let _play_lands = crate::crossfade::test_seam::on_before_release(Arc::new(move || {
        generation.fetch_add(1, Ordering::SeqCst);
    }));

    harness.start(|| {
        player.play(item(&first)).unwrap();
    });
    player.set_next(Some(item(&second)));
    player.set_transition(TrackTransition::Crossfade, 4);
    engine_of(player).maybe_start(0, OVERSTATED_DURATION_MS);
    let mut events = harness.pump_until(Duration::from_secs(10), |events| {
        count(events, finished) > 0 || !player.crossfading.load(Ordering::SeqCst)
    });
    events.extend(harness.pump_for(Duration::from_secs(1)));

    assert_eq!(trouble.installed(), 1, "the secondary was not hooked");
    assert!(!player.crossfading.load(Ordering::SeqCst));
    assert_eq!(
        count(&events, finished),
        0,
        "the fade reported the end of a track that a play had replaced: {events:?}"
    );
}

/// The catch on the outgoing pipeline's bus belongs to one fade. The next fade
/// on the same pipeline (the track change after a fade that gave way plays on
/// it) must catch its own.
#[test]
fn play_26_a_second_crossfade_on_the_same_pipeline_also_reports_the_end_it_missed() {
    let harness = Harness::new();
    let directory = tempfile::tempdir().unwrap();
    let first = tone(&directory, "first.wav", SHORT_TRACK_MS);
    let second = tone(&directory, "second.wav", 3_000);
    let trouble = trouble_on_secondary(Trouble::Hang);

    let mut pipelines = Vec::new();
    for round in 1..=2 {
        trouble.reset();
        let events = outgoing_track_ends_during_the_wait(&harness, &first, &second);
        assert_eq!(
            trouble.installed(),
            1,
            "round {round}: the secondary was not hooked"
        );
        assert_eq!(count(&events, advanced), 0, "round {round}");
        assert_eq!(
            count(&events, finished),
            1,
            "round {round}: the outgoing track ended during the wait and was not reported once: {events:?}"
        );
        pipelines.push(
            harness
                .player
                .playbin
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone(),
        );
    }
    assert_eq!(
        pipelines[0], pipelines[1],
        "the rounds must run on one pipeline, or the second proves nothing"
    );
}
