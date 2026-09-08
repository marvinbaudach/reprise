//! One process-global `tracing` subscriber that routes each thread's events
//! into that thread's own buffer.
//!
//! A test that installs its own subscriber with
//! `tracing::subscriber::with_default` shares `tracing`'s process-global
//! callsite interest cache with every other test thread. That cache is written
//! once per callsite and never re-read: if another thread reaches the same
//! callsite while no subscriber is visible *from that thread*, the callsite is
//! cached as `Interest::never()` and the capturing test — and every later one
//! in the process — sees an empty buffer. Keeping one subscriber registered
//! for the whole binary removes the fallback that poisons it, so interest is
//! always resolved against a real subscriber.
//!
//! `reprise-core`'s `log_capture` solves the same problem the same way.

use std::cell::RefCell;
use std::io;
use std::sync::{Arc, Mutex, OnceLock};

static INSTALL_CAPTURE: OnceLock<()> = OnceLock::new();

thread_local! {
    static ACTIVE_CAPTURE: RefCell<Option<CapturedLogs>> = const { RefCell::new(None) };
}

/// The formatted log lines one assertion scope saw on its own thread.
#[derive(Clone, Default)]
pub(crate) struct CapturedLogs(Arc<Mutex<Vec<u8>>>);

impl CapturedLogs {
    /// Runs one assertion scope with this thread's events routed here.
    pub(crate) fn capture<T>(&self, operation: impl FnOnce() -> T) -> T {
        install();
        let guard = CaptureGuard::install(self.clone());
        let result = operation();
        drop(guard);
        result
    }

    /// Everything the scope captured, as the subscriber formatted it.
    pub(crate) fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).expect("log output is utf-8")
    }
}

fn install() {
    INSTALL_CAPTURE.get_or_init(|| {
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_target(true)
            .with_writer(ThreadLocalWriter)
            .finish();
        // `set_global_default` builds a `Dispatch` on the way in, which walks
        // every callsite registered so far and re-resolves its interest against
        // the subscriber installed here. A callsite some thread had already
        // resolved against no subscriber is repaired by that walk.
        tracing::subscriber::set_global_default(subscriber)
            .expect("the GNOME test log capture owns the global subscriber");
    });
}

struct CaptureGuard(Option<CapturedLogs>);

impl CaptureGuard {
    fn install(logs: CapturedLogs) -> Self {
        Self(ACTIVE_CAPTURE.with(|slot| slot.replace(Some(logs))))
    }
}

impl Drop for CaptureGuard {
    fn drop(&mut self) {
        ACTIVE_CAPTURE.with(|slot| {
            slot.replace(self.0.take());
        });
    }
}

/// Hands the subscriber the buffer of whichever thread is emitting, and
/// discards events from threads that are not capturing.
struct ThreadLocalWriter;

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for ThreadLocalWriter {
    type Writer = CaptureSink;

    fn make_writer(&'a self) -> Self::Writer {
        CaptureSink(ACTIVE_CAPTURE.with(|slot| slot.borrow().as_ref().map(|logs| logs.0.clone())))
    }
}

struct CaptureSink(Option<Arc<Mutex<Vec<u8>>>>);

impl io::Write for CaptureSink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Some(buffer) = self.0.as_ref() {
            buffer.lock().unwrap().extend_from_slice(bytes);
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::CapturedLogs;

    /// This callsite lives here and nowhere else, so no other test in the
    /// binary can have resolved its interest first.
    fn probe() {
        tracing::info!(target: "reprise::test_log_capture", probe = 1, "probe emitted");
    }

    /// The interleaving that used to empty a capture buffer: a thread with no
    /// subscriber of its own reaches the callsite first and caches
    /// `Interest::never()` for the whole process. Break `install` and this
    /// goes red.
    #[test]
    fn a_bare_emit_from_another_thread_does_not_silence_the_capture() {
        // One live `Dispatch` is what makes `tracing` take its `has_just_one`
        // shortcut and resolve a callsite against whichever thread touches it
        // first. Every test in this crate that installs its own subscriber
        // creates one, so this is the binary's ordinary state — pinned here so
        // the guard does not depend on a sibling's scheduling.
        let _elsewhere = tracing::Dispatch::new(tracing::subscriber::NoSubscriber::default());
        std::thread::spawn(probe).join().unwrap();

        let logs = CapturedLogs::default();
        logs.capture(probe);

        assert!(
            logs.text().contains("probe emitted"),
            "the capture must survive a callsite another thread reached first, saw {:?}",
            logs.text()
        );
    }

    /// Events from a thread that is not capturing must not land in another
    /// thread's buffer.
    #[test]
    fn a_thread_without_a_capture_scope_writes_nowhere() {
        let logs = CapturedLogs::default();
        logs.capture(|| std::thread::spawn(probe).join().unwrap());

        assert_eq!(logs.text(), "", "only the capturing thread's events count");
    }
}
