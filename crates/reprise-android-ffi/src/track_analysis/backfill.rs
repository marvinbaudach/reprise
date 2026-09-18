//! The library-wide track-analysis backfill: one track at a time, at
//! background priority, preempted by any foreground request. See decision 5
//! of the mother plan; `ReprisePlaybackService.kt` starts and cancels this
//! while playback runs (A5).

use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;

use reprise_core::db::Db;

use crate::track_analysis::{
    AnalysisContext, AnalysisInFlight, AnalysisPcmSink, AndroidAnalysisOutcome, TrackPcmDecoder,
};
use crate::MusicLibrary;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, uniffi::Record)]
pub struct TrackAnalysisProgress {
    pub done: u32,
    pub total: u32,
    pub failed: u32,
}

#[uniffi::export(callback_interface)]
pub trait TrackAnalysisProgressListener: Send + Sync {
    fn on_progress(&self, progress: TrackAnalysisProgress);
}

type ProgressListener = dyn Fn(TrackAnalysisProgress) + Send + Sync;

struct Shared {
    active: bool,
    cancelled: bool,
    current_track_id: Option<i64>,
    progress: TrackAnalysisProgress,
}

struct Control {
    shared: Mutex<Shared>,
    /// The sink of whichever track is currently decoding, so a foreground
    /// request can cancel it without a second channel back into the worker.
    current_sink: Mutex<Option<Arc<AnalysisPcmSink>>>,
}

/// Every `Arc`-backed handle the worker thread needs, owned by the thread
/// for its whole run rather than borrowed from `MusicLibrary`.
struct Handles {
    reader: Arc<Mutex<Db>>,
    writer: Arc<Mutex<Db>>,
    decoder: Arc<Mutex<Option<Arc<dyn TrackPcmDecoder>>>>,
    in_flight: Arc<AnalysisInFlight>,
    failed: Arc<Mutex<HashSet<i64>>>,
}

impl Handles {
    fn context(&self) -> AnalysisContext<'_> {
        AnalysisContext {
            reader: &self.reader,
            writer: &self.writer,
            decoder: &self.decoder,
            in_flight: &self.in_flight,
            failed: &self.failed,
        }
    }
}

/// Owns the sole backfill worker thread and its latest progress snapshot.
pub struct TrackAnalysisBackfill {
    control: Arc<Control>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl TrackAnalysisBackfill {
    #[must_use]
    pub fn new() -> Self {
        Self {
            control: Arc::new(Control {
                shared: Mutex::new(Shared {
                    active: false,
                    cancelled: false,
                    current_track_id: None,
                    progress: TrackAnalysisProgress::default(),
                }),
                current_sink: Mutex::new(None),
            }),
            worker: Mutex::new(None),
        }
    }

    /// Starts a run. A call while a run is already active is a no-op.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        &self,
        reader: Arc<Mutex<Db>>,
        writer: Arc<Mutex<Db>>,
        decoder: Arc<Mutex<Option<Arc<dyn TrackPcmDecoder>>>>,
        in_flight: Arc<AnalysisInFlight>,
        failed: Arc<Mutex<HashSet<i64>>>,
        listener: Box<dyn TrackAnalysisProgressListener>,
    ) {
        let mut worker = self.worker.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(previous) = worker.take() {
            if previous.is_finished() {
                let _ = previous.join();
            } else {
                *worker = Some(previous);
                return;
            }
        }
        {
            let mut shared = self
                .control
                .shared
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if shared.active {
                *worker = None;
                return;
            }
            shared.active = true;
            shared.cancelled = false;
            shared.current_track_id = None;
            shared.progress = TrackAnalysisProgress::default();
        }

        let control = Arc::clone(&self.control);
        let handles = Handles {
            reader,
            writer,
            decoder,
            in_flight,
            failed,
        };
        let listener: Arc<ProgressListener> =
            Arc::new(move |progress| listener.on_progress(progress));
        *worker = Some(std::thread::spawn(move || {
            run_worker(&control, &handles, &listener)
        }));
    }

