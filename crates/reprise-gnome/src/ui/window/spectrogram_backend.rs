//! Constructs the concrete rendering-data backfill worker and wraps it in the
//! `BackfillRun` seam the UI adapter consumes.
//!
//! Like `player_backends`, this lives in the window layer because that is
//! where `reprise_platform_linux` may be named; `ui/spectrogram` itself stays
//! free of the platform crate so its state machine can be tested without
//! GStreamer.

use std::path::PathBuf;
use std::rc::Rc;

use reprise_core::db::Db;
use reprise_core::spectrogram_backfill::{BackfillProgress, BackfillSummary};
use reprise_platform_linux::spectrogram_backfill::SpectrogramBackfillHandle;

use super::spectrogram_batch::{BackfillRun, SpectrogramBatch};

struct PlatformRun(SpectrogramBackfillHandle);

impl BackfillRun for PlatformRun {
    fn drain_progress(&self) -> Vec<BackfillProgress> {
        self.0.try_progress().collect()
    }

    fn is_finished(&self) -> bool {
        self.0.is_finished()
    }

    fn cancel(&self) {
        self.0.cancel();
    }

    fn finish(self: Box<Self>) -> Option<BackfillSummary> {
        match self.0.join() {
            Ok(summary) => Some(summary),
            Err(error) => {
                tracing::warn!(%error, "the library analysis worker ended without a summary");
                None
            }
        }
    }
}

/// The batch, bound to the worker that this platform can actually run.
pub(in crate::ui) fn build(conn: Rc<Db>, db_path: PathBuf) -> Rc<SpectrogramBatch> {
    SpectrogramBatch::new(conn, move || {
        Some(Box::new(PlatformRun(SpectrogramBackfillHandle::start(
            db_path.clone(),
        ))) as Box<dyn BackfillRun>)
    })
}
