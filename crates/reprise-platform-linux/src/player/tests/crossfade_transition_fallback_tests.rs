//! PLAY-25 (proposed): a crossfade never promotes an incoming track that
//! has not started. The secondary is made to hang, or to start late, by a
//! probe on its own gain element; the primary plays on a `fakesink`.

use super::crossfade_transition_tests::{
    crossfade_over_when_hung, trouble_on_secondary, Trouble, HANG_GUARD,
};
use super::segment_support::{count, ticks, write_regions_wav, Harness};
use super::*;
use reprise_core::library::settings::TrackTransition;

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
fn play_25_a_crossfade_whose_incoming_track_hangs_falls_back_to_a_plain_track_change() {
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

/// Guards against an over-eager fallback: a secondary that starts late but
/// inside the bound still fades and is promoted, as before.
#[test]
fn play_25_a_crossfade_whose_incoming_track_prerolls_slowly_still_fades() {
    let harness = Harness::new();
    let player = &harness.player;
    let directory = tempfile::tempdir().unwrap();
    let first = tone(&directory, "first.wav", 7_000);
    let second = tone(&directory, "second.wav", 4_000);
    player.set_transition(TrackTransition::Crossfade, 4);
    let trouble = trouble_on_secondary(Trouble::Late(SLOW_FIRST_BUFFER));

    let (events, _) = crossfade_over_when_hung(
        &harness,
        &trouble,
        || {
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
}
