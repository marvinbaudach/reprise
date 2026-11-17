//! One ordered command queue per pipeline, so no caller ever waits on one.
//!
//! A `playbin3` on a source that is blocked in the kernel (a dead NFS or SMB
//! mount: `filesrc`'s `open()` runs inside `set_state(PAUSED)`, and a `read()`
//! keeps `Null` and a flushing seek waiting) answers none of the calls that
//! change it: a state change, a seek, the final `Null`. Made on the GLib main
//! context, or while holding the playbin lock, each of them stands the whole
//! window still. So the main context never makes them. Every pipeline the player
//! builds is a [`Pipeline`], the element together with a queue of its own, a
//! thread (`reprise-pipeline-queue`) fed by a channel, and the only way to reach
//! the pipeline for anything that can block is to hand that queue a job.
//!
//! - [`Pipeline::submit`] queues a job and returns.
//! - [`Pipeline::ask`] queues a job and returns an [`Answer`], whose
//!   [`Answer::wait`] gives the job's result, or `None` once the bound has
//!   passed. The job runs to its end whether or not anyone still waits for it.
//!
//! Jobs run one at a time in the order they were submitted, which is the point:
//! `gst_element_call_async` runs on an unbounded pool and orders nothing, so it
//! cannot keep a `stop` or a pause behind the start it follows. A job takes no
//! lock of the player.
//!
//! # Retiring
//!
//! A pipeline is taken out of service by [`Pipeline::retire`], which consumes
//! it. Jobs that have not begun are dropped, a job already running finishes,
//! and then the queue takes the element to `Null` and drops it, which is what
//! `reaper.rs` used to do on a thread of its own, now ordered behind the
//! pipeline's own work: a `stop` right after a start can never see that start
//! land after its `Null`. A `Pipeline` dropped without `retire` is retired, and
//! a job submitted after retirement is dropped.
//!
//! Whoever retires a pipeline that has a bus watch removes the watch first, so
//! nothing the pipeline posts while it winds down reaches the frontend.
//!
//! If a thread cannot be spawned the jobs run inline, with a warning: better
//! than a pipeline nobody can start or stop.
//!
//! # The state asked for
//!
//! `get_state` lags behind queued jobs, so each pipeline also records the state
//! last asked of it ([`Asked`]). The record is written synchronously, by the
//! start and by `toggle_pause`, and it is readable from a job: a CUE start that
//! the user paused before its preroll finishes into `Paused`.

use gstreamer as gst;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use super::reaper;

/// Name of the thread that runs one pipeline's jobs.
const QUEUE_THREAD: &str = "reprise-pipeline-queue";

/// A job: runs on the pipeline's own thread, with the element.
type Job = Box<dyn FnOnce(&gst::Element) + Send + 'static>;

enum Message {
    Job(Job),
    Retire,
}

/// The state last asked of a pipeline, which is not the state it is in: the
/// change is queued, and may be stuck behind a source that blocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Asked {
    /// No start has been asked for: a pipeline built for no track.
    Idle,
    Playing,
    Paused,
}

impl Asked {
    const fn as_u8(self) -> u8 {
        match self {
            Self::Idle => 0,
            Self::Playing => 1,
            Self::Paused => 2,
        }
    }

    const fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Playing,
            2 => Self::Paused,
            _ => Self::Idle,
        }
    }
}

/// Shared between a pipeline, its queue thread and every handle to the queue.
struct Shared {
    retired: AtomicBool,
    asked: AtomicU8,
    /// The queue's work is over: the element reached `Null`.
    finished: AtomicBool,
}

#[derive(Clone)]
enum Lane {
    Thread(Sender<Message>),
    /// No thread could be spawned: jobs run on the caller's.
    Inline(gst::Element),
}

/// Whether one pipeline has been taken out of service, for what must not act
/// on behalf of a retired pipeline: a signal handler that still runs on its
/// streaming threads until its `Null` lands. Holds no lane and no element, so
/// a handler that captures one makes no reference cycle with the element it is
/// connected to.
#[derive(Clone)]
pub(crate) struct Retirement(Arc<Shared>);

