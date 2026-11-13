//! The last step of a retired pipeline: taking it down to `Null`.
//!
//! `set_state(Null)` waits for the pipeline's streaming threads to stop, and a
//! `filesrc` blocked in a kernel read (a dead NFS/SMB mount, a stalled FIFO)
//! does not stop until the read returns, which can be never. A pipeline that
//! is being replaced because its start hung is exactly such a pipeline, so it
//! is never taken down on the main context or under the playbin lock: both
//! would wait with it.
//!
//! Who does it is the pipeline's own queue (`pipeline_queue.rs`): a retired
//! pipeline reaches `Null` on the thread that ran its jobs, behind the last of
//! them, so a `Null` can never overtake the start it follows. This module keeps
//! what is left of the old reaper, the `Null` itself with its slow-`Null`
//! warning, and the count of retired pipelines that have not got there yet.
//!
//! The cost of that is paid on a source that is truly dead. Each attempt of a
//! hung start leaves one queue thread and one pipeline blocked in `Null` for
//! the rest of the process, so a track that never starts leaves up to
//! `START_ATTEMPTS` of each behind. On an exclusive ALSA device the retired
//! pipeline's sink may keep the device busy for as long as that lasts.

use gstreamer as gst;
use gstreamer::prelude::*;
use std::time::{Duration, Instant};

/// A `Null` slower than this is worth a warning: something held the pipeline.
const SLOW_NULL: Duration = Duration::from_secs(1);

/// Takes `pipeline` to `Null` and drops it. Blocks for as long as that takes,
/// so only a queue thread calls it.
pub(super) fn bring_to_null(pipeline: gst::Element) {
    let began = Instant::now();
    let result = pipeline.set_state(gst::State::Null);
    let took = began.elapsed();
    if took > SLOW_NULL {
        tracing::warn!(?result, ?took, "a retired pipeline was slow to reach Null");
    } else {
        tracing::debug!(?result, ?took, "a retired pipeline reached Null");
    }
    drop(pipeline);
}

/// How many retired pipelines have not yet reached `Null`, counted from the
/// moment they are retired, for the tests.
#[cfg(test)]
pub(super) mod in_flight {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNT: AtomicUsize = AtomicUsize::new(0);

    pub(in crate::player) fn enter() {
        COUNT.fetch_add(1, Ordering::SeqCst);
    }

    pub(in crate::player) fn leave() {
        COUNT.fetch_sub(1, Ordering::SeqCst);
    }

    pub(in crate::player) fn count() -> usize {
        COUNT.load(Ordering::SeqCst)
    }
}
