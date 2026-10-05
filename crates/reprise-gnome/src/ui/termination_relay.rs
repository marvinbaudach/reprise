//! The thread side of START-5: turning signals into at most one message for
//! the main loop, and deciding what a repeated signal means.
//!
//! Signal handlers cannot touch GTK, so a listener thread receives each signal
//! and either forwards it to the main loop or ends the process. This module is
//! the decision and the plumbing, free of GTK, so it can be exercised without
//! raising a real signal in the test binary.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const LISTENER_THREAD_NAME: &str = "termination-signals";

/// How long after the main loop took the first request a repeat counts as a
/// wedged shutdown. A closing terminal sends SIGHUP twice and systemd follows
/// SIGTERM with SIGHUP within milliseconds; those must not end the process
/// before the session is on disk.
pub(super) const REPEAT_GRACE: Duration = Duration::from_secs(3);

/// What the listener and the main loop share.
#[derive(Default)]
pub(super) struct Shared {
    taken_at: Mutex<Option<Instant>>,
    released: AtomicBool,
}

impl Shared {
    /// The main loop calls this the moment it reads the first request.
    pub(super) fn mark_taken(&self) {
        let mut taken_at = self.taken_at.lock().unwrap_or_else(PoisonError::into_inner);
        taken_at.get_or_insert_with(Instant::now);
    }

    /// The application has stopped running: no main loop is left to act, so
    /// every signal from now on takes its normal course.
    pub(super) fn release(&self) {
        self.released.store(true, Ordering::SeqCst);
    }

