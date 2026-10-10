//! Starts a local file over when `playbin3` never leaves READY.
//!
//! GStreamer 1.28's `playbin3` sometimes hangs on a start: the stream arrives
//! while `decodebin3` is still being brought up, the state change then never
//! completes and the pipeline sits in READY with a pending PAUSED or PLAYING
//! for good. It happens once in a few hundred starts on a busy machine (a scan,
//! a build, stem separation next to the player) and never at idle, and nothing
//! the player could wait for ends it. The way out is the one a test uses: begin
//! the same start again from `Null`.
//!
//! [`StartWatchdog::arm`] is called once a local-file start has been handed to
//! the pipeline. [`START_DEADLINE`] later it looks at the pipeline: a start that
//! is still READY with a state pending is begun again, up to [`START_ATTEMPTS`]
//! starts in all, and the last hung one is reported as a
//! [`PlayerEvent::Error`], which the frontend answers by skipping the track.
//!
//! CUE segment starts and the crossfade secondary start do not arm it.

use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use reprise_core::playback::{PlaybackFailure, PlaybackSessionId, PlayerEvent};

use crate::player_pipeline::is_remote_playback_uri;

/// How long a start may stay in READY before it counts as hung.
pub(super) const START_DEADLINE: Duration = Duration::from_secs(4);

/// How many times one track is started, the first start included.
pub(super) const START_ATTEMPTS: u32 = 3;

/// What a start looked like at its deadline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct StartObservation {
    pub(super) current: gst::State,
    pub(super) pending: gst::State,
    /// `false` once something else started: a new `play`, a gapless step or a
    /// crossfade promotion all bump the stream generation.
    pub(super) same_stream: bool,
}

/// What to do about a start at its deadline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Verdict {
    /// The start is not (or no longer) hung; stop watching it.
    StandDown,
    /// Begin the same start again from `Null`, into the pending state.
    Restart,
    /// Every attempt hung; report the track as unplayable.
    GiveUp,
}

/// Decides what to do with the `attempt`-th start (counted from 1) of a track.
pub(super) fn judge(observation: StartObservation, attempt: u32) -> Verdict {
    let hung = observation.current == gst::State::Ready
        && matches!(
            observation.pending,
            gst::State::Paused | gst::State::Playing
        );
    if !observation.same_stream || !hung {
        Verdict::StandDown
    } else if attempt < START_ATTEMPTS {
        Verdict::Restart
    } else {
        Verdict::GiveUp
    }
}

/// Whether a start of `uri` is one the watchdog covers: a plain local file,
/// not a CUE segment (whose start finishes on the bus).
pub(super) fn covers(uri: &str, live: bool, segment: Option<(i64, i64)>) -> bool {
    !live && segment.is_none() && !is_remote_playback_uri(Some(uri))
}

/// Watches the start of the current track. Holds only clones of what the
/// player already shares, so a timer in flight never keeps a `Player` alive.
pub(super) struct StartWatchdog {
    playbin: Arc<Mutex<gst::Element>>,
    stream_generation: Arc<AtomicU64>,
    on_event: Arc<dyn Fn(PlayerEvent) + Send + Sync>,
    /// Bumped to cancel the timer in flight, whichever one that is.
    epoch: Arc<AtomicU64>,
    pub(super) deadline: Duration,
}

impl StartWatchdog {
    pub(super) fn new(
        playbin: Arc<Mutex<gst::Element>>,
        stream_generation: Arc<AtomicU64>,
        on_event: Arc<dyn Fn(PlayerEvent) + Send + Sync>,
    ) -> Self {
        Self {
            playbin,
            stream_generation,
            on_event,
            epoch: Arc::new(AtomicU64::new(0)),
            deadline: START_DEADLINE,
        }
    }

    /// Stops watching. Idempotent; a timer already queued ends at its next look.
    pub(super) fn disarm(&self) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
    }

    /// Watches the start that has just been handed to the pipeline as stream
    /// `generation`, replacing any earlier watch. The timer runs on the default
    /// main context — the one the bus watch is dispatched from.
    pub(super) fn arm(&self, generation: u64) {
        let epoch = self.epoch.fetch_add(1, Ordering::SeqCst) + 1;
        let look = Look {
            playbin: self.playbin.clone(),
            stream_generation: self.stream_generation.clone(),
            on_event: self.on_event.clone(),
            epoch: self.epoch.clone(),
        };
        let mut attempt = 1;
        gst::glib::timeout_add(self.deadline, move || {
            let next = look.at_deadline(epoch, generation, attempt);
            attempt += 1;
            next
        });
    }
}

impl Drop for StartWatchdog {
    fn drop(&mut self) {
        self.disarm();
    }
}

