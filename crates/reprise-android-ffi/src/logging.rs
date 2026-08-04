//! The Android app's one `tracing` sink.
//!
//! Everything this crate reports about an environmental failure — a cover cache
//! it cannot write, a play count that never reached the database — is a
//! `tracing` event, and `tracing` *discards* events while no subscriber is
//! installed. Without this module those lines exist only inside the crate's own
//! tests, which install their own throwaway subscribers; the shipped APK has
//! none, so every one of them is a no-op on a device.
//!
//! ## Why an exported function rather than a constructor
//!
//! The app enters this library through two independent doors:
//! [`crate::MusicLibrary::open`] from the activity, and
//! [`crate::AndroidPlaybackSession::new`] from `ReprisePlaybackService`, which
//! Media3 may start without the activity ever having run. Installing the
//! subscriber inside either constructor leaves the other door's events
//! unlogged — and the play-count warnings, the ones worth having, come through
//! the service's door. Installing it inside *both* duplicates one decision in
//! two places, which is how the two of them drift.
//!
//! `MusicLibrary::open` can also fail, and a subscriber installed after that
//! `?` would miss exactly the failure worth reporting.
//!
//! So the Kotlin app calls [`init_logging`] once from `Application.onCreate`,
//! which Android runs once per process before either door opens.
//!
//! **Called twice** — the second call does nothing. [`Once`] runs the install
//! exactly once, so no second subscriber is installed, no line is emitted
//! twice, and `tracing`'s "a global default has already been set" can never be
//! reached.
//!
//! **Never called** — the crate behaves exactly as it did before this module
//! existed: events are dropped and nothing else changes. No code path here
//! depends on a subscriber being present.

use std::sync::Once;

use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;
use tracing_subscriber::EnvFilter;

/// The `logcat` tag every line from this library carries, so `adb logcat -s
/// Reprise` is the whole filter a device session needs.
#[cfg(target_os = "android")]
const LOGCAT_TAG: &str = "Reprise";

/// The same override the desktop reads (`reprise-gnome`'s `main.rs`), kept
/// identical so one documented variable describes every frontend.
const LOG_FILTER_ENV_VAR: &str = "REPRISE_LOG";

/// This crate's own events run at `debug` by default and everything beneath
/// them at `info`. The artwork path deliberately reports an undecodable cover
/// at `debug` — expected in the wild, useless at `info`, and the only evidence
/// that a missing cover was a decode failure rather than an absent picture.
const DEFAULT_LOG_FILTER: &str = "info,reprise_android_ffi=debug";

static INSTALL: Once = Once::new();

/// Installs this library's `tracing` subscriber for the life of the process.
///
/// Idempotent and infallible by construction: call it from
/// `Application.onCreate` and nowhere else. See the module documentation for
/// what happens when it is called twice or not at all.
#[uniffi::export]
pub fn init_logging() {
    INSTALL.call_once(install);
}

fn log_filter() -> EnvFilter {
    EnvFilter::try_from_env(LOG_FILTER_ENV_VAR)
        .unwrap_or_else(|_| EnvFilter::new(DEFAULT_LOG_FILTER))
}

#[cfg(target_os = "android")]
fn install() {
    let Ok(logcat) = tracing_android::layer(LOGCAT_TAG) else {
        // The reporting channel itself is what failed, so there is nowhere to
        // report it to. Everything else keeps working without a sink, exactly
        // as it did before this module existed.
        return;
    };
    let _ = tracing_subscriber::registry()
        .with(log_filter())
        .with(logcat)
        .try_init();
}

#[cfg(not(target_os = "android"))]
fn install() {
    let _ = tracing_subscriber::registry()
        .with(log_filter())
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(host::HostLog),
        )
        .try_init();
}

/// The sink a host build writes to.
///
/// This crate is an Android library. It builds for a host only so `cargo test`
/// and `cargo clippy` can see it, and no host ever ships it — so rather than
/// invent a second logging policy for a target that does not exist, the
/// non-Android sink is a buffer the crate's own tests read back. That is what
/// makes "the events reach the installed subscriber" something this crate can
/// prove instead of assert.
#[cfg(not(target_os = "android"))]
mod host {
    use std::io::Write;
    use std::sync::Mutex;

    use tracing_subscriber::fmt::MakeWriter;

    static WRITTEN: Mutex<String> = Mutex::new(String::new());

    pub(super) struct HostLog;

    pub(super) struct HostLogWriter;

    impl Write for HostLogWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if let Ok(mut written) = WRITTEN.lock() {
                written.push_str(&String::from_utf8_lossy(buf));
            }
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for HostLog {
        type Writer = HostLogWriter;

        fn make_writer(&'a self) -> Self::Writer {
            HostLogWriter
        }
    }

    #[cfg(test)]
    pub(super) fn written() -> String {
        WRITTEN.lock().map_or_else(
            |poisoned| poisoned.into_inner().clone(),
            |written| written.clone(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{host, init_logging};

    /// A field value no other event in this crate carries, so the assertion
    /// below cannot be satisfied by somebody else's log line.
    const PROBE: &str = "init-logging-probe-2c1f";

    /// The two properties the shipped app depends on, in the one test that can
    /// hold both: after `init_logging` a subscriber exists, and an event
    /// emitted afterwards actually arrives at it — once, however often the app
    /// asked for initialisation.
    #[test]
    fn init_logging_installs_exactly_one_subscriber_however_often_it_is_called() {
        init_logging();
        init_logging();

        assert!(
            tracing::dispatcher::has_been_set(),
            "init_logging must leave a global subscriber behind",
        );

        tracing::warn!(probe = PROBE, "dropped an Android play count");

        let written = host::written();
        assert_eq!(
            written.matches(PROBE).count(),
            1,
            "the event must reach the installed subscriber exactly once, got: {written}",
        );
        assert!(
            written.contains("WARN"),
            "the level must survive the trip to the sink, got: {written}",
        );
    }
}
