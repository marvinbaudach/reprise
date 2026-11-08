//! Takes a retired pipeline down to `Null` off the thread that retired it.
//!
//! `set_state(Null)` waits for the pipeline's streaming threads to stop, and a
//! `filesrc` blocked in a kernel read (a dead NFS/SMB mount, a stalled FIFO)
//! does not stop until the read returns, which can be never. A pipeline that
//! is being replaced because its start hung is exactly such a pipeline, so it
//! is never taken down on the main context or under the playbin lock: both
//! would wait with it. The reaper owns it from the moment it is retired, and
//! takes as long as it takes.
//!
//! The reaper takes no lock of the player and holds nothing it shares.
//!
//! The cost of that is paid on a source that is truly dead. Each attempt of a
//! hung start leaves one reaper thread and one pipeline blocked in `Null` for
//! the rest of the process, so a track that never starts leaves up to
//! `START_ATTEMPTS` of each behind. On an exclusive ALSA device the retired
//! pipeline's sink may keep the device busy for as long as that lasts.

use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Name of the thread that takes one retired pipeline down.
const REAPER_THREAD: &str = "reprise-pipeline-reaper";

/// A `Null` slower than this is worth a warning: something held the pipeline.
const SLOW_NULL: Duration = Duration::from_secs(1);

/// Retires `pipeline`: sets it to `Null` on a thread of its own and drops it
/// there. Returns at once. If no thread can be spawned the same is done
/// inline, which is what the caller was trying to avoid but still better than
/// leaking a pipeline in a running state.
pub(super) fn retire_pipeline(pipeline: gst::Element) {
    // A failed spawn drops its closure, so the pipeline is shared with the
    // fallback through a slot whichever side gets there first empties.
    #[cfg(test)]
    in_flight::enter();
    let slot = Arc::new(Mutex::new(Some(pipeline)));
    let owned = slot.clone();
    let spawned = std::thread::Builder::new()
        .name(REAPER_THREAD.into())
        .spawn(move || reap(&owned));
    if let Err(error) = spawned {
        tracing::warn!(%error, "could not spawn a pipeline reaper; taking the pipeline down inline");
        reap(&slot);
    }
}

/// Takes the pipeline out of `slot`, if it is still there, to `Null`.
fn reap(slot: &Mutex<Option<gst::Element>>) {
    let taken = slot.lock().unwrap_or_else(PoisonError::into_inner).take();
    let Some(pipeline) = taken else {
        #[cfg(test)]
        in_flight::leave();
        return;
    };
    let began = Instant::now();
    let result = pipeline.set_state(gst::State::Null);
    let took = began.elapsed();
    if took > SLOW_NULL {
        tracing::warn!(?result, ?took, "a retired pipeline was slow to reach Null");
    } else {
        tracing::debug!(?result, ?took, "a retired pipeline reached Null");
    }
    drop(pipeline);
    #[cfg(test)]
    in_flight::leave();
}

/// How many retired pipelines have not yet reached `Null`, counted from the
/// moment they are retired, for the tests.
#[cfg(test)]
pub(super) mod in_flight {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNT: AtomicUsize = AtomicUsize::new(0);

    pub(super) fn enter() {
        COUNT.fetch_add(1, Ordering::SeqCst);
    }

    pub(super) fn leave() {
        COUNT.fetch_sub(1, Ordering::SeqCst);
    }

    pub(in crate::player) fn count() -> usize {
        COUNT.load(Ordering::SeqCst)
    }
}