impl Retirement {
    pub(crate) fn is_retired(&self) -> bool {
        self.0.retired.load(Ordering::SeqCst)
    }
}

/// A clonable way to put jobs on one pipeline's queue, for what has to submit
/// without owning the pipeline: the bus watch, which finishes a CUE start.
/// Holding one never keeps a retired pipeline's queue alive.
#[derive(Clone)]
pub(crate) struct QueueHandle {
    lane: Lane,
    shared: Arc<Shared>,
}

impl QueueHandle {
    /// Queues `job` and returns. Dropped if the pipeline has been retired.
    pub(crate) fn submit(&self, job: impl FnOnce(&gst::Element) + Send + 'static) {
        if self.shared.retired.load(Ordering::SeqCst) {
            return;
        }
        match &self.lane {
            Lane::Thread(sender) => {
                // A closed channel is a retired pipeline: the job is dropped.
                let _ = sender.send(Message::Job(Box::new(job)));
            }
            Lane::Inline(element) => job(element),
        }
    }

    /// Queues `job` and returns a handle on its result.
    pub(crate) fn ask<T: Send + 'static>(
        &self,
        job: impl FnOnce(&gst::Element) -> T + Send + 'static,
    ) -> Answer<T> {
        let (answer, result) = channel();
        self.submit(move |element| {
            let _ = answer.send(job(element));
        });
        Answer { result }
    }

    /// Whether the pipeline has been retired: jobs submitted from now on are
    /// dropped, and a job already running is the last that does anything.
    pub(crate) fn is_retired(&self) -> bool {
        self.shared.retired.load(Ordering::SeqCst)
    }

    /// The state last asked of the pipeline.
    pub(crate) fn asked(&self) -> Asked {
        Asked::from_u8(self.shared.asked.load(Ordering::SeqCst))
    }

    pub(crate) fn set_asked(&self, asked: Asked) {
        self.shared.asked.store(asked.as_u8(), Ordering::SeqCst);
    }

    /// Whether the queue has taken its element to `Null` and ended.
    #[cfg(test)]
    pub(crate) fn is_finished(&self) -> bool {
        self.shared.finished.load(Ordering::SeqCst)
    }
}

/// The result of a job that was asked for, which may never come.
pub(crate) struct Answer<T> {
    result: Receiver<T>,
}

impl<T> Answer<T> {
    /// The job's result, or `None` if it did not finish within `bound` or was
    /// dropped unrun by a retirement.
    pub(crate) fn wait(self, bound: Duration) -> Option<T> {
        self.result.recv_timeout(bound).ok()
    }
}

/// A `playbin3` element and the queue every change to it goes through.
pub(crate) struct Pipeline {
    element: gst::Element,
    queue: QueueHandle,
}

impl Pipeline {
    pub(crate) fn new(element: gst::Element) -> Self {
        let shared = Arc::new(Shared {
            retired: AtomicBool::new(false),
            asked: AtomicU8::new(Asked::Idle.as_u8()),
            finished: AtomicBool::new(false),
        });
        let (sender, messages) = channel::<Message>();
        let owned = element.clone();
        let owned_shared = shared.clone();
        let spawned = std::thread::Builder::new()
            .name(QUEUE_THREAD.into())
            .spawn(move || run(&owned, &messages, &owned_shared));
        let lane = match spawned {
            Ok(_) => Lane::Thread(sender),
            Err(error) => {
                tracing::warn!(%error, "could not spawn a pipeline queue; its jobs run inline");
                Lane::Inline(element.clone())
            }
        };
        Self {
            element,
            queue: QueueHandle { lane, shared },
        }
    }

    /// The element, for what only reads or sets a property of it. A state
    /// change or a seek goes through the queue instead.
    pub(crate) fn element(&self) -> &gst::Element {
        &self.element
    }

    pub(crate) fn queue(&self) -> QueueHandle {
        self.queue.clone()
    }

    pub(crate) fn retirement(&self) -> Retirement {
        Retirement(self.queue.shared.clone())
    }

    pub(crate) fn submit(&self, job: impl FnOnce(&gst::Element) + Send + 'static) {
        self.queue.submit(job);
    }

