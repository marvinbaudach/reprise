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
//! A start that is still in READY is not necessarily hung: a spun-down disk or
//! a network share keeps a healthy one there while the file is read in. So the
//! watchdog does not count time since the start but time since the file source
//! last moved. [`StartWatchdog::arm`] is called once a local-file start has
//! been handed to the pipeline and looks at it every [`POLLS_PER_DEADLINE`]th
//! of the deadline. A start that is READY with a state pending and whose source
//! has not moved for the deadline is begun again, up to [`START_ATTEMPTS`]
//! starts in all, each allowed longer than the one before, and the last hung
//! one is reported as a [`PlayerEvent::Error`], which the frontend answers by
//! skipping the track.
//!
//! CUE segment starts and the crossfade secondary start do not arm it.

use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use reprise_core::playback::{PlaybackFailure, PlaybackFailureKind, PlayerEvent};

use crate::player_pipeline::{is_remote_playback_uri, next_playback_session_id};

/// How long the source of a start in READY may stand still before the first
/// attempt counts as hung.
pub(super) const START_DEADLINE: Duration = Duration::from_secs(4);

/// How many times one track is started, the first start included. Attempt `n`
/// is allowed `n` deadlines, so the three of them give up after 4 + 8 + 12 s
/// of a source that never moves.
pub(super) const START_ATTEMPTS: u32 = 3;

/// How often a start is looked at, per deadline.
const POLLS_PER_DEADLINE: u32 = 8;

/// What a start looked like at one look.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct StartObservation {
    pub(super) current: gst::State,
    pub(super) pending: gst::State,
    /// `false` once something else started: a new `play`, a gapless step or a
    /// crossfade promotion all bump the stream generation.
    pub(super) same_stream: bool,
    /// How long the file source has not moved (or, before it has shown up at
    /// all, how long since the start).
    pub(super) idle: Duration,
}

/// What to do about a start at one look.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Verdict {
    /// The start is not (or no longer) hung; stop watching it.
    StandDown,
    /// It looks hung but has not yet been still for its whole deadline.
    Wait,
    /// Begin the same start again from `Null`, into the pending state.
    Restart,
    /// Every attempt hung; report the track as unplayable.
    GiveUp,
}

/// How long the `attempt`-th start (counted from 1) may stand still: a slow
/// source that really is stuck gets longer each time before the track is lost.
pub(super) fn allowed_idle(deadline: Duration, attempt: u32) -> Duration {
    deadline * attempt
}

/// Decides what to do with the `attempt`-th start (counted from 1) of a track.
pub(super) fn judge(observation: StartObservation, attempt: u32, deadline: Duration) -> Verdict {
    let hung = observation.current == gst::State::Ready
        && matches!(
            observation.pending,
            gst::State::Paused | gst::State::Playing
        );
    if !observation.same_stream || !hung {
        Verdict::StandDown
    } else if observation.idle < allowed_idle(deadline, attempt) {
        Verdict::Wait
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

/// How far the pipeline's file source has read, in bytes, if it has one yet.
/// `filesrc` answers the query in pull mode too, which is how a local file is
/// read.
fn source_position(playbin: &gst::Element) -> Option<u64> {
    let bin = playbin.downcast_ref::<gst::Bin>()?;
    let mut elements = bin.iterate_recurse();
    while let Ok(Some(element)) = elements.next() {
        if element
            .factory()
            .is_some_and(|factory| factory.name() == "filesrc")
        {
            return element
                .query_position::<gst::format::Bytes>()
                .map(Into::into);
        }
    }
    None
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
            deadline: self.deadline,
        };
        let mut progress = Progress::starting(Instant::now());
        gst::glib::timeout_add(self.deadline / POLLS_PER_DEADLINE, move || {
            look.at_poll(epoch, generation, &mut progress)
        });
    }
}

impl Drop for StartWatchdog {
    fn drop(&mut self) {
        self.disarm();
    }
}

/// How one watched start has gone so far; owned by its timer.
struct Progress {
    attempt: u32,
    source_position: Option<u64>,
    /// When the source last moved, or the attempt began.
    moved_at: Instant,
}

impl Progress {
    fn starting(now: Instant) -> Self {
        Self {
            attempt: 1,
            source_position: None,
            moved_at: now,
        }
    }

