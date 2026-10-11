//! FB-6 against a sink that never starts: every `Player::play` returns `Ok`
//! and the fault arrives later, as a `PlayerEvent::Error`. The skip run must
//! count those faults, so that it ends at the queue's length instead of
//! skipping a repeating queue for ever — which it did while every start reset
//! the count the moment `play` returned.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use reprise_core::playback::{
    AudioEffects, PlaybackBackend, PlaybackError, PlaybackFailure, PlaybackFailureKind,
    PlaybackItem, PlaybackSessionId, PlaybackState, PlayerEvent, StreamEvent, StreamGeneration,
};
use reprise_core::queue::Repeat;

use super::play_origin::PlayOrigin;
use super::player_controller::PlayerController;
use super::test_support::controller_with_events;

const TRACKS: [i64; 3] = [41, 42, 43];

#[derive(Default)]
struct Calls {
    played: RefCell<Vec<String>>,
    /// Bumped by every `play`, as the real player does.
    generation: Cell<u64>,
}

struct TestPlayback(Rc<Calls>);

impl PlaybackBackend for TestPlayback {
    fn play(&self, item: PlaybackItem<'_>) -> Result<(), PlaybackError> {
        self.0.played.borrow_mut().push(item.path.to_owned());
        self.0.generation.set(self.0.generation.get() + 1);
        Ok(())
    }

    fn play_uri(&self, _: &str) -> Result<(), PlaybackError> {
        Ok(())
    }

    fn toggle_pause(&self) -> Result<PlaybackState, PlaybackError> {
        Ok(PlaybackState::Paused)
    }

    fn seek_to(&self, _: i64) -> Result<(), PlaybackError> {
        Ok(())
    }

    fn set_volume(&self, _: f64) {}

    fn set_audio_effects(&self, _: AudioEffects) -> Result<(), PlaybackError> {
        Ok(())
    }

    fn stop(&self) -> Result<(), PlaybackError> {
        Ok(())
    }

