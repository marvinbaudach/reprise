use std::cell::{Cell, RefCell};

use reprise_core::spectrogram_backfill::BackfillStatus;

use super::*;

/// A run the test drives by hand: it reports what it is told to report and
/// finishes when it is told to finish.
#[derive(Default)]
struct FakeRun {
    queued: RefCell<Vec<BackfillProgress>>,
    finished: Cell<bool>,
    summary: RefCell<Option<BackfillSummary>>,
    cancels: Rc<Cell<usize>>,
}

impl BackfillRun for FakeRun {
    fn drain_progress(&self) -> Vec<BackfillProgress> {
        self.queued.borrow_mut().drain(..).collect()
    }

    fn is_finished(&self) -> bool {
        self.finished.get()
    }

    fn cancel(&self) {
        self.cancels.set(self.cancels.get() + 1);
    }

    fn finish(self: Box<Self>) -> Option<BackfillSummary> {
        self.summary.borrow_mut().take()
    }
}

fn progress(completed: usize, total: usize) -> BackfillProgress {
    BackfillProgress {
        completed,
        total,
        track_id: completed as i64,
    }
}

fn summary(status: BackfillStatus, stored: usize, failed: usize) -> BackfillSummary {
    BackfillSummary {
        status,
        stored,
        failed,
        source_changed: 0,
    }
}

/// Builds a batch over one prepared run and hands back both, plus the counter
/// its `cancel` increments.
fn batch_over(run: FakeRun) -> (Rc<SpectrogramBatch>, Rc<Cell<usize>>) {
    let cancels = run.cancels.clone();
    let run = RefCell::new(Some(run));
    let db = Rc::new(crate::test_db::open().unwrap());
    let batch = SpectrogramBatch::new(db, move || {
        run.borrow_mut()
            .take()
            .map(|run| Box::new(run) as Box<dyn BackfillRun>)
    });
    (batch, cancels)
}

#[test]
fn a_reported_bucket_moves_the_card_without_ending_the_run() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let run = FakeRun::default();
    run.queued.borrow_mut().push(progress(1, 4));
    run.queued.borrow_mut().push(progress(2, 4));
    let (batch, _) = batch_over(run);
    batch.start();

    assert_eq!(batch.poll(), glib::ControlFlow::Continue);

    let progress = batch.progress.get();
    assert_eq!(progress.state, SpectrogramBatchState::Running);
    assert_eq!((progress.analyzed, progress.total), (2, 4));
    assert!((progress.fraction() - 0.5).abs() < f64::EPSILON);
}

#[test]
fn a_completed_run_settles_on_its_summary_and_stops_polling() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let run = FakeRun::default();
    run.queued.borrow_mut().push(progress(4, 4));
    run.finished.set(true);
    *run.summary.borrow_mut() = Some(summary(BackfillStatus::Completed, 3, 1));
    let (batch, _) = batch_over(run);
    batch.start();

    assert_eq!(batch.poll(), glib::ControlFlow::Break);

    let progress = batch.progress.get();
    assert_eq!(progress.state, SpectrogramBatchState::Complete);
    assert_eq!((progress.analyzed, progress.failed), (3, 1));
    assert!(!batch.is_running());
}

#[test]
fn a_stopped_run_is_reported_as_stopped_not_as_complete() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let run = FakeRun::default();
    run.finished.set(true);
    *run.summary.borrow_mut() = Some(summary(BackfillStatus::Cancelled, 2, 0));
    let (batch, _) = batch_over(run);
    batch.start();

    batch.poll();

    assert_eq!(batch.progress.get().state, SpectrogramBatchState::Stopped);
}

#[test]
fn a_worker_that_vanishes_without_a_summary_counts_as_failed() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let run = FakeRun::default();
    run.finished.set(true);
    let (batch, _) = batch_over(run);
    batch.start();

    batch.poll();

    assert_eq!(batch.progress.get().state, SpectrogramBatchState::Failed);
}

#[test]
fn a_run_that_cannot_be_launched_fails_instead_of_reporting_progress() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let batch = SpectrogramBatch::new(Rc::new(crate::test_db::open().unwrap()), || None);

    batch.start();

    assert_eq!(batch.progress.get().state, SpectrogramBatchState::Failed);
    assert!(!batch.is_running());
}

#[test]
fn nav_15_a_started_run_can_still_be_cancelled_from_its_progress_card() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let (batch, cancels) = batch_over(FakeRun::default());

    batch.start();
    assert!(batch.is_running());
    assert_eq!(cancels.get(), 0);

    batch.cancel();
    assert_eq!(cancels.get(), 1);
}

#[test]
fn nav_15_a_second_start_never_opens_a_second_run() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let launches = Rc::new(Cell::new(0));
    let batch = SpectrogramBatch::new(Rc::new(crate::test_db::open().unwrap()), {
        let launches = launches.clone();
        move || {
            launches.set(launches.get() + 1);
            Some(Box::new(FakeRun::default()) as Box<dyn BackfillRun>)
        }
    });

    batch.start();
    batch.start();
    batch.start();

    assert_eq!(launches.get(), 1);
}

#[test]
fn an_unchanged_library_never_launches_the_spectrogram_worker_twice() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let db = Rc::new(crate::test_db::open().unwrap());
    reprise_core::library::startup_tasks::record_completed_at(
        &db,
        reprise_core::library::startup_tasks::SignatureTask::Spectrogram,
        123,
    )
    .unwrap();
    let launches = Rc::new(Cell::new(0));
    let batch = SpectrogramBatch::new(db, {
        let launches = launches.clone();
        move || {
            launches.set(launches.get() + 1);
            Some(Box::new(FakeRun::default()) as Box<dyn BackfillRun>)
        }
    });

    batch.start();

    assert_eq!(launches.get(), 0);
    assert_eq!(batch.progress.get().state, SpectrogramBatchState::Complete);
}

#[test]
fn subscribers_see_every_step_of_a_run() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let run = FakeRun::default();
    run.queued.borrow_mut().push(progress(1, 2));
    let (batch, _) = batch_over(run);
    batch.subscribe_progress(|| true, {
        let seen = seen.clone();
        move |progress| seen.borrow_mut().push(progress.state)
    });

    batch.start();
    batch.poll();

    assert_eq!(
        *seen.borrow(),
        vec![
            SpectrogramBatchState::Idle,
            SpectrogramBatchState::Running,
            SpectrogramBatchState::Running,
        ]
    );
}
