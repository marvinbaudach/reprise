//! Test-only serialization of the *global* GLib main context.
//!
//! `cargo test` runs the test functions of one binary on a thread pool, but
//! `MainContext::default()` can only ever be owned by a single thread:
//! `iteration()`, `block_on()` and `MainLoop::run()` all acquire it and panic
//! with "default main context already acquired by another thread" when a
//! sibling test already holds it. That failure is a scheduling artefact — the
//! affected tests pass in isolation and flip red purely because an unrelated
//! edit reshuffles the run order.
//!
//! Every test that pumps the global context therefore takes this guard as its
//! first statement and holds it for the whole test body. Pump helpers
//! (`wait_for_layout`, `pump_ms`, …) deliberately do *not* lock: the guard is
//! not reentrant, so locking in both places would deadlock.
//!
//! A mutex cannot cover one related failure mode, so do not expect it to:
//! `gtk4::init()` acquires the default context and *leaks* the guard
//! (gtk-rs-core#186), pinning ownership to one libtest thread for the rest of
//! the process. Tests that call it must therefore stay `#[ignore]`d display
//! tests, which `scripts/check-display-tests.sh` runs one per process.
//!
//! The shape mirrors `AUDIO_SINK_TEST_LOCK` in
//! `reprise-platform-linux/src/player/tests.rs`, which already serializes that
//! crate's own main-context pumping tests.

use std::sync::{Mutex, MutexGuard, PoisonError};

static MAIN_CONTEXT_LOCK: Mutex<()> = Mutex::new(());

/// Serializes access to the global GLib main context for the calling test.
///
/// Poisoned-recovery, not `.unwrap()`: the lock guards nothing but exclusive
/// ownership of a process-global context, so an earlier panic leaves no state
/// to repair — refusing to run every later test over one unrelated panic would
/// be worse than the poisoning itself.
#[must_use = "the guard must live as long as the test pumps the main context"]
pub(crate) fn lock_main_context() -> MutexGuard<'static, ()> {
    MAIN_CONTEXT_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}
