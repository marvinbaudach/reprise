//! GTK runtime adapter for the library-wide rendering-data backfill.
//!
//! The backfill itself lives in `reprise_core::spectrogram_backfill` and its
//! worker thread in the platform crate. This adapter owns only the part that
//! is specific to a running window: when a run may start, how its progress
//! reaches the scan card, and what happens when the listener stops it.
//!
//! The run is reached through [`BackfillRun`] rather than by naming the
//! platform handle, per the composition-root rule in `ui/mod.rs`. That seam
//! also lets the state machine below be tested without GStreamer or a display.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use reprise_core::spectrogram_backfill::{BackfillProgress, BackfillStatus, BackfillSummary};

use super::progress_subscribers::ProgressSubscribers;

/// How often the adapter drains progress from the worker.
///
/// The worker spends 1.1–1.7 s per track, so a quarter second is far finer
/// than the data it reports; it is chosen to keep the card's motion smooth,
/// and the timer exists only while a run does.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum SpectrogramBatchState {
    Idle,
    Running,
    Complete,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) struct SpectrogramBatchProgress {
    pub(in crate::ui) state: SpectrogramBatchState,
    pub(in crate::ui) analyzed: usize,
    pub(in crate::ui) total: usize,
    pub(in crate::ui) failed: usize,
}

impl SpectrogramBatchProgress {
    pub(in crate::ui) fn idle() -> Self {
        Self {
            state: SpectrogramBatchState::Idle,
            analyzed: 0,
            total: 0,
            failed: 0,
        }
    }

    pub(in crate::ui) fn fraction(self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        self.analyzed as f64 / self.total as f64
    }
}

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
    launch: Box<dyn Fn() -> Option<Box<dyn BackfillRun>>>,
    run: RefCell<Option<Box<dyn BackfillRun>>>,
    progress: Cell<SpectrogramBatchProgress>,
    subscribers: ProgressSubscribers<SpectrogramBatchProgress>,
}

impl SpectrogramBatch {
    pub(in crate::ui) fn new(
        launch: impl Fn() -> Option<Box<dyn BackfillRun>> + 'static,
    ) -> Rc<Self> {
        Rc::new(Self {
            launch: Box::new(launch),
            run: RefCell::new(None),
            progress: Cell::new(SpectrogramBatchProgress::idle()),
            subscribers: ProgressSubscribers::default(),
        })
    }

    pub(in crate::ui) fn is_running(&self) -> bool {
        self.progress.get().state == SpectrogramBatchState::Running
    }

    pub(in crate::ui) fn subscribe_progress(
        &self,
        is_alive: impl Fn() -> bool + 'static,
        callback: impl Fn(SpectrogramBatchProgress) + 'static,
    ) {
        self.subscribers
            .subscribe(self.progress.get(), is_alive, callback);
    }

    /// Starts a run, or stops the running one. This is the whole of what the
    /// menu item does, so that the item's two labels cannot drift from the
    /// two behaviours. The item is the stop, not the permission: the window
    /// starts a run on launch without going through here.
    pub(in crate::ui) fn toggle(self: &Rc<Self>) {
        if self.is_running() {
            self.cancel();
        } else {
            self.start();
        }
    }

    pub(in crate::ui) fn start(self: &Rc<Self>) {
        if self.is_running() {
            return;
        }
        let Some(run) = (self.launch)() else {
            tracing::warn!("could not start the library analysis worker");
            self.set_progress(SpectrogramBatchProgress {
                state: SpectrogramBatchState::Failed,
                ..SpectrogramBatchProgress::idle()
            });
            return;
        };
        *self.run.borrow_mut() = Some(run);
        self.set_progress(SpectrogramBatchProgress {
            state: SpectrogramBatchState::Running,
            ..SpectrogramBatchProgress::idle()
        });
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
        let settled = settled(progress, summary);
        tracing::info!(
            state = ?settled.state,
            analyzed = settled.analyzed,
            failed = settled.failed,
            "library analysis finished"
        );
        self.set_progress(settled);
        glib::ControlFlow::Break
    }

    fn set_progress(&self, progress: SpectrogramBatchProgress) {
        self.progress.set(progress);
        self.subscribers.notify(progress);
    }
}

/// The closing state of a run, from its summary.
///
/// A worker that vanished without a summary counts as failed rather than
/// complete: reporting a clean finish for a run whose outcome nobody saw is
/// exactly the lie that made the missing backfill hard to notice.
fn settled(
    progress: SpectrogramBatchProgress,
    summary: Option<BackfillSummary>,
) -> SpectrogramBatchProgress {
    let Some(summary) = summary else {
        return SpectrogramBatchProgress {
            state: SpectrogramBatchState::Failed,
            ..progress
        };
    };
    SpectrogramBatchProgress {
        state: match summary.status {
            BackfillStatus::Completed => SpectrogramBatchState::Complete,
            BackfillStatus::Cancelled => SpectrogramBatchState::Stopped,
        },
        analyzed: summary.stored,
        total: progress.total.max(summary.stored),
        failed: summary.failed,
    }
}

#[cfg(test)]
#[path = "spectrogram_batch_tests.rs"]
mod tests;