    /// Notes where the source is now and returns how long it has stood still.
    fn idle_at(&mut self, now: Instant, source_position: Option<u64>) -> Duration {
        if source_position.is_some() && source_position != self.source_position {
            self.source_position = source_position;
            self.moved_at = now;
        }
        now.saturating_duration_since(self.moved_at)
    }

    /// The same start begun again: a new attempt with a new source.
    fn restarted(&mut self, now: Instant) {
        self.attempt += 1;
        self.source_position = None;
        self.moved_at = now;
    }
}

/// One timer's view of the player, moved into the timer closure.
struct Look {
    playbin: Arc<Mutex<gst::Element>>,
    stream_generation: Arc<AtomicU64>,
    on_event: Arc<dyn Fn(PlayerEvent) + Send + Sync>,
    epoch: Arc<AtomicU64>,
    deadline: Duration,
}

impl Look {
    fn at_poll(
        &self,
        epoch: u64,
        generation: u64,
        progress: &mut Progress,
    ) -> gst::glib::ControlFlow {
        if self.epoch.load(Ordering::SeqCst) != epoch {
            return gst::glib::ControlFlow::Break;
        }
        // The lock is held from the look to the restart, so a `play` cannot
        // slip in between and be restarted over.
        let playbin = self.playbin.lock().unwrap_or_else(PoisonError::into_inner);
        let (_, current, pending) = playbin.state(gst::ClockTime::ZERO);
        let now = Instant::now();
        let observation = StartObservation {
            current,
            pending,
            same_stream: self.stream_generation.load(Ordering::SeqCst) == generation,
            idle: progress.idle_at(now, source_position(&playbin)),
        };
        match judge(observation, progress.attempt, self.deadline) {
            Verdict::StandDown => gst::glib::ControlFlow::Break,
            Verdict::Wait => gst::glib::ControlFlow::Continue,
            Verdict::Restart => {
                tracing::warn!(
                    attempt = progress.attempt,
                    ?pending,
                    "playbin3 hung in READY on a local start; starting over"
                );
                match restart(&playbin, pending) {
                    Ok(()) => {
                        progress.restarted(now);
                        gst::glib::ControlFlow::Continue
                    }
                    Err(error) => {
                        // A state change that fails has put its own error on
                        // the bus, with the session it belongs to; a second
                        // report from here would be taken for another track's.
                        tracing::error!(%error, "could not start the hung track over");
                        gst::glib::ControlFlow::Break
                    }
                }
            }
            Verdict::GiveUp => {
                drop(playbin);
                self.give_up();
                gst::glib::ControlFlow::Break
            }
        }
    }

