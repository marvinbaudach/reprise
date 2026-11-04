//! Test seam: lets a test reach the crossfade secondary before it plays.
//!
//! The secondary is built on the crossfade thread, so no test can hold it
//! before its first buffer has moved. This hook runs on that thread with the
//! freshly built secondary, before it leaves `Null`. It is process-global,
//! which is safe because every test that crossfades holds
//! `AUDIO_SINK_TEST_LOCK` for its whole life.

use super::*;

pub(crate) type Hook = Arc<dyn Fn(&gst::Element) + Send + Sync>;

static SECONDARY_BUILT: Mutex<Option<Hook>> = Mutex::new(None);

/// What the secondary the last fade gave up on looked like at that moment.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GaveUp {
    pub(crate) current: gst::State,
    pub(crate) pending: gst::State,
    /// How long it had been given from `Playing` on.
    pub(crate) waited: Duration,
}

static GAVE_UP: Mutex<Option<GaveUp>> = Mutex::new(None);

static BEFORE_RELEASE: Mutex<Option<Arc<dyn Fn() + Send + Sync>>> = Mutex::new(None);

/// Removes the before-release hook again when dropped, panic or not.
pub(crate) struct ReleaseHookGuard;

impl Drop for ReleaseHookGuard {
    fn drop(&mut self) {
        *BEFORE_RELEASE
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;
    }
}

/// Runs `hook` on the crossfade thread at the point where a fade giving way
/// has passed its generation check and not yet released its end-of-stream
/// catch: the window in which a manual `play()` can still land.
pub(crate) fn on_before_release(hook: Arc<dyn Fn() + Send + Sync>) -> ReleaseHookGuard {
    *BEFORE_RELEASE
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = Some(hook);
    ReleaseHookGuard
}

pub(super) fn before_release() {
    let hook = BEFORE_RELEASE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    if let Some(hook) = hook {
        hook();
    }
}

/// Removes the hook again when dropped, panic or not.
pub(crate) struct HookGuard;

impl Drop for HookGuard {
    fn drop(&mut self) {
        *SECONDARY_BUILT
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;
        forget_gave_up();
    }
}

pub(crate) fn on_secondary_built(hook: Hook) -> HookGuard {
    *SECONDARY_BUILT
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = Some(hook);
    HookGuard
}

/// Records what state the secondary was in, and for how long it had been
/// given, when its fade gave way. State alone cannot tell a pipeline that
/// hung on its own from one the fade stopped waiting for too early.
pub(super) fn gave_up(secondary: &gst::Element, waited: Duration) {
    let (_, current, pending) = secondary.state(gst::ClockTime::ZERO);
    *GAVE_UP.lock().unwrap_or_else(PoisonError::into_inner) = Some(GaveUp {
        current,
        pending,
        waited,
    });
}

/// What the last [`gave_up`] recorded, if a fade gave up since.
pub(crate) fn gave_up_in() -> Option<GaveUp> {
    *GAVE_UP.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(crate) fn forget_gave_up() {
    *GAVE_UP.lock().unwrap_or_else(PoisonError::into_inner) = None;
}

pub(super) fn secondary_built(secondary: &gst::Element) {
    let hook = SECONDARY_BUILT
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    if let Some(hook) = hook {
        hook(secondary);
    }
}
