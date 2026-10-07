//! The frontend side of PLAY-22: a CUE track's time is its own everywhere it
//! is read. The backend reports position and duration relative to the track
//! (see `reprise-platform-linux`'s `player/segment.rs`); these tests drive
//! the controller with such values on a fake backend and prove that play
//! counting, scrobble eligibility, Previous, the sleep timer and the lyrics
//! lookup all measure the track, not the file it is cut from.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use reprise_core::playback::{
    AudioEffects, PlaybackBackend, PlaybackError, PlaybackItem, PlaybackState, PlayerEvent,
};
use reprise_core::scrobbling::{self, ScrobbleProvider};
use reprise_view::sleep_timer::SleepTimer;

use super::play_origin::PlayOrigin;
use super::player_controller::PlayerController;
use super::sleep_timer_hooks::SleepTimerBinding;
use super::test_support::controller_with_db;
use crate::ui::lyrics::player_lyrics::lyrics_query_for;

const ALBUM_PATH: &str = "/music/live-album.flac";
/// Track 3 of the album: ten to thirteen minutes and twenty seconds into the
/// file, so every file-absolute value is far outside the track's own range.
const TRACK: i64 = 7;
const TRACK_START_MS: i64 = 600_000;
const TRACK_LENGTH_MS: i64 = 200_000;
const NEXT_TRACK: i64 = 8;

/// A `play` call as the backend received it: the file and the stretch of it.
type PlayedItem = (String, Option<(i64, i64)>);

#[derive(Default)]
struct Calls {
    played: RefCell<Vec<PlayedItem>>,
    sought: RefCell<Vec<i64>>,
    pauses: Cell<u32>,
    volumes: RefCell<Vec<f64>>,
}

struct TestPlayback(Rc<Calls>);

impl PlaybackBackend for TestPlayback {
    fn play(&self, item: PlaybackItem<'_>) -> Result<(), PlaybackError> {
        self.0
            .played
            .borrow_mut()
            .push((item.path.to_owned(), item.segment));
        Ok(())
    }

    fn play_uri(&self, _: &str) -> Result<(), PlaybackError> {
        Ok(())
    }

    fn toggle_pause(&self) -> Result<PlaybackState, PlaybackError> {
        self.0.pauses.set(self.0.pauses.get() + 1);
        Ok(PlaybackState::Paused)
    }

    fn seek_to(&self, position_ms: i64) -> Result<(), PlaybackError> {
        self.0.sought.borrow_mut().push(position_ms);
        Ok(())
    }

    fn set_volume(&self, volume: f64) {
        self.0.volumes.borrow_mut().push(volume);
    }

    fn set_audio_effects(&self, _: AudioEffects) -> Result<(), PlaybackError> {
        Ok(())
    }

    fn stop(&self) -> Result<(), PlaybackError> {
        Ok(())
    }

    fn set_next(&self, _: Option<PlaybackItem<'_>>) {}

    fn set_transition(&self, _: reprise_core::library::settings::TrackTransition, _: u8) {}
}

/// Two consecutive CUE tracks of one file, each `length_ms` long, the first
/// starting at `TRACK_START_MS`.
fn insert_cue_tracks(db: &reprise_core::db::Db, length_ms: i64) {
    for (index, id) in [TRACK, NEXT_TRACK].into_iter().enumerate() {
        let start_ms = TRACK_START_MS + index as i64 * length_ms;
        crate::test_db::connection(db)
            .execute(
                "INSERT INTO tracks (id, path, title, artist, album, duration_ms, added_at,
                                     segment_index, segment_start_ms, segment_end_ms, cue_path)
                 VALUES (?1, ?2, ?3, 'Artist', 'Live', ?4, 0, ?5, ?6, ?7, '/music/live-album.cue')",
                rusqlite::params![
                    id,
                    ALBUM_PATH,
                    format!("Track {id}"),
                    length_ms,
                    index as i64 + 3,
                    start_ms,
                    start_ms + length_ms,
                ],
            )
            .unwrap();
    }
}

struct Fixture {
    controller: Rc<PlayerController>,
    calls: Rc<Calls>,
    _test_root: tempfile::TempDir,
    _main_context: std::sync::MutexGuard<'static, ()>,
}

/// A controller playing the first of two CUE tracks of `length_ms` each.
fn playing_cue_track(length_ms: i64) -> Fixture {
    let main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let db = Rc::new(crate::test_db::open().unwrap());
    insert_cue_tracks(&db, length_ms);
    let calls = Rc::new(Calls::default());
    let controller =
        controller_with_db(test_root.path(), db, Box::new(TestPlayback(calls.clone())));
    controller.play_from_view(vec![TRACK, NEXT_TRACK], 0, PlayOrigin::library());
    Fixture {
        controller,
        calls,
        _test_root: test_root,
        _main_context: main_context,
    }
}

fn tick(controller: &Rc<PlayerController>, position_ms: i64, duration_ms: i64) {
    controller.apply_event(PlayerEvent::Position {
        position_ms,
        duration_ms,
    });
}

fn play_count(controller: &PlayerController, id: i64) -> i64 {
    crate::test_db::connection(&controller.conn)
        .query_row("SELECT play_count FROM tracks WHERE id = ?1", [id], |row| {
            row.get(0)
        })
        .unwrap()
}