    pub(crate) fn ask<T: Send + 'static>(
        &self,
        job: impl FnOnce(&gst::Element) -> T + Send + 'static,
    ) -> Answer<T> {
        self.queue.ask(job)
    }

    pub(crate) fn asked(&self) -> Asked {
        self.queue.asked()
    }

    pub(crate) fn set_asked(&self, asked: Asked) {
        self.queue.set_asked(asked);
    }

    /// Takes the pipeline out of service, as described in the module comment.
    /// Returns at once.
    pub(crate) fn retire(self) {
        drop(self);
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        if self.queue.shared.retired.swap(true, Ordering::SeqCst) {
            return;
        }
        #[cfg(test)]
        reaper::in_flight::enter();
        match &self.queue.lane {
            Lane::Thread(sender) => {
                if sender.send(Message::Retire).is_err() {
                    // The thread is gone (a job panicked past its guard):
                    // nothing else will take the element down.
                    reaper::bring_to_null(self.element.clone());
                    self.queue.shared.finished.store(true, Ordering::SeqCst);
                    #[cfg(test)]
                    reaper::in_flight::leave();
                }
            }
            Lane::Inline(element) => {
                reaper::bring_to_null(element.clone());
                self.queue.shared.finished.store(true, Ordering::SeqCst);
                #[cfg(test)]
                reaper::in_flight::leave();
            }
        }
    }
}

/// The queue thread: runs jobs in order until the pipeline is retired, then
/// takes the element down.
fn run(element: &gst::Element, messages: &Receiver<Message>, shared: &Shared) {
    while let Ok(Message::Job(job)) = messages.recv() {
        if shared.retired.load(Ordering::SeqCst) {
            continue;
        }
        if catch_unwind(AssertUnwindSafe(|| job(element))).is_err() {
            tracing::error!("a pipeline queue job panicked");
        }
    }
    reaper::bring_to_null(element.clone());
    shared.finished.store(true, Ordering::SeqCst);
    #[cfg(test)]
    reaper::in_flight::leave();
}

#[cfg(test)]
mod tests {
    use super::*;
    use gstreamer::prelude::*;
    use std::sync::mpsc::sync_channel;
    use std::sync::Mutex;
    use std::time::Instant;

    const PATIENCE: Duration = Duration::from_secs(10);

    fn element() -> gst::Element {
        gst::init().unwrap();
        gst::ElementFactory::make("fakesink").build().unwrap()
    }

    fn finished_soon(queue: &QueueHandle) -> bool {
        let began = Instant::now();
        while !queue.is_finished() && began.elapsed() < PATIENCE {
            std::thread::sleep(Duration::from_millis(5));
        }
        queue.is_finished()
    }

    #[test]
    fn jobs_run_in_the_order_they_were_submitted() {
        let pipeline = Pipeline::new(element());
        let ran = Arc::new(Mutex::new(Vec::new()));
        for number in 0..200 {
            let ran = ran.clone();
            pipeline.submit(move |_| ran.lock().unwrap().push(number));
        }
        let last = pipeline.ask(|_| ());
        assert!(last.wait(PATIENCE).is_some());
        assert_eq!(*ran.lock().unwrap(), (0..200).collect::<Vec<_>>());
    }

    #[test]
    fn an_answer_gives_the_result_and_none_when_the_job_is_still_running() {
        let pipeline = Pipeline::new(element());
        assert_eq!(pipeline.ask(|_| 7).wait(PATIENCE), Some(7));

        let (release, released) = sync_channel::<()>(0);
        let blocked = pipeline.ask(move |_| {
            let _ = released.recv();
            9
        });
        assert_eq!(blocked.wait(Duration::from_millis(50)), None);
        let _ = release.send(());
    }

    #[test]
    fn a_job_runs_to_its_end_after_nobody_waits_for_it_any_more() {
        let pipeline = Pipeline::new(element());
        let (release, released) = sync_channel::<()>(0);
        let ran = Arc::new(AtomicBool::new(false));
        let flag = ran.clone();
        let late = pipeline.ask(move |_| {
            let _ = released.recv();
            flag.store(true, Ordering::SeqCst);
        });
        assert!(late.wait(Duration::from_millis(30)).is_none());
        let _ = release.send(());
        assert!(pipeline.ask(|_| ()).wait(PATIENCE).is_some());
        assert!(ran.load(Ordering::SeqCst));
    }

