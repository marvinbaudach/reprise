//! Ports that record instead of acting.
//!
//! These are what makes Task 3.1's promise checkable: a complete runtime,
//! driven end to end, with no display, no audio device and no media files.
//! They are not mocks with expectations — they are simple recorders, so a
//! test asserts on what the runtime *did*, not on a script it was supposed
//! to follow.
//!
//! Published behind the `fakes` feature so `reprise-mcp`'s headless tests
//! (Task 3.4) can drive a real runtime with them rather than growing a
//! second, subtly different set.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use reprise_core::device_sync::machine::Effect;
use reprise_core::library::settings::TrackTransition;
use reprise_core::playback::{AudioEffects, PlaybackBackend, PlaybackError, PlaybackState};

use crate::ports::{Clock, DeviceEffects, LibraryPort, PlayableTrack, TrackLocation};

/// One thing a fake port was asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendCall {
    Play(String),
    PlayUri(String),
    TogglePause,
    Stop,
    SeekTo(i64),
    SetVolume(u64),
}

/// A playback backend that remembers its calls and plays nothing.
pub struct FakePlayback {
    calls: Rc<RefCell<Vec<BackendCall>>>,
    state: Rc<RefCell<PlaybackState>>,
    /// When set, every `play`/`play_uri` fails with it — the way a missing
    /// codec or an unreadable file behaves, without needing either.
    refuse: Rc<RefCell<bool>>,
}

impl Default for FakePlayback {
    fn default() -> Self {
        Self::new()
    }
}

impl FakePlayback {
    #[must_use]
    pub fn new() -> Self {
        Self {
            calls: Rc::new(RefCell::new(Vec::new())),
            state: Rc::new(RefCell::new(PlaybackState::Stopped)),
            refuse: Rc::new(RefCell::new(false)),
        }
    }

    /// A second handle onto the same recorder, so a test can inspect the
    /// backend after the runtime has taken ownership of it.
    #[must_use]
    pub fn handle(&self) -> FakePlaybackHandle {
        FakePlaybackHandle {
            calls: Rc::clone(&self.calls),
            state: Rc::clone(&self.state),
            refuse: Rc::clone(&self.refuse),
        }
    }
}

/// The observer side of a [`FakePlayback`].
#[derive(Clone)]
pub struct FakePlaybackHandle {
    calls: Rc<RefCell<Vec<BackendCall>>>,
    state: Rc<RefCell<PlaybackState>>,
    refuse: Rc<RefCell<bool>>,
}

impl FakePlaybackHandle {
    #[must_use]
    pub fn calls(&self) -> Vec<BackendCall> {
        self.calls.borrow().clone()
    }

    pub fn clear(&self) {
        self.calls.borrow_mut().clear();
    }

    /// What the backend itself believes it is doing, as opposed to what the
    /// runtime believes. The two diverging is a real bug class.
    #[must_use]
    pub fn state(&self) -> PlaybackState {
        *self.state.borrow()
    }

    /// Makes every subsequent start fail.
    pub fn refuse_playback(&self, refuse: bool) {
        *self.refuse.borrow_mut() = refuse;
    }
}

impl PlaybackBackend for FakePlayback {
    fn play(&self, path: &str) -> Result<(), PlaybackError> {
        self.calls.borrow_mut().push(BackendCall::Play(path.into()));
        if *self.refuse.borrow() {
            return Err(PlaybackError::Backend("fake refuses to play".into()));
        }
        *self.state.borrow_mut() = PlaybackState::Playing;
        Ok(())
    }

    fn play_uri(&self, uri: &str) -> Result<(), PlaybackError> {
        self.calls
            .borrow_mut()
            .push(BackendCall::PlayUri(uri.into()));
        if *self.refuse.borrow() {
            return Err(PlaybackError::Backend("fake refuses to play".into()));
        }
        *self.state.borrow_mut() = PlaybackState::Playing;
        Ok(())
    }

    fn toggle_pause(&self) -> Result<PlaybackState, PlaybackError> {
        self.calls.borrow_mut().push(BackendCall::TogglePause);
        let mut state = self.state.borrow_mut();
        *state = match *state {
            PlaybackState::Playing => PlaybackState::Paused,
            _ => PlaybackState::Playing,
        };
        Ok(*state)
    }

    fn seek_to(&self, position_ms: i64) -> Result<(), PlaybackError> {
        self.calls
            .borrow_mut()
            .push(BackendCall::SeekTo(position_ms));
        Ok(())
    }

    fn set_volume(&self, volume: f64) {
        // Recorded in permille: a float in an equality assertion is a trap,
        // and a volume is a display value, not a measurement.
        self.calls
            .borrow_mut()
            .push(BackendCall::SetVolume((volume * 1000.0).round() as u64));
    }