fn listen_events(controller: &PlayerController, id: i64) -> i64 {
    crate::test_db::connection(&controller.conn)
        .query_row(
            "SELECT COUNT(*) FROM listen_events WHERE track_id = ?1",
            [id],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn play_22_the_frontend_starts_a_cue_track_with_its_own_stretch() {
    let fixture = playing_cue_track(TRACK_LENGTH_MS);

    assert_eq!(
        fixture.calls.played.borrow().as_slice(),
        [(
            ALBUM_PATH.to_owned(),
            Some((TRACK_START_MS, TRACK_START_MS + TRACK_LENGTH_MS))
        )]
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn play_22_a_cue_track_counts_a_play_by_its_own_length() {
    let fixture = playing_cue_track(TRACK_LENGTH_MS);
    let controller = &fixture.controller;

    tick(controller, TRACK_LENGTH_MS / 2 + 10_000, TRACK_LENGTH_MS);
    controller.apply_event(PlayerEvent::TrackFinished);
    tick(controller, TRACK_LENGTH_MS / 2 - 10_000, TRACK_LENGTH_MS);
    controller.apply_event(PlayerEvent::TrackFinished);

    assert_eq!(
        play_count(controller, TRACK),
        1,
        "past half of its own length"
    );
    assert_eq!(listen_events(controller, TRACK), 1);
    assert_eq!(
        play_count(controller, NEXT_TRACK),
        0,
        "short of half of its own length"
    );
    assert_eq!(listen_events(controller, NEXT_TRACK), 0);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn play_22_a_cue_track_is_scrobble_eligible_by_its_own_length() {
    let fixture = playing_cue_track(TRACK_LENGTH_MS);
    let controller = &fixture.controller;
    let summary = reprise_core::queries::query_track_summary(&controller.conn, TRACK)
        .unwrap()
        .unwrap();
    assert_eq!(
        summary.duration_ms, TRACK_LENGTH_MS,
        "a scrobble is judged against the length the session begins with"
    );

    tick(controller, TRACK_LENGTH_MS / 2 - 10_000, TRACK_LENGTH_MS);
    let short = controller.max_position_ms.get();
    tick(controller, TRACK_LENGTH_MS / 2 + 10_000, TRACK_LENGTH_MS);
    let enough = controller.max_position_ms.get();

    for provider in [ScrobbleProvider::ListenBrainz, ScrobbleProvider::LastFm] {
        assert!(!scrobbling::should_scrobble_for(
            provider,
            short,
            summary.duration_ms
        ));
        assert!(scrobbling::should_scrobble_for(
            provider,
            enough,
            summary.duration_ms
        ));
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn play_22_previous_restarts_a_cue_track_at_its_own_start() {
    let fixture = playing_cue_track(TRACK_LENGTH_MS);
    let controller = &fixture.controller;

    tick(controller, 10_000, TRACK_LENGTH_MS);
    controller.previous();

    assert_eq!(
        fixture.calls.sought.borrow().as_slice(),
        [0],
        "the backend maps the track's own zero to the track's start in the file"
    );
    assert_eq!(fixture.calls.played.borrow().len(), 1, "nothing restarted");
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn play_22_the_sleep_timer_fades_out_at_a_cue_track_s_own_end() {
    const SHORT_TRACK_MS: i64 = 60_000;
    let fixture = playing_cue_track(SHORT_TRACK_MS);
    let controller = &fixture.controller;
    let binding = SleepTimerBinding::new(Rc::new(RefCell::new(SleepTimer::off())));
    controller.install_sleep_timer(&binding);
    assert!(controller.arm_sleep_timer_end_of_track(&binding));

    tick(controller, SHORT_TRACK_MS / 2, SHORT_TRACK_MS);
    assert!(
        fixture
            .calls
            .volumes
            .borrow()
            .iter()
            .all(|volume| *volume >= 1.0),
        "no fade halfway through the track"
    );
    tick(controller, SHORT_TRACK_MS - 2_000, SHORT_TRACK_MS);
    assert!(
        fixture
            .calls
            .volumes
            .borrow()
            .iter()
            .any(|volume| *volume < 1.0),
        "the fade starts near the track's own end"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn play_22_the_sleep_timer_pauses_after_a_cue_track_that_plays_through() {
    let fixture = playing_cue_track(TRACK_LENGTH_MS);
    let controller = &fixture.controller;
    let binding = SleepTimerBinding::new(Rc::new(RefCell::new(SleepTimer::off())));
    controller.install_sleep_timer(&binding);
    assert!(controller.arm_sleep_timer_end_of_track(&binding));

    controller.apply_event(PlayerEvent::AdvancedToNext);

    assert_eq!(
        controller.current_track.get().map(|(id, _)| id),
        Some(NEXT_TRACK)
    );
    assert_eq!(fixture.calls.pauses.get(), 1);
    assert_eq!(fixture.calls.played.borrow().len(), 1, "nothing restarted");
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn play_22_lyrics_are_looked_up_by_a_cue_track_s_own_length() {
    let fixture = playing_cue_track(TRACK_LENGTH_MS);
    let summary = reprise_core::queries::query_track_summary(&fixture.controller.conn, TRACK)
        .unwrap()
        .unwrap();

    let lyrics = lyrics_query_for(&summary);

    assert_eq!(lyrics.query.duration_ms, TRACK_LENGTH_MS);
    assert!(
        lyrics.track_path.is_none(),
        "the file's sidecar belongs to every track in it"
    );
}