    fn taken_at(&self) -> Option<Instant> {
        *self.taken_at.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn is_released(&self) -> bool {
        self.released.load(Ordering::SeqCst)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Verdict {
    /// The first request: hand it to the main loop.
    Forward,
    /// A repeat while the first is still being handled: nothing to do.
    Coalesce,
    /// The signal takes its normal course and ends the process.
    EndProcess,
}

/// A repeat ends the process only once the main loop has taken the first
/// request and [`REPEAT_GRACE`] has passed since. If the main loop never takes
/// it, repeats stay coalesced; SIGKILL (or the service manager's stop timeout)
/// is the backstop for a main loop that is not iterating at all.
pub(super) fn verdict(
    forwarded: bool,
    taken_at: Option<Instant>,
    released: bool,
    now: Instant,
) -> Verdict {
    if released {
        return Verdict::EndProcess;
    }
    if !forwarded {
        return Verdict::Forward;
    }
    match taken_at {
        Some(taken_at) if now.saturating_duration_since(taken_at) >= REPEAT_GRACE => {
            Verdict::EndProcess
        }
        _ => Verdict::Coalesce,
    }
}

/// Runs `source` on the listener thread. `source` feeds every received signal
/// to the callback it is given; `end_process` is how a signal ends the
/// process.
pub(super) fn spawn(
    source: impl FnOnce(&mut dyn FnMut(i32)) + Send + 'static,
    shared: std::sync::Arc<Shared>,
    sender: async_channel::Sender<i32>,
    end_process: impl Fn(i32) + Send + 'static,
) -> io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name(LISTENER_THREAD_NAME.into())
        .spawn(move || {
            let mut forwarded = false;
            source(&mut |signal| {
                let verdict = verdict(
                    forwarded,
                    shared.taken_at(),
                    shared.is_released(),
                    Instant::now(),
                );
                match verdict {
                    Verdict::Forward if sender.try_send(signal).is_ok() => forwarded = true,
                    Verdict::Forward | Verdict::EndProcess => end_process(signal),
                    Verdict::Coalesce => {
                        tracing::debug!(signal, "repeated termination request coalesced");
                    }
                }
            });
        })
}

/// The signals of `candidates` the process should listen for: those whose
/// inherited disposition is not "ignore". `nohup reprise &` and a background
/// job start with SIGHUP or SIGINT ignored and must keep ignoring it.
pub(super) fn armed(candidates: &[i32], is_ignored: impl Fn(i32) -> bool) -> Vec<i32> {
    candidates
        .iter()
        .copied()
        .filter(|signal| !is_ignored(*signal))
        .collect()
}

/// Whether the process currently ignores `signal` (`SIG_IGN`).
pub(super) fn is_ignored(signal: i32) -> bool {
    let mut current = std::mem::MaybeUninit::<libc::sigaction>::zeroed();
    // SAFETY: a null new action only queries the current disposition, and
    // `current` is a valid, zero-initialised out-pointer that `sigaction`
    // fills when it returns 0. An all-zero `sigaction` is a valid value.
    let queried = unsafe { libc::sigaction(signal, std::ptr::null(), current.as_mut_ptr()) } == 0;
    // SAFETY: the call above succeeded, so `current` is initialised.
    queried && unsafe { current.assume_init() }.sa_sigaction == libc::SIG_IGN
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;
    use std::sync::{mpsc, Arc};

    use super::*;

    const SIGNAL_A: i32 = 1;
    const SIGNAL_B: i32 = 15;

    #[test]
    fn start_5c_the_first_request_is_forwarded() {
        assert_eq!(
            verdict(false, None, false, Instant::now()),
            Verdict::Forward
        );
    }

    #[test]
    fn start_5c_a_repeat_before_the_main_loop_took_the_first_is_coalesced() {
        assert_eq!(
            verdict(true, None, false, Instant::now()),
            Verdict::Coalesce
        );
    }

    #[test]
    fn start_5c_a_repeat_inside_the_grace_after_the_take_is_coalesced() {
        let taken = Instant::now();
        let now = taken + REPEAT_GRACE - Duration::from_millis(1);
        assert_eq!(verdict(true, Some(taken), false, now), Verdict::Coalesce);
    }

    #[test]
    fn start_5c_a_repeat_after_the_grace_ends_the_process() {
        let taken = Instant::now();
        let now = taken + REPEAT_GRACE;
        assert_eq!(verdict(true, Some(taken), false, now), Verdict::EndProcess);
    }

    #[test]
    fn start_5c_a_signal_after_the_application_stopped_ends_the_process() {
        assert_eq!(
            verdict(false, None, true, Instant::now()),
            Verdict::EndProcess
        );
    }

    /// Runs `signals` through the real listener thread with a counting stand-in
    /// for ending the process, and returns what the main loop would receive and
    /// how many times the process was ended.
    fn relay(shared: &Arc<Shared>, signals: &[i32]) -> (Vec<i32>, usize) {
        let (feed, source) = mpsc::channel();
        let (sender, receiver) = async_channel::bounded(1);
        let ended = Arc::new(AtomicUsize::new(0));
        let ended_by_thread = ended.clone();
        let handle = spawn(
            move |deliver| source.into_iter().for_each(deliver),
            shared.clone(),
            sender,
            move |_| {
                ended_by_thread.fetch_add(1, Ordering::SeqCst);
            },
        )
        .unwrap();
        for signal in signals {
            feed.send(*signal).unwrap();
        }
        drop(feed);
        handle.join().unwrap();
        let received = std::iter::from_fn(|| receiver.try_recv().ok()).collect();
        (received, ended.load(Ordering::SeqCst))
    }

    #[test]
    fn start_5c_two_signals_in_quick_succession_reach_the_main_loop_once_and_end_nothing() {
        let (received, ended) = relay(&Arc::new(Shared::default()), &[SIGNAL_A, SIGNAL_B]);

        assert_eq!(received, vec![SIGNAL_A]);
        assert_eq!(
            ended, 0,
            "the repeat must not end the process before the save"
        );
    }

    #[test]
    fn start_5c_a_repeat_stays_coalesced_while_the_main_loop_is_saving() {
        let shared = Arc::new(Shared::default());
        shared.mark_taken();

        let (received, ended) = relay(&shared, &[SIGNAL_A, SIGNAL_B, SIGNAL_A]);

        assert_eq!(received, vec![SIGNAL_A]);
        assert_eq!(ended, 0);
    }

    #[test]
    fn start_5c_a_released_listener_lets_every_signal_end_the_process() {
        let shared = Arc::new(Shared::default());
        shared.release();

        let (received, ended) = relay(&shared, &[SIGNAL_A]);

        assert!(received.is_empty());
        assert_eq!(ended, 1);
    }

    #[test]
    fn start_5d_signals_that_were_ignored_at_start_are_not_armed() {
        let armed = armed(&[SIGNAL_A, SIGNAL_B, 2], |signal| signal == SIGNAL_A);

        assert_eq!(armed, vec![SIGNAL_B, 2]);
    }

    #[test]
    fn start_5d_the_inherited_ignore_disposition_is_detected() {
        // SIGUSR1 and SIGUSR2 are free in the test binary: nothing listens.
        // SAFETY: `signal` only swaps the disposition of a signal nothing else
        // in this process uses, and the test restores it before returning.
        unsafe {
            libc::signal(libc::SIGUSR1, libc::SIG_IGN);
        }
        let ignored = is_ignored(libc::SIGUSR1);
        let default_kept = is_ignored(libc::SIGUSR2);
        // SAFETY: as above.
        unsafe {
            libc::signal(libc::SIGUSR1, libc::SIG_DFL);
        }

        assert!(ignored, "SIG_IGN is detected");
        assert!(!default_kept, "the default disposition is not ignored");
    }
}
