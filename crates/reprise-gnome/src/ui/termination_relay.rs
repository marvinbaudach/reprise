//! The thread side of START-5: turning signals into at most one message for
//! the main loop, deciding what a repeated signal means, and making sure a
//! main loop that never answers cannot keep the process alive.
//!
//! Signal handlers cannot touch GTK, so a listener thread receives each signal
//! and either forwards it to the main loop or ends the process. This module is
//! the decision and the plumbing, free of GTK, so it can be exercised without
//! raising a real signal in the test binary.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const LISTENER_THREAD_NAME: &str = "termination-signals";
const WATCHDOG_THREAD_NAME: &str = "termination-watchdog";

/// How long after the main loop took the first request a repeat counts as a
/// wedged shutdown. A closing terminal sends SIGHUP twice and systemd follows
/// SIGTERM with SIGHUP within milliseconds; those must not end the process
/// before the session is on disk.
pub(super) const REPEAT_GRACE: Duration = Duration::from_secs(3);

/// How long the main loop may leave the first request unread before the
/// process is ended anyway. A main loop that is blocked never answers, and
/// without a repeat signal nothing else would end the process. Longer than
/// [`REPEAT_GRACE`] so a loop that is merely slow to start still saves.
pub(super) const WEDGE_LIMIT: Duration = Duration::from_secs(10);

const _: () = assert!(WEDGE_LIMIT.as_millis() > REPEAT_GRACE.as_millis());

/// The request the main loop took, and when.
#[derive(Clone, Copy)]
struct Taken {
    at: Instant,
    signal: i32,
}

/// What the listener and the main loop share.
#[derive(Default)]
pub(super) struct Shared {
    taken: Mutex<Option<Taken>>,
    released: AtomicBool,
}

impl Shared {
    /// The main loop calls this the moment it reads the first request, with
    /// the signal that request carried.
    pub(super) fn mark_taken(&self, signal: i32) {
        let mut taken = self.taken.lock().unwrap_or_else(PoisonError::into_inner);
        taken.get_or_insert_with(|| Taken {
            at: Instant::now(),
            signal,
        });
    }

    /// The signal the main loop handled, if it handled one.
    pub(super) fn handled_signal(&self) -> Option<i32> {
        self.taken().map(|taken| taken.signal)
    }

    /// The application has stopped running: no main loop is left to act, so
    /// every signal from now on takes its normal course.
    pub(super) fn release(&self) {
        self.released.store(true, Ordering::SeqCst);
    }

    fn taken(&self) -> Option<Taken> {
        *self.taken.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn taken_at(&self) -> Option<Instant> {
        self.taken().map(|taken| taken.at)
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
/// request and [`REPEAT_GRACE`] has passed since, whether or not the
/// application has stopped running meanwhile: a SIGTERM followed by a SIGHUP
/// during teardown must not cut the database close short. A main loop that
/// never takes the first request is ended by the watchdog instead (see
/// [`spawn`]), and so is a repeat once nothing is left to take it.
pub(super) fn verdict(
    forwarded: bool,
    taken_at: Option<Instant>,
    released: bool,
    now: Instant,
) -> Verdict {
    if !forwarded {
        return if released {
            Verdict::EndProcess
        } else {
            Verdict::Forward
        };
    }
    match taken_at {
        Some(taken_at) if now.saturating_duration_since(taken_at) >= REPEAT_GRACE => {
            Verdict::EndProcess
        }
        Some(_) => Verdict::Coalesce,
        None if released => Verdict::EndProcess,
        None => Verdict::Coalesce,
    }
}

/// Runs `source` on the listener thread. `source` feeds every received signal
/// to the callback it is given; `end_process` is how a signal ends the
/// process.
///
/// Forwarding the first request also starts a one-shot watchdog: if the main
/// loop has not taken it after `wedge_limit`, `end_process` runs for it. That
/// is what ends a wedged process that is never sent a second signal.
pub(super) fn spawn(
    source: impl FnOnce(&mut dyn FnMut(i32)) + Send + 'static,
    shared: Arc<Shared>,
    sender: async_channel::Sender<i32>,
    end_process: impl Fn(i32) + Send + Sync + 'static,
    wedge_limit: Duration,
) -> io::Result<JoinHandle<()>> {
    let end_process = Arc::new(end_process);
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
                    Verdict::Forward if sender.try_send(signal).is_ok() => {
                        forwarded = true;
                        watch(shared.clone(), end_process.clone(), signal, wedge_limit);
                    }
                    Verdict::Forward | Verdict::EndProcess => end_process(signal),
                    Verdict::Coalesce => {
                        tracing::debug!(signal, "repeated termination request coalesced");
                    }
                }
            });
        })
}