/// One timer's view of the player, moved into the timer closure.
struct Look {
    playbin: Arc<Mutex<gst::Element>>,
    stream_generation: Arc<AtomicU64>,
    on_event: Arc<dyn Fn(PlayerEvent) + Send + Sync>,
    epoch: Arc<AtomicU64>,
}

impl Look {
    fn at_deadline(&self, epoch: u64, generation: u64, attempt: u32) -> gst::glib::ControlFlow {
        if self.epoch.load(Ordering::SeqCst) != epoch {
            return gst::glib::ControlFlow::Break;
        }
        // The lock is held from the look to the restart, so a `play` cannot
        // slip in between and be restarted over.
        let playbin = self.playbin.lock().unwrap_or_else(PoisonError::into_inner);
        let (_, current, pending) = playbin.state(gst::ClockTime::ZERO);
        let observation = StartObservation {
            current,
            pending,
            same_stream: self.stream_generation.load(Ordering::SeqCst) == generation,
        };
        match judge(observation, attempt) {
            Verdict::StandDown => gst::glib::ControlFlow::Break,
            Verdict::Restart => {
                tracing::warn!(
                    attempt,
                    ?pending,
                    "playbin3 hung in READY on a local start; starting over"
                );
                match restart(&playbin, pending) {
                    Ok(()) => gst::glib::ControlFlow::Continue,
                    Err(error) => {
                        drop(playbin);
                        self.give_up(&format!("restart failed: {error}"));
                        gst::glib::ControlFlow::Break
                    }
                }
            }
            Verdict::GiveUp => {
                drop(playbin);
                self.give_up("the pipeline never left READY");
                gst::glib::ControlFlow::Break
            }
        }
    }

    fn give_up(&self, why: &str) {
        tracing::error!(why, "local start abandoned after {START_ATTEMPTS} attempts");
        (self.on_event)(PlayerEvent::Error(PlaybackFailure::new(
            format!("The track did not start: {why}"),
            reprise_core::playback::PlaybackFailureKind::Other,
            PlaybackSessionId::UNSCOPED,
        )));
    }
}

/// The same start again: `Null` clears the hang, and the URI, the gain and the
/// buffering flags survive it as properties of the playbin.
fn restart(playbin: &gst::Element, target: gst::State) -> Result<(), gst::StateChangeError> {
    playbin.set_state(gst::State::Null)?;
    playbin.set_state(target)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observed(current: gst::State, pending: gst::State, same_stream: bool) -> StartObservation {
        StartObservation {
            current,
            pending,
            same_stream,
        }
    }

    fn hung() -> StartObservation {
        observed(gst::State::Ready, gst::State::Playing, true)
    }

    #[test]
    fn a_start_stuck_in_ready_is_started_over_until_the_attempts_run_out() {
        assert_eq!(judge(hung(), 1), Verdict::Restart);
        assert_eq!(judge(hung(), START_ATTEMPTS - 1), Verdict::Restart);
        assert_eq!(judge(hung(), START_ATTEMPTS), Verdict::GiveUp);
    }

    #[test]
    fn a_hung_start_into_paused_is_hung_too() {
        let paused = observed(gst::State::Ready, gst::State::Paused, true);
        assert_eq!(judge(paused, 1), Verdict::Restart);
    }

    #[test]
    fn a_start_that_left_ready_is_left_alone() {
        for current in [gst::State::Paused, gst::State::Playing, gst::State::Null] {
            let started = observed(current, gst::State::VoidPending, true);
            assert_eq!(judge(started, 1), Verdict::StandDown, "{current:?}");
        }
        let prerolling = observed(gst::State::Paused, gst::State::Playing, true);
        assert_eq!(judge(prerolling, START_ATTEMPTS), Verdict::StandDown);
    }

    #[test]
    fn ready_with_nothing_pending_is_not_a_hang() {
        let idle = observed(gst::State::Ready, gst::State::VoidPending, true);
        assert_eq!(judge(idle, START_ATTEMPTS), Verdict::StandDown);
    }

    #[test]
    fn a_stream_that_was_superseded_is_left_alone() {
        let superseded = observed(gst::State::Ready, gst::State::Playing, false);
        assert_eq!(judge(superseded, 1), Verdict::StandDown);
        assert_eq!(judge(superseded, START_ATTEMPTS), Verdict::StandDown);
    }

    #[test]
    fn only_a_plain_local_file_is_covered() {
        assert!(covers("file:///music/a.flac", false, None));
        assert!(!covers("file:///music/a.flac", true, None));
        assert!(!covers("file:///music/a.flac", false, Some((0, 1_000))));
        assert!(!covers("https://radio.example/live", false, None));
    }
}
