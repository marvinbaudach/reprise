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

/// Waits until the toplevel holds the global input focus.
///
/// `gtk_widget_has_focus()` is `is_focus() && window.is_active()`, and X
/// delivers the activation asynchronously — measured at ~21 ms under Xvfb. A
/// non-blocking drain returns long before that, so any test that exercises a
/// `has_focus()`-gated code path must wait here first. `iteration(true)`
/// blocks until there is something to dispatch rather than spinning a core.
///
/// Lives here rather than beside one test module because more than one surface
/// needs it: the sidebar's drop targets and the source row's reveal-on-focus
/// rule both fail without it, and in exactly the same misleading way — the
/// widget takes the focus, `has_focus()` says otherwise, and the test reads as
/// a product bug.
pub(crate) fn settle_until_active(window: &gtk4::Window) {
    use gtk4::prelude::*;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while !window.is_active() {
        assert!(
            std::time::Instant::now() < deadline,
            "test window did not become active within 2s; \
             the display server must grant focus for has_focus() assertions"
        );
        gtk4::glib::MainContext::default().iteration(true);
    }
}