    /// Reports the track as unplayable. The report is its own session, as a
    /// bus error is one per start: the frontend answers the first error of a
    /// session and ignores its repeats, and must not take this for a repeat of
    /// the last track's.
    fn give_up(&self) {
        tracing::error!("local start abandoned after {START_ATTEMPTS} attempts");
        (self.on_event)(PlayerEvent::Error(PlaybackFailure::new(
            "The track did not start: the pipeline never left READY",
            PlaybackFailureKind::StartNeverFinished,
            next_playback_session_id(),
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

    const DEADLINE: Duration = Duration::from_secs(4);

    fn observed(
        current: gst::State,
        pending: gst::State,
        same_stream: bool,
        idle: Duration,
    ) -> StartObservation {
        StartObservation {
            current,
            pending,
            same_stream,
            idle,
        }
    }

    /// Hung, and still for as long as the `attempt`-th start is allowed.
    fn hung(attempt: u32) -> StartObservation {
        observed(
            gst::State::Ready,
            gst::State::Playing,
            true,
            allowed_idle(DEADLINE, attempt),
        )
    }

    #[test]
    fn a_start_stuck_in_ready_is_started_over_until_the_attempts_run_out() {
        assert_eq!(judge(hung(1), 1, DEADLINE), Verdict::Restart);
        assert_eq!(
            judge(hung(START_ATTEMPTS - 1), START_ATTEMPTS - 1, DEADLINE),
            Verdict::Restart
        );
        assert_eq!(
            judge(hung(START_ATTEMPTS), START_ATTEMPTS, DEADLINE),
            Verdict::GiveUp
        );
    }

    #[test]
    fn a_start_whose_source_still_moves_is_waited_for() {
        let moving = observed(
            gst::State::Ready,
            gst::State::Playing,
            true,
            DEADLINE - Duration::from_millis(1),
        );
        for attempt in 1..=START_ATTEMPTS {
            assert_eq!(judge(moving, attempt, DEADLINE), Verdict::Wait);
        }
    }

    #[test]
    fn every_attempt_is_allowed_longer_than_the_one_before() {
        let still_for_one_deadline = observed(
            gst::State::Ready,
            gst::State::Playing,
            true,
            allowed_idle(DEADLINE, 1),
        );
        assert_eq!(judge(still_for_one_deadline, 1, DEADLINE), Verdict::Restart);
        assert_eq!(judge(still_for_one_deadline, 2, DEADLINE), Verdict::Wait);
        assert_eq!(
            judge(still_for_one_deadline, START_ATTEMPTS, DEADLINE),
            Verdict::Wait
        );
        let total: Duration = (1..=START_ATTEMPTS)
            .map(|attempt| allowed_idle(DEADLINE, attempt))
            .sum();
        assert_eq!(total, Duration::from_secs(24));
    }

    #[test]
    fn a_hung_start_into_paused_is_hung_too() {
        let paused = observed(
            gst::State::Ready,
            gst::State::Paused,
            true,
            allowed_idle(DEADLINE, 1),
        );
        assert_eq!(judge(paused, 1, DEADLINE), Verdict::Restart);
    }

    #[test]
    fn a_start_that_left_ready_is_left_alone() {
        let long = allowed_idle(DEADLINE, START_ATTEMPTS);
        for current in [gst::State::Paused, gst::State::Playing, gst::State::Null] {
            let started = observed(current, gst::State::VoidPending, true, long);
            assert_eq!(
                judge(started, 1, DEADLINE),
                Verdict::StandDown,
                "{current:?}"
            );
        }
        let prerolling = observed(gst::State::Paused, gst::State::Playing, true, long);
        assert_eq!(
            judge(prerolling, START_ATTEMPTS, DEADLINE),
            Verdict::StandDown
        );
    }

    #[test]
    fn ready_with_nothing_pending_is_not_a_hang() {
        let idle = observed(
            gst::State::Ready,
            gst::State::VoidPending,
            true,
            allowed_idle(DEADLINE, START_ATTEMPTS),
        );
        assert_eq!(judge(idle, START_ATTEMPTS, DEADLINE), Verdict::StandDown);
    }

    #[test]
    fn a_stream_that_was_superseded_is_left_alone() {
        let superseded = observed(
            gst::State::Ready,
            gst::State::Playing,
            false,
            allowed_idle(DEADLINE, START_ATTEMPTS),
        );
        assert_eq!(judge(superseded, 1, DEADLINE), Verdict::StandDown);
        assert_eq!(
            judge(superseded, START_ATTEMPTS, DEADLINE),
            Verdict::StandDown
        );
    }

    #[test]
    fn only_a_plain_local_file_is_covered() {
        assert!(covers("file:///music/a.flac", false, None));
        assert!(!covers("file:///music/a.flac", true, None));
        assert!(!covers("file:///music/a.flac", false, Some((0, 1_000))));
        assert!(!covers("https://radio.example/live", false, None));
    }

    #[test]
    fn a_source_that_moves_resets_the_idle_time() {
        let begun = Instant::now();
        let mut progress = Progress::starting(begun);
        let at = |ms| begun + Duration::from_millis(ms);
        // Nothing to read yet: idle since the start.
        assert_eq!(progress.idle_at(at(100), None), Duration::from_millis(100));
        assert_eq!(progress.idle_at(at(200), Some(0)), Duration::ZERO);
        assert_eq!(
            progress.idle_at(at(500), Some(0)),
            Duration::from_millis(300)
        );
        assert_eq!(progress.idle_at(at(600), Some(4_096)), Duration::ZERO);
        // The source being asked about in vain is not movement either.
        assert_eq!(progress.idle_at(at(900), None), Duration::from_millis(300));
    }

    #[test]
    fn a_new_attempt_starts_its_idle_time_over() {
        let begun = Instant::now();
        let mut progress = Progress::starting(begun);
        progress.idle_at(begun + Duration::from_secs(1), Some(10));
        progress.restarted(begun + Duration::from_secs(5));
        assert_eq!(progress.attempt, 2);
        assert_eq!(
            progress.idle_at(begun + Duration::from_secs(6), Some(10)),
            Duration::ZERO,
            "the new source reads from where it reads"
        );
    }
}