    fn set_next(&self, _: Option<PlaybackItem<'_>>) {}

    fn set_transition(&self, _: reprise_core::library::settings::TrackTransition, _: u8) {}

    fn current_generation(&self) -> StreamGeneration {
        StreamGeneration::from(self.0.generation.get())
    }
}

struct Fixture {
    controller: Rc<PlayerController>,
    calls: Rc<Calls>,
    events: async_channel::Sender<StreamEvent>,
    _test_root: tempfile::TempDir,
    _main_context: std::sync::MutexGuard<'static, ()>,
}

/// A controller playing the first of three tracks on repeat-all. The files
/// exist, so a fault is "could not play" and never marks a track missing.
fn playing_repeat_all_queue() -> Fixture {
    let main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let db = Rc::new(crate::test_db::open().unwrap());
    for id in TRACKS {
        let path = test_root.path().join(format!("track-{id}.flac"));
        std::fs::write(&path, b"x").unwrap();
        crate::test_db::connection(&db)
            .execute(
                "INSERT INTO tracks (id, path, title, artist, album, duration_ms, added_at)
                 VALUES (?1, ?2, ?3, 'Artist', 'Album', 200000, 0)",
                rusqlite::params![id, path.to_str().unwrap(), format!("Track {id}")],
            )
            .unwrap();
    }
    let calls = Rc::new(Calls::default());
    let (controller, events) =
        controller_with_events(test_root.path(), db, Box::new(TestPlayback(calls.clone())));
    controller.queue.borrow_mut().set_repeat(Repeat::All);
    controller.play_from_view(TRACKS.to_vec(), 0, PlayOrigin::library());
    Fixture {
        controller,
        calls,
        events,
        _test_root: test_root,
        _main_context: main_context,
    }
}

fn stalled_start() -> PlayerEvent {
    PlayerEvent::Error(PlaybackFailure::new(
        "playback did not start",
        PlaybackFailureKind::Other,
        PlaybackSessionId::UNSCOPED,
    ))
}

fn tick(controller: &Rc<PlayerController>, position_ms: i64) {
    controller.apply_event(PlayerEvent::Position {
        position_ms,
        duration_ms: 200_000,
    });
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fb_6_asynchronous_faults_end_the_skip_run_at_the_queue_length() {
    let fixture = playing_repeat_all_queue();
    let controller = &fixture.controller;

    for _ in 0..TRACKS.len() * 4 {
        controller.apply_event(stalled_start());
    }

    assert_eq!(
        fixture.calls.played.borrow().len(),
        TRACKS.len(),
        "each track was tried once, then the run ended"
    );
    assert_eq!(controller.current_track.get(), None);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fb_6_the_skip_count_survives_the_starts_the_skip_loop_makes() {
    let fixture = playing_repeat_all_queue();
    let controller = &fixture.controller;

    controller.apply_event(stalled_start());
    controller.apply_event(stalled_start());

    assert_eq!(controller.consecutive_skips.get(), 2);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fb_6_a_moving_playhead_ends_the_skip_run() {
    let fixture = playing_repeat_all_queue();
    let controller = &fixture.controller;
    controller.apply_event(stalled_start());
    controller.apply_event(stalled_start());

    tick(controller, 0);
    assert_eq!(controller.consecutive_skips.get(), 2, "no movement yet");
    tick(controller, 1_500);

    assert_eq!(controller.consecutive_skips.get(), 0);
    assert_eq!(controller.failure_skip_limit.get(), 0);
    controller.apply_event(stalled_start());
    controller.apply_event(stalled_start());
    assert!(
        controller.current_track.get().is_some(),
        "a fresh run of two faults is still within the queue's length"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fb_6_starting_a_track_yourself_begins_a_fresh_run() {
    let fixture = playing_repeat_all_queue();
    let controller = &fixture.controller;
    controller.apply_event(stalled_start());
    controller.apply_event(stalled_start());

    controller.play_from_view(TRACKS.to_vec(), 0, PlayOrigin::library());

    assert_eq!(controller.consecutive_skips.get(), 0);
    assert_eq!(controller.failure_skip_limit.get(), 0);
}

/// Lets the controller's event drain run.
fn drain() {
    let context = gtk4::glib::MainContext::default();
    let until = std::time::Instant::now() + std::time::Duration::from_millis(300);
    while std::time::Instant::now() < until {
        context.iteration(false);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn tagged_fault(generation: u64) -> StreamEvent {
    StreamEvent {
        generation: StreamGeneration::from(generation),
        event: stalled_start(),
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fb_6_a_fault_that_predates_the_latest_start_skips_nothing() {
    let fixture = playing_repeat_all_queue();
    let current = fixture.calls.generation.get();

    // The give-up of a start the user has since replaced.
    fixture.events.try_send(tagged_fault(current - 1)).unwrap();
    drain();
    assert_eq!(
        fixture.calls.played.borrow().len(),
        1,
        "nothing was skipped"
    );

    // The same fault of the current start does skip it.
    fixture.events.try_send(tagged_fault(current)).unwrap();
    drain();
    assert_eq!(fixture.calls.played.borrow().len(), 2, "the drain works");
}

fn tagged_tick(generation: u64, position_ms: i64) -> StreamEvent {
    StreamEvent {
        generation: StreamGeneration::from(generation),
        event: PlayerEvent::Position {
            position_ms,
            duration_ms: 200_000,
        },
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fb_6_a_late_tick_of_the_previous_stream_does_not_end_the_skip_run() {
    let fixture = playing_repeat_all_queue();
    let controller = &fixture.controller;
    controller.apply_event(stalled_start());
    controller.apply_event(stalled_start());
    let current = fixture.calls.generation.get();

    // The previous stream's playhead, delivered after the starts that stalled.
    fixture
        .events
        .try_send(tagged_tick(current - 1, 90_000))
        .unwrap();
    drain();
    assert_eq!(controller.consecutive_skips.get(), 2, "a stuck start");

    fixture
        .events
        .try_send(tagged_tick(current, 1_000))
        .unwrap();
    drain();
    assert_eq!(controller.consecutive_skips.get(), 0, "the stream moves");
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fb_6_events_behind_a_track_finish_are_judged_by_the_stream_that_follows() {
    let fixture = playing_repeat_all_queue();
    let controller = &fixture.controller;
    let current = fixture.calls.generation.get();

    // One burst: the track ends, and its last tick was queued behind it.
    fixture
        .events
        .try_send(StreamEvent {
            generation: StreamGeneration::from(current),
            event: PlayerEvent::TrackFinished,
        })
        .unwrap();
    fixture
        .events
        .try_send(tagged_tick(current, 90_000))
        .unwrap();
    drain();

    assert_eq!(
        fixture.calls.played.borrow().len(),
        2,
        "the finish started the next track"
    );
    assert_eq!(
        controller.max_position_ms.get(),
        0,
        "the finished track's tick was credited to the track that followed"
    );
}
