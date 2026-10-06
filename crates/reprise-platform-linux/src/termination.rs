//! Turning termination signals into at most one request for a frontend's main
//! loop (START-5).
//!
//! Signal handlers cannot touch a toolkit, so a listener thread receives each
//! SIGTERM, SIGHUP or SIGINT and either forwards the first to the frontend or
//! ends the process. This module owns that whole relay, free of any toolkit:
//! the signal registration, the listener thread, the decision what a repeated
//! signal means, the watchdog that ends a process whose main loop never
//! answers, and ending the process the way the signal would have.
//!
//! A frontend calls [`start`] once, awaits [`Relay::take_request`] on its main
//! loop and saves and quits when it returns. When the application has stopped
//! it calls [`Relay::release`], and as the last thing the process does
//! [`Relay::finish`], so a shell or service manager sees death by the handled
//! signal (128 plus its number) rather than a plain exit.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use signal_hook::low_level::emulate_default_handler;

const LISTENER_THREAD_NAME: &str = "termination-signals";
const WATCHDOG_THREAD_NAME: &str = "termination-watchdog";

/// How long after the main loop took the first request a repeat counts as a
/// wedged shutdown. A closing terminal sends SIGHUP twice and systemd follows
/// SIGTERM with SIGHUP within milliseconds; those must not end the process
/// before the session is on disk.
const REPEAT_GRACE: Duration = Duration::from_secs(3);

/// How long the main loop may leave the first request unread before the
/// process is ended anyway. A main loop that is blocked never answers, and
/// without a repeat signal nothing else would end the process. Longer than
/// [`REPEAT_GRACE`] so a loop that is merely slow to start still saves.
const WEDGE_LIMIT: Duration = Duration::from_secs(10);

const _: () = assert!(WEDGE_LIMIT.as_millis() > REPEAT_GRACE.as_millis());

/// The request the main loop took, and when.
#[derive(Clone, Copy)]
struct Taken {
    at: Instant,
    signal: i32,
}

/// What the listener and the main loop share.
#[derive(Default)]
struct Shared {
    taken: Mutex<Option<Taken>>,
    released: AtomicBool,
}

impl Shared {
    /// The main loop calls this the moment it reads the first request, with
    /// the signal that request carried.
    fn mark_taken(&self, signal: i32) {
        let mut taken = self.taken.lock().unwrap_or_else(PoisonError::into_inner);
        taken.get_or_insert_with(|| Taken {
            at: Instant::now(),
            signal,
        });
    }

    /// The signal the main loop handled, if it handled one.
    fn handled_signal(&self) -> Option<i32> {
        self.taken().map(|taken| taken.signal)
    }

    /// The application has stopped running: no main loop is left to act, so
    /// every signal from now on takes its normal course.
    fn release(&self) {
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
enum Verdict {
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
fn verdict(forwarded: bool, taken_at: Option<Instant>, released: bool, now: Instant) -> Verdict {
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
fn spawn(
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
fn end_as_handled(shared: &Shared, end_process: impl Fn(i32)) {
    if let Some(signal) = shared.handled_signal() {
        end_process(signal);
    }
}

/// Registers the listener unless `cell` already holds one. A second
/// registration would install a second set of handlers and a second thread,
/// and the first of them to see a signal would end the process unsaved.
fn start_once<T>(
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
fn armed(candidates: &[i32], is_ignored: impl Fn(i32) -> bool) -> Vec<i32> {
    candidates
        .iter()
        .copied()
        .filter(|signal| !is_ignored(*signal))
        .collect()
}

const TERMINATION_SIGNALS: [i32; 3] = [SIGTERM, SIGHUP, SIGINT];

/// The one relay of the process; signal dispositions are process-wide.
static RELAY: OnceLock<Relay> = OnceLock::new();

/// The frontend's end of the relay: the requests the listener forwards and
/// the state the listener decides by.
#[derive(Clone)]
pub struct Relay {
    shared: Arc<Shared>,
    received: async_channel::Receiver<i32>,
}

impl Relay {
    /// Waits for the first termination request and returns its signal number,
    /// recording that the main loop took it. `None` when the listener is gone.
    pub async fn take_request(&self) -> Option<i32> {
        let signal = self.received.recv().await.ok()?;
        self.shared.mark_taken(signal);
        Some(signal)
    }

    /// Stops acting on termination requests once the application has stopped
    /// running: nothing is left to save, so a first signal during teardown
    /// ends the process as it did before START-5, including one still waiting
    /// unread. A repeat of a request already handled keeps its grace, so it
    /// cannot cut the teardown short.
    pub fn release(&self) {
        self.shared.release();
        if let Ok(signal) = self.received.try_recv() {
            end_process(signal);
        }
    }

    /// The last thing the process does, after the application has run and been
    /// torn down: when a termination request was handled, end the process by
    /// that signal's default action so its exit status says so. A normal exit
    /// returns.
    pub fn finish(&self) {
        end_as_handled(&self.shared, end_process);
    }
}

/// Registers the signal handlers and the listener thread, once per process.
/// Before this runs, a termination request keeps its default disposition. A
/// signal the process inherited as ignored stays ignored. Returns `None` when
/// a relay is already running or when nothing was armed.
pub fn start() -> io::Result<Option<&'static Relay>> {
    start_once(&RELAY, register)
}

/// The relay [`start`] registered, if it has.
pub fn running() -> Option<&'static Relay> {
    RELAY.get()
}

/// A relay fed by `source` instead of by real signals, ending the process
/// through `end_process` instead of by signal. The seam a frontend's tests use
/// to drive its main-loop half without raising a signal in the test binary.
pub fn spawn_relay(
    source: impl FnOnce(&mut dyn FnMut(i32)) + Send + 'static,
    end_process: impl Fn(i32) + Send + Sync + 'static,
    wedge_limit: Duration,
) -> io::Result<(Relay, JoinHandle<()>)> {
    let shared = Arc::new(Shared::default());
    let (sender, received) = async_channel::bounded(1);
    let listener = spawn(source, shared.clone(), sender, end_process, wedge_limit)?;
    Ok((Relay { shared, received }, listener))
}

/// Installs the signal handlers and the listener thread.
fn register() -> io::Result<Option<Relay>> {
    let signals = armed(&TERMINATION_SIGNALS, crate::signals::signal_is_ignored);
    if signals.is_empty() {
        tracing::info!("every termination signal was inherited as ignored; leaving them ignored");
        return Ok(None);
    }
    let mut signals = Signals::new(signals)?;
    let (relay, _listener) = spawn_relay(
        move |deliver| signals.forever().for_each(deliver),
        end_process,
        WEDGE_LIMIT,
    )?;
    Ok(Some(relay))
}

/// Ends the process the way `signal` normally would.
fn end_process(signal: i32) {
    if let Err(error) = emulate_default_handler(signal) {
        tracing::error!(%error, signal, "could not end the process on a termination signal");
    }
}

#[cfg(test)]
#[path = "termination_tests.rs"]
mod tests;