    #[test]
    fn retiring_drops_the_jobs_that_have_not_begun_and_lets_the_running_one_finish() {
        let element = element();
        let pipeline = Pipeline::new(element.clone());
        let queue = pipeline.queue();
        let (started, running) = sync_channel::<()>(0);
        let (release, released) = sync_channel::<()>(0);
        let finished = Arc::new(AtomicBool::new(false));
        let flag = finished.clone();
        pipeline.submit(move |_| {
            let _ = started.send(());
            let _ = released.recv();
            flag.store(true, Ordering::SeqCst);
        });
        let ran_later = Arc::new(AtomicBool::new(false));
        let flag = ran_later.clone();
        pipeline.submit(move |_| flag.store(true, Ordering::SeqCst));
        running.recv_timeout(PATIENCE).unwrap();

        pipeline.retire();
        let _ = release.send(());

        assert!(finished_soon(&queue));
        assert!(
            finished.load(Ordering::SeqCst),
            "the running job is let finish"
        );
        assert!(
            !ran_later.load(Ordering::SeqCst),
            "a job that had not begun must be dropped"
        );
    }

    #[test]
    fn null_follows_the_job_that_was_running_and_only_then() {
        let element = element();
        let pipeline = Pipeline::new(element.clone());
        let queue = pipeline.queue();
        let (started, running) = sync_channel::<()>(0);
        let (release, released) = sync_channel::<()>(0);
        pipeline.submit(move |element| {
            let _ = element.set_state(gst::State::Playing);
            let _ = started.send(());
            let _ = released.recv();
        });
        running.recv_timeout(PATIENCE).unwrap();

        pipeline.retire();
        std::thread::sleep(Duration::from_millis(100));
        assert!(
            !queue.is_finished(),
            "Null must wait for the running job, not overtake it"
        );
        assert_ne!(element.current_state(), gst::State::Null);

        let _ = release.send(());
        assert!(finished_soon(&queue));
        assert_eq!(element.current_state(), gst::State::Null);
    }

    #[test]
    fn a_job_submitted_after_retirement_is_dropped() {
        let pipeline = Pipeline::new(element());
        let queue = pipeline.queue();
        pipeline.retire();
        assert!(finished_soon(&queue));

        let ran = Arc::new(AtomicBool::new(false));
        let flag = ran.clone();
        let answer = queue.ask(move |_| flag.store(true, Ordering::SeqCst));
        assert!(answer.wait(Duration::from_millis(100)).is_none());
        assert!(!ran.load(Ordering::SeqCst));
    }

    #[test]
    fn a_pipeline_dropped_without_retire_is_retired() {
        let element = element();
        let pipeline = Pipeline::new(element.clone());
        let queue = pipeline.queue();
        pipeline
            .ask(|element| element.set_state(gst::State::Paused))
            .wait(PATIENCE)
            .unwrap()
            .unwrap();

        drop(pipeline);

        assert!(finished_soon(&queue));
        assert_eq!(element.current_state(), gst::State::Null);
    }

    #[test]
    fn a_handle_that_outlives_the_pipeline_does_not_keep_the_thread_alive() {
        let pipeline = Pipeline::new(element());
        let handle = pipeline.queue();
        let also = handle.clone();
        pipeline.retire();
        assert!(finished_soon(&handle));
        drop(also);
    }

    #[test]
    fn the_state_asked_for_is_kept_apart_from_the_state_the_element_is_in() {
        let pipeline = Pipeline::new(element());
        assert_eq!(pipeline.asked(), Asked::Idle);
        pipeline.set_asked(Asked::Paused);
        assert_eq!(pipeline.queue().asked(), Asked::Paused);
        pipeline.queue().set_asked(Asked::Playing);
        assert_eq!(pipeline.asked(), Asked::Playing);
    }
}