/// Ends the process for `signal` if the main loop has still not taken it after
/// `wedge_limit`.
fn watch(
    shared: Arc<Shared>,
    end_process: Arc<impl Fn(i32) + Send + Sync + 'static>,
    signal: i32,
    wedge_limit: Duration,
) {
    let watchdog = thread::Builder::new()
        .name(WATCHDOG_THREAD_NAME.into())
        .spawn(move || {
            thread::sleep(wedge_limit);
            if shared.taken_at().is_none() {
                tracing::error!(
                    signal,
                    "the main loop never took the termination request; ending the process"
                );
                end_process(signal);
            }
        });
    if let Err(error) = watchdog {
        tracing::warn!(%error, "could not start the termination watchdog");
    }
}

/// After the application has stopped and been torn down, ends the process the
/// way the handled signal normally would, so the shell or service manager
/// sees death by that signal (status 128 plus its number) rather than a
/// plain exit. Does nothing when no request was handled.
pub(super) fn end_as_handled(shared: &Shared, end_process: impl Fn(i32)) {
    if let Some(signal) = shared.handled_signal() {
        end_process(signal);
    }
}

/// Registers the listener unless `cell` already holds one. A second
/// registration would install a second set of handlers and a second thread,
/// and the first of them to see a signal would end the process unsaved.
pub(super) fn start_once<T>(
    cell: &OnceLock<T>,
    register: impl FnOnce() -> io::Result<Option<T>>,
) -> io::Result<Option<&T>> {
    if cell.get().is_some() {
        tracing::debug!("termination listener already running; not registering a second one");
        return Ok(None);
    }
    Ok(register()?.map(|listener| cell.get_or_init(|| listener)))
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

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    const SIGNAL_A: i32 = 1;
    const SIGNAL_B: i32 = 15;

    /// A wedge limit no test waits out: the watchdog stays asleep.
    const NEVER: Duration = Duration::from_secs(3600);
    const SHORT_WEDGE_LIMIT: Duration = Duration::from_millis(50);
    /// Long enough for a watchdog with `SHORT_WEDGE_LIMIT` to have fired.
    const SETTLED: Duration = SHORT_WEDGE_LIMIT.saturating_mul(6);
    const WAIT_LIMIT: Duration = Duration::from_secs(10);

    /// The real listener thread, fed by hand, with a stand-in for ending the
    /// process that reports every signal it was asked to end on.
    struct Relay {
        feed: mpsc::Sender<i32>,
        received: async_channel::Receiver<i32>,
        ended: mpsc::Receiver<i32>,
        listener: JoinHandle<()>,
    }

    impl Relay {
        fn start(shared: &Arc<Shared>, wedge_limit: Duration) -> Self {
            let (feed, source) = mpsc::channel();
            let (sender, received) = async_channel::bounded(1);
            let (ended, ended_signals) = mpsc::channel();
            let ended = Mutex::new(ended);
            let listener = spawn(
                move |deliver| source.into_iter().for_each(deliver),
                shared.clone(),
                sender,
                move |signal| {
                    let _ = ended.lock().unwrap().send(signal);
                },
                wedge_limit,
            )
            .unwrap();
            Self {
                feed,
                received,
                ended: ended_signals,
                listener,
            }
        }

        fn send(&self, signals: &[i32]) {
            for signal in signals {
                self.feed.send(*signal).unwrap();
            }
        }

        /// Blocks until the listener has forwarded a request to the main loop.
        fn forwarded(&self) -> i32 {
            let deadline = Instant::now() + WAIT_LIMIT;
            loop {
                if let Ok(signal) = self.received.try_recv() {
                    return signal;
                }
                assert!(Instant::now() < deadline, "no request was forwarded");
                thread::yield_now();
            }
        }

        /// Ends the feed, waits for the listener to drain it and returns the
        /// requests the main loop would still find and the signals that ended
        /// the process.
        fn finish(self) -> (Vec<i32>, Vec<i32>) {
            drop(self.feed);
            self.listener.join().unwrap();
            let received = std::iter::from_fn(|| self.received.try_recv().ok()).collect();
            (received, self.ended.try_iter().collect())
        }
    }

    /// Runs `signals` through a listener whose watchdog never fires.
    fn relay(shared: &Arc<Shared>, signals: &[i32]) -> (Vec<i32>, usize) {
        let relay = Relay::start(shared, NEVER);
        relay.send(signals);
        let (received, ended) = relay.finish();
        (received, ended.len())
    }

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
    fn start_5c_a_first_signal_after_the_application_stopped_ends_the_process() {
        assert_eq!(
            verdict(false, None, true, Instant::now()),
            Verdict::EndProcess
        );
    }

    #[test]
    fn start_5c_a_repeat_inside_the_grace_is_coalesced_after_the_application_stopped() {
        let taken = Instant::now();
        let now = taken + Duration::from_millis(1);
        assert_eq!(verdict(true, Some(taken), true, now), Verdict::Coalesce);
    }

    #[test]
    fn start_5c_a_repeat_after_the_grace_ends_the_process_once_the_application_stopped() {
        let taken = Instant::now();
        let now = taken + REPEAT_GRACE;
        assert_eq!(verdict(true, Some(taken), true, now), Verdict::EndProcess);
    }

    #[test]
    fn start_5c_a_stopped_application_ends_a_request_nothing_will_take() {
        assert_eq!(
            verdict(true, None, true, Instant::now()),
            Verdict::EndProcess
        );
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
        shared.mark_taken(SIGNAL_A);

        let (received, ended) = relay(&shared, &[SIGNAL_A, SIGNAL_B, SIGNAL_A]);

        assert_eq!(received, vec![SIGNAL_A]);
        assert_eq!(ended, 0);
    }

    #[test]
    fn start_5c_a_first_signal_reaching_a_released_listener_ends_the_process() {
        let shared = Arc::new(Shared::default());
        shared.release();

        let (received, ended) = relay(&shared, &[SIGNAL_A]);

        assert!(received.is_empty());
        assert_eq!(ended, 1);
    }

    #[test]
    fn start_5c_a_repeat_during_teardown_does_not_end_the_process_inside_the_grace() {
        let shared = Arc::new(Shared::default());
        let relay = Relay::start(&shared, NEVER);
        relay.send(&[SIGNAL_A]);
        assert_eq!(relay.forwarded(), SIGNAL_A);
        shared.mark_taken(SIGNAL_A);
        shared.release();
        relay.send(&[SIGNAL_B]);

        let (_, ended) = relay.finish();

        assert!(ended.is_empty(), "the repeat cut the teardown short");
    }

    #[test]
    fn start_5c_a_request_the_main_loop_never_takes_ends_the_process_without_a_repeat() {
        let relay = Relay::start(&Arc::new(Shared::default()), SHORT_WEDGE_LIMIT);
        relay.send(&[SIGNAL_A]);

        let ended = relay.ended.recv_timeout(WAIT_LIMIT);

        assert_eq!(ended, Ok(SIGNAL_A), "the watchdog ended the wedged process");
        relay.finish();
    }

    #[test]
    fn start_5c_the_watchdog_leaves_a_request_the_main_loop_took_alone() {
        let shared = Arc::new(Shared::default());
        shared.mark_taken(SIGNAL_A);
        let relay = Relay::start(&shared, SHORT_WEDGE_LIMIT);
        relay.send(&[SIGNAL_A]);

        let ended = relay.ended.recv_timeout(SETTLED);

        assert_eq!(ended, Err(mpsc::RecvTimeoutError::Timeout));
        relay.finish();
    }

    #[test]
    fn start_5c_the_wedge_limit_outlasts_the_repeat_grace() {
        assert!(WEDGE_LIMIT > REPEAT_GRACE);
    }

    #[test]
    fn start_5e_a_handled_signal_ends_the_process_the_way_that_signal_would() {
        let shared = Shared::default();
        shared.mark_taken(SIGNAL_B);
        let ended = std::cell::RefCell::new(Vec::new());

        end_as_handled(&shared, |signal| ended.borrow_mut().push(signal));

        assert_eq!(*ended.borrow(), vec![SIGNAL_B]);
    }

    #[test]
    fn start_5e_a_normal_exit_without_a_handled_signal_ends_nothing() {
        let ended = std::cell::Cell::new(0);

        end_as_handled(&Shared::default(), |_| ended.set(ended.get() + 1));

        assert_eq!(ended.get(), 0);
    }

    #[test]
    fn start_5e_the_first_handled_signal_decides_the_exit() {
        let shared = Shared::default();
        shared.mark_taken(SIGNAL_A);
        shared.mark_taken(SIGNAL_B);

        assert_eq!(shared.handled_signal(), Some(SIGNAL_A));
    }

    #[test]
    fn start_5f_a_second_start_registers_nothing() {
        let cell = OnceLock::new();
        let registered = std::cell::Cell::new(0);
        let register = || {
            registered.set(registered.get() + 1);
            Ok(Some(registered.get()))
        };

        let first = start_once(&cell, register).unwrap();
        let second = start_once(&cell, register).unwrap();

        assert_eq!(first, Some(&1));
        assert_eq!(second, None);
        assert_eq!(registered.get(), 1, "the second start must not register");
    }

    #[test]
    fn start_5f_a_start_that_armed_nothing_may_be_retried() {
        let cell: OnceLock<u8> = OnceLock::new();

        assert_eq!(start_once(&cell, || Ok(None)).unwrap(), None);
        assert_eq!(start_once(&cell, || Ok(Some(7))).unwrap(), Some(&7));
    }

    #[test]
    fn start_5d_signals_that_were_ignored_at_start_are_not_armed() {
        let armed = armed(&[SIGNAL_A, SIGNAL_B, 2], |signal| signal == SIGNAL_A);

        assert_eq!(armed, vec![SIGNAL_B, 2]);
    }
}