    /// Cancels the active run, preempting its current item, and joins the
    /// worker thread. A no-op when no run is active.
    pub fn cancel(&self) {
        {
            let mut shared = self
                .control
                .shared
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if !shared.active {
                return;
            }
            shared.cancelled = true;
        }
        if let Some(sink) = self
            .control
            .current_sink
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            sink.cancel();
        }
        if let Some(handle) = self
            .worker
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            let _ = handle.join();
        }
    }

    #[must_use]
    pub fn progress(&self) -> TrackAnalysisProgress {
        self.control
            .shared
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .progress
    }

    /// Cancels the current item's decode if the backfill is active on a
    /// track other than `keep_track_id`. A no-op when idle or already on
    /// `keep_track_id`.
    pub(crate) fn preempt_current_unless(&self, keep_track_id: i64) {
        let current = {
            let shared = self
                .control
                .shared
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if !shared.active {
                return;
            }
            shared.current_track_id
        };
        if current.is_some() && current != Some(keep_track_id) {
            if let Some(sink) = self
                .control
                .current_sink
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .as_ref()
            {
                sink.cancel();
            }
        }
    }
}

impl Default for TrackAnalysisBackfill {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TrackAnalysisBackfill {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn run_worker(control: &Control, handles: &Handles, listener: &Arc<ProgressListener>) {
    let mut done = 0_u32;
    let mut failed_count = 0_u32;

    loop {
        if is_cancelled(control) {
            break;
        }
        let pending = {
            let reader = handles
                .reader
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            match reprise_core::db::pending_render_data_tracks(&reader) {
                Ok(pending) => pending,
                Err(_) => break,
            }
        };
        let failed_ids = handles
            .failed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        // Recomputed every iteration rather than fixed at the start: a
        // track another caller (a foreground request) finishes while the
        // backfill is on a different item simply disappears from `pending`,
        // and the total shrinks with it rather than the backfill's own
        // counters chasing a number that was never going to be reached.
        let still_pending: Vec<_> = pending
            .into_iter()
            .filter(|track| !failed_ids.contains(&track.track_id))
            .collect();
        let total = done + failed_count + still_pending.len() as u32;

        let Some(track) = still_pending
            .into_iter()
            .find(|track| !handles.in_flight.contains(track.track_id))
        else {
            publish(
                control,
                TrackAnalysisProgress {
                    done,
                    total,
                    failed: failed_count,
                },
                listener,
            );
            break;
        };

        {
            let mut shared = control
                .shared
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if shared.cancelled {
                break;
            }
            shared.current_track_id = Some(track.track_id);
        }

        let outcome =
            handles
                .context()
                .compute(track.track_id, true, None, Some(&control.current_sink));

        {
            let mut shared = control
                .shared
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            shared.current_track_id = None;
        }
        *control
            .current_sink
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;

        match outcome {
            Ok(AndroidAnalysisOutcome::Cancelled) => {
                // Not a failure and not progress: the track stays pending
                // and is picked up again on the next loop iteration.
            }
            Ok(AndroidAnalysisOutcome::DecodeFailed | AndroidAnalysisOutcome::NoDecoder) => {
                failed_count += 1;
            }
            Ok(_) => {
                done += 1;
            }
            Err(_) => {
                failed_count += 1;
            }
        }
        publish(
            control,
            TrackAnalysisProgress {
                done,
                total,
                failed: failed_count,
            },
            listener,
        );
    }

    let mut shared = control
        .shared
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    shared.active = false;
    shared.current_track_id = None;
}

fn is_cancelled(control: &Control) -> bool {
    control
        .shared
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .cancelled
}

fn publish(control: &Control, progress: TrackAnalysisProgress, listener: &Arc<ProgressListener>) {
    {
        let mut shared = control
            .shared
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        shared.progress = progress;
    }
    listener(progress);
}

#[uniffi::export]
impl MusicLibrary {
    pub fn start_track_analysis_backfill(&self, listener: Box<dyn TrackAnalysisProgressListener>) {
        self.analysis_backfill.start(
            self.reader_handle(),
            self.writer_handle(),
            Arc::clone(&self.pcm_decoder),
            Arc::clone(&self.analysis_in_flight),
            Arc::clone(&self.analysis_failed),
            listener,
        );
    }

    pub fn cancel_track_analysis_backfill(&self) {
        self.analysis_backfill.cancel();
    }

    pub fn track_analysis_backfill_progress(&self) -> TrackAnalysisProgress {
        self.analysis_backfill.progress()
    }
}

#[cfg(test)]
#[path = "backfill_tests.rs"]
mod tests;