    fn set_audio_effects(&self, _effects: AudioEffects) -> Result<(), PlaybackError> {
        Ok(())
    }

    fn stop(&self) -> Result<(), PlaybackError> {
        self.calls.borrow_mut().push(BackendCall::Stop);
        *self.state.borrow_mut() = PlaybackState::Stopped;
        Ok(())
    }

    fn set_next(&self, _path: Option<&str>) {}

    fn set_transition(&self, _mode: TrackTransition, _crossfade_seconds: u8) {}
}

/// A library that resolves exactly the tracks it was given.
#[derive(Default)]
pub struct FakeLibrary {
    tracks: BTreeMap<i64, PlayableTrack>,
}

impl FakeLibrary {
    /// A library holding `ids`, each with a local path and derived display
    /// strings. The path is never opened, which is the point.
    #[must_use]
    pub fn with_tracks(ids: impl IntoIterator<Item = i64>) -> Self {
        let tracks = ids
            .into_iter()
            .map(|id| {
                (
                    id,
                    PlayableTrack {
                        track_id: id,
                        location: TrackLocation::Path(format!("/music/{id}.flac")),
                        title: format!("Track {id}"),
                        artist: "Artist".into(),
                        album: "Album".into(),
                        duration_ms: 180_000,
                    },
                )
            })
            .collect();
        Self { tracks }
    }

    /// Adds or replaces one track, for the cases that need a stream URI or a
    /// specific duration.
    #[must_use]
    pub fn with(mut self, track: PlayableTrack) -> Self {
        self.tracks.insert(track.track_id, track);
        self
    }
}

impl LibraryPort for FakeLibrary {
    fn resolve(&self, track_id: i64) -> Option<PlayableTrack> {
        self.tracks.get(&track_id).cloned()
    }
}

/// A device-effect port that records requests and performs none of them, so
/// a test decides exactly when and how each one is answered.
#[derive(Default)]
pub struct FakeDevices {
    planned: Rc<RefCell<Vec<String>>>,
    performed: Rc<RefCell<Vec<(String, Effect)>>>,
}

impl FakeDevices {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn handle(&self) -> FakeDevicesHandle {
        FakeDevicesHandle {
            planned: Rc::clone(&self.planned),
            performed: Rc::clone(&self.performed),
        }
    }
}

/// The observer side of a [`FakeDevices`].
#[derive(Clone)]
pub struct FakeDevicesHandle {
    planned: Rc<RefCell<Vec<String>>>,
    performed: Rc<RefCell<Vec<(String, Effect)>>>,
}

impl FakeDevicesHandle {
    #[must_use]
    pub fn planned(&self) -> Vec<String> {
        self.planned.borrow().clone()
    }

    #[must_use]
    pub fn performed(&self) -> Vec<(String, Effect)> {
        self.performed.borrow().clone()
    }

    /// The effects requested since the last call, and clears them.
    #[must_use]
    pub fn take_performed(&self) -> Vec<(String, Effect)> {
        std::mem::take(&mut self.performed.borrow_mut())
    }
}

impl DeviceEffects for FakeDevices {
    fn plan(&self, device: &str) {
        self.planned.borrow_mut().push(device.to_owned());
    }

    fn perform(&self, device: &str, effect: Effect) {
        self.performed
            .borrow_mut()
            .push((device.to_owned(), effect));
    }
}

/// A clock a test moves by hand.
#[derive(Default)]
pub struct FakeClock {
    unix: Rc<RefCell<i64>>,
    monotonic_ms: Rc<RefCell<u64>>,
}

impl FakeClock {
    /// A clock starting at `unix` seconds and at monotonic zero.
    #[must_use]
    pub fn starting_at(unix: i64) -> Self {
        Self {
            unix: Rc::new(RefCell::new(unix)),
            monotonic_ms: Rc::new(RefCell::new(0)),
        }
    }

    #[must_use]
    pub fn handle(&self) -> FakeClockHandle {
        FakeClockHandle {
            unix: Rc::clone(&self.unix),
            monotonic_ms: Rc::clone(&self.monotonic_ms),
        }
    }
}

/// The control side of a [`FakeClock`].
#[derive(Clone)]
pub struct FakeClockHandle {
    unix: Rc<RefCell<i64>>,
    monotonic_ms: Rc<RefCell<u64>>,
}

impl FakeClockHandle {
    /// Moves both clocks forward by `ms`.
    pub fn advance_ms(&self, ms: u64) {
        *self.monotonic_ms.borrow_mut() += ms;
        *self.unix.borrow_mut() += (ms / 1_000) as i64;
    }
}

impl Clock for FakeClock {
    fn now_unix(&self) -> i64 {
        *self.unix.borrow()
    }

    fn now_monotonic_ms(&self) -> u64 {
        *self.monotonic_ms.borrow()
    }
}
