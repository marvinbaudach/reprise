//! GTK runtime adapter for the library-wide rendering-data backfill.
//!
//! The backfill itself lives in `reprise_core::spectrogram_backfill`, its
//! worker thread in the platform crate, and the progress state in
//! `reprise_view::analysis_progress` — shared with the other frontends, which
//! analyze the same library and must not answer "how far along, and is this
//! worth showing" differently. What remains here is the GTK part: a frame-clock
//! poll, and the lifetime of one run inside one window.
//!
//! The run is reached through [`BackfillRun`] rather than by naming the
//! platform handle, per the composition-root rule in `ui/mod.rs`. That seam
//! also lets this be tested without GStreamer or a display.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use reprise_core::db::Db;
use reprise_core::library::startup_tasks::{self, SignatureTask};
use reprise_core::spectrogram_backfill::{BackfillProgress, BackfillSummary};
use reprise_view::analysis_progress::settled;
pub(in crate::ui) use reprise_view::analysis_progress::{
    AnalysisProgress as SpectrogramBatchProgress, AnalysisState as SpectrogramBatchState,
};

use super::progress_subscribers::ProgressSubscribers;

/// How often the adapter drains progress from the worker.
///
/// The worker spends 1.1–1.7 s per track, so a quarter second is far finer
/// than the data it reports; it is chosen to keep the card's motion smooth,
/// and the timer exists only while a run does.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// One backfill run, as much of it as the window needs to see.
pub(in crate::ui) trait BackfillRun {
    fn drain_progress(&self) -> Vec<BackfillProgress>;
    fn is_finished(&self) -> bool;
    fn cancel(&self);
    /// Collects the finished run. Called only after `is_finished`, so it does
    /// not block. `None` means the worker died without a summary.
    fn finish(self: Box<Self>) -> Option<BackfillSummary>;
}

pub(in crate::ui) struct SpectrogramBatch {
    conn: Rc<Db>,
    launch: Box<dyn Fn() -> Option<Box<dyn BackfillRun>>>,
    run: RefCell<Option<Box<dyn BackfillRun>>>,
    pass: Cell<Option<startup_tasks::ExactTaskPass>>,
    progress: Cell<SpectrogramBatchProgress>,
    subscribers: ProgressSubscribers<SpectrogramBatchProgress>,
}

impl SpectrogramBatch {
    pub(in crate::ui) fn new(
        conn: Rc<Db>,
        launch: impl Fn() -> Option<Box<dyn BackfillRun>> + 'static,
    ) -> Rc<Self> {
        Rc::new(Self {
            conn,
            launch: Box::new(launch),
            run: RefCell::new(None),
            pass: Cell::new(None),
            progress: Cell::new(SpectrogramBatchProgress::idle()),
            subscribers: ProgressSubscribers::default(),
        })
    }

    pub(in crate::ui) fn is_running(&self) -> bool {
        self.progress.get().is_running()
    }

    pub(in crate::ui) fn subscribe_progress(
        &self,
        is_alive: impl Fn() -> bool + 'static,
        callback: impl Fn(SpectrogramBatchProgress) + 'static,
    ) {
        self.subscribers
            .subscribe(self.progress.get(), is_alive, callback);
    }

    pub(in crate::ui) fn start(self: &Rc<Self>) {
        if self.is_running() {
            return;
        }
        let Some(pass) = startup_tasks::begin_exact(&self.conn, SignatureTask::Spectrogram) else {
            self.set_progress(SpectrogramBatchProgress {
                state: SpectrogramBatchState::Complete,
                ..SpectrogramBatchProgress::idle()
            });
            return;
        };
        self.pass.set(Some(pass));
        let Some(run) = (self.launch)() else {
            self.pass.set(None);
            tracing::warn!("could not start the library analysis worker");
            self.set_progress(SpectrogramBatchProgress::failed());
            return;
        };
        *self.run.borrow_mut() = Some(run);
        self.set_progress(SpectrogramBatchProgress::running());
        let batch = Rc::downgrade(self);
        glib::timeout_add_local(POLL_INTERVAL, move || {
            let Some(batch) = batch.upgrade() else {
                return glib::ControlFlow::Break;
            };
            batch.poll()
        });
    }

    pub(in crate::ui) fn cancel(&self) {
        if let Some(run) = self.run.borrow().as_ref() {
            run.cancel();
        }
    }

    /// One tick: fold in whatever the worker reported, and settle the run if
    /// it has ended. Split out from the timer closure so it can be driven
    /// directly by a test.
    fn poll(self: &Rc<Self>) -> glib::ControlFlow {
        let mut progress = self.progress.get();
        let finished = {
            let run = self.run.borrow();
            let Some(run) = run.as_ref() else {
                return glib::ControlFlow::Break;
            };
            if let Some(last) = run.drain_progress().last() {
                progress.analyzed = last.completed;
                progress.total = last.total;
            }
            run.is_finished()
        };
        if !finished {
            self.set_progress(progress);
            return glib::ControlFlow::Continue;
        }
        let summary = self.run.borrow_mut().take().and_then(BackfillRun::finish);
        let pass = self.pass.take();
        let completed_cleanly = summary.as_ref().is_some_and(|summary| {
            summary.status == reprise_core::spectrogram_backfill::BackfillStatus::Completed
                && summary.failed == 0
                && summary.source_changed == 0
        });
        let settled = settled(progress, summary);
        tracing::info!(
            state = ?settled.state,
            analyzed = settled.analyzed,
            failed = settled.failed,
            "library analysis finished"
        );
        if completed_cleanly {
            if let Some(pass) = pass {
                pass.record_completed_or_warn(&self.conn);
            }
        }
        self.set_progress(settled);
        glib::ControlFlow::Break
    }

    fn set_progress(&self, progress: SpectrogramBatchProgress) {
        self.progress.set(progress);
        self.subscribers.notify(progress);
    }
}

#[cfg(test)]
#[path = "spectrogram_batch_tests.rs"]
mod tests;
