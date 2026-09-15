//! Durable, non-blocking database persistence for Android's playback queue.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex, TryLockError};
use std::thread::JoinHandle;
use std::time::Duration;
use std::{io, path::Path};

use reprise_core::db::Db;
use reprise_core::queue::Queue;

use crate::playback_session::queue_persistence;
use crate::queue_snapshot_file::QueueSnapshotFile;

const RETRY_BACKOFFS: [Duration; 3] = [
    Duration::from_millis(250),
    Duration::from_millis(500),
    Duration::from_millis(1_000),
];
const RETRY_WAKEUP: Duration = Duration::from_secs(1);
#[cfg(test)]
const FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
struct PendingSnapshot {
    sequence: u64,
    queue: Queue,
}

#[derive(Default)]
struct FlushState {
    committed_sequence: u64,
    worker_alive: bool,
    #[cfg(test)]
    attempted_sequence: u64,
    #[cfg(test)]
    successful_commits: usize,
}

pub(crate) struct QueuePersister {
    pending: Option<Sender<PendingSnapshot>>,
    shutting_down: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    next_sequence: AtomicU64,
    persist_serial: Mutex<()>,
    snapshot_file: QueueSnapshotFile,
    #[cfg(test)]
    flush_state: Arc<(Mutex<FlushState>, Condvar)>,
}

impl QueuePersister {
    pub(crate) fn spawn(
        database_path: &Path,
        writer: Arc<Mutex<Db>>,
        restored_sequence: Option<u64>,
    ) -> io::Result<Self> {
        let snapshot_file = QueueSnapshotFile::new(database_path)?;
        let next_sequence = restored_sequence.map_or(1, |sequence| sequence.saturating_add(1));
        let worker_file = QueueSnapshotFile::new(database_path)?;
        let (pending, queued) = mpsc::channel();
        let shutting_down = Arc::new(AtomicBool::new(false));
        let worker_flag = Arc::clone(&shutting_down);
        let flush_state = Arc::new((
            Mutex::new(FlushState {
                committed_sequence: 0,
                worker_alive: true,
                #[cfg(test)]
                attempted_sequence: 0,
                #[cfg(test)]
                successful_commits: 0,
            }),
            Condvar::new(),
        ));
        let worker_flush_state = Arc::clone(&flush_state);
        let worker = std::thread::Builder::new()
            .name("reprise-android-queue".to_owned())
            .spawn(move || {
                let _worker_life = WorkerLife::new(Arc::clone(&worker_flush_state));
                drain_snapshots(
                    &writer,
                    &worker_file,
                    &queued,
                    &worker_flag,
                    &worker_flush_state,
                );
            });
        let (pending, worker) = match worker {
            Ok(worker) => (Some(pending), Some(worker)),
            Err(error) => {
                tracing::warn!(
                    %error,
                    "Android queues will stay in their snapshot file: the database worker did not start",
                );
                mark_worker_stopped(&flush_state);
                (None, None)
            }
        };
        Ok(Self {
            pending,
            shutting_down,
            worker,
            next_sequence: AtomicU64::new(next_sequence),
            persist_serial: Mutex::new(()),
            snapshot_file,
            #[cfg(test)]
            flush_state,
        })
    }

    pub(crate) fn persist(&self, queue: &Queue) -> io::Result<()> {
        let _serial = self
            .persist_serial
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let sequence = self.next_sequence.fetch_add(1, Ordering::Relaxed);
        self.snapshot_file.write(sequence, queue)?;
        if let Some(pending) = self.pending.as_ref() {
            if let Err(error) = pending.send(PendingSnapshot {
                sequence,
                queue: queue.clone(),
            }) {
                tracing::warn!(
                    %error,
                    sequence,
                    "kept an Android queue snapshot after its database worker stopped",
                );
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn flush(&self) {
        let target = {
            let _serial = self
                .persist_serial
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            self.next_sequence.load(Ordering::Relaxed).saturating_sub(1)
        };
        let (state, changed) = &*self.flush_state;
        let mut state = state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let deadline = std::time::Instant::now() + FLUSH_TIMEOUT;
        while state.worker_alive && state.committed_sequence < target {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            assert!(
                !remaining.is_zero(),
                "Android queue flush timed out waiting for sequence {target}; last committed was {}",
                state.committed_sequence,
            );
            let (next_state, wait) = changed
                .wait_timeout(state, remaining)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state = next_state;
            assert!(
                !wait.timed_out()
                    || !state.worker_alive
                    || state.committed_sequence >= target,
                "Android queue flush timed out waiting for sequence {target}; last committed was {}",
                state.committed_sequence,
            );
        }
    }

    #[cfg(test)]
    pub(crate) fn wait_until_worker_attempts(&self, sequence: u64) {
        let (state, changed) = &*self.flush_state;
        let mut state = state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let deadline = std::time::Instant::now() + FLUSH_TIMEOUT;
        while state.worker_alive && state.attempted_sequence < sequence {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            assert!(
                !remaining.is_zero(),
                "Android queue worker did not attempt sequence {sequence} before the test timeout",
            );
            let (next_state, wait) = changed
                .wait_timeout(state, remaining)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state = next_state;
            assert!(
                !wait.timed_out() || !state.worker_alive || state.attempted_sequence >= sequence,
                "Android queue worker did not attempt sequence {sequence} before the test timeout",
            );
        }
        assert!(
            state.attempted_sequence >= sequence,
            "Android queue worker stopped before attempting sequence {sequence}",
        );
    }

    #[cfg(test)]
    pub(crate) fn successful_commit_count(&self) -> usize {
        self.flush_state
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .successful_commits
    }
}

impl Drop for QueuePersister {
    fn drop(&mut self) {
        self.shutting_down.store(true, Ordering::Relaxed);
        self.pending = None;
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                tracing::warn!("the Android queue-persistence thread panicked");
            }
        }
    }
}

fn drain_snapshots(
    writer: &Arc<Mutex<Db>>,
    snapshot_file: &QueueSnapshotFile,
    queued: &Receiver<PendingSnapshot>,
    shutting_down: &AtomicBool,
    flush_state: &Arc<(Mutex<FlushState>, Condvar)>,
) {
    let mut latest = None;
    loop {
        if latest.is_none() {
            match queued.recv() {
                Ok(snapshot) => keep_newest(&mut latest, snapshot),
                Err(_) => return,
            }
        }
        take_latest(queued, &mut latest);
        if shutting_down.load(Ordering::Relaxed) {
            return;
        }
        let mut committed = false;
        for wait in RETRY_BACKOFFS {
            match try_commit_latest(writer, &latest, flush_state) {
                CommitOutcome::Committed => {
                    committed = true;
                    break;
                }
                CommitOutcome::RetryableFailure => {}
                CommitOutcome::PermanentFailure => return,
            }
            match queued.recv_timeout(wait) {
                Ok(snapshot) => keep_newest(&mut latest, snapshot),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            take_latest(queued, &mut latest);
            if shutting_down.load(Ordering::Relaxed) {
                return;
            }
        }
        if !committed {
            match try_commit_latest(writer, &latest, flush_state) {
                CommitOutcome::Committed => committed = true,
                CommitOutcome::RetryableFailure => {}
                CommitOutcome::PermanentFailure => return,
            }
        }
        if committed {
            let sequence = latest
                .take()
                .expect("a successful commit consumes the snapshot that was committed")
                .sequence;
            snapshot_file.remove_if_sequence(sequence);
            mark_committed(flush_state, sequence);
            continue;
        }
        match queued.recv_timeout(RETRY_WAKEUP) {
            Ok(snapshot) => keep_newest(&mut latest, snapshot),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn take_latest(queued: &Receiver<PendingSnapshot>, latest: &mut Option<PendingSnapshot>) {
    for snapshot in queued.try_iter() {
        keep_newest(latest, snapshot);
    }
}

fn keep_newest(latest: &mut Option<PendingSnapshot>, candidate: PendingSnapshot) {
    if latest
        .as_ref()
        .is_some_and(|snapshot| snapshot.sequence >= candidate.sequence)
    {
        return;
    }
    *latest = Some(candidate);
}

enum CommitOutcome {
    Committed,
    RetryableFailure,
    PermanentFailure,
}

fn try_commit_latest(
    writer: &Mutex<Db>,
    latest: &Option<PendingSnapshot>,
    flush_state: &Arc<(Mutex<FlushState>, Condvar)>,
) -> CommitOutcome {
    let snapshot = latest
        .as_ref()
        .expect("the worker receives or retains a snapshot before every commit attempt");
    mark_attempted(flush_state, snapshot.sequence);
    try_commit(writer, snapshot)
}

fn try_commit(writer: &Mutex<Db>, snapshot: &PendingSnapshot) -> CommitOutcome {
    let database = match writer.try_lock() {
        Ok(database) => database,
        Err(TryLockError::WouldBlock) => return CommitOutcome::RetryableFailure,
        Err(TryLockError::Poisoned(_)) => {
            tracing::warn!(
                sequence = snapshot.sequence,
                "kept an Android queue snapshot: the shared writer was poisoned",
            );
            return CommitOutcome::PermanentFailure;
        }
    };
    match queue_persistence::save(&database, &snapshot.queue) {
        Ok(()) => CommitOutcome::Committed,
        Err(error) => {
            tracing::warn!(
                %error,
                sequence = snapshot.sequence,
                "kept an Android queue snapshot after a database write failure",
            );
            CommitOutcome::RetryableFailure
        }
    }
}

fn mark_attempted(flush_state: &Arc<(Mutex<FlushState>, Condvar)>, sequence: u64) {
    #[cfg(test)]
    {
        let (state, changed) = &**flush_state;
        let mut state = state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.attempted_sequence = state.attempted_sequence.max(sequence);
        changed.notify_all();
    }
    #[cfg(not(test))]
    let _ = (flush_state, sequence);
}

fn mark_committed(flush_state: &Arc<(Mutex<FlushState>, Condvar)>, sequence: u64) {
    let (state, changed) = &**flush_state;
    let mut state = state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    state.committed_sequence = state.committed_sequence.max(sequence);
    #[cfg(test)]
    {
        state.successful_commits += 1;
    }
    changed.notify_all();
}

fn mark_worker_stopped(flush_state: &Arc<(Mutex<FlushState>, Condvar)>) {
    let (state, changed) = &**flush_state;
    let mut state = state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    state.worker_alive = false;
    changed.notify_all();
}

struct WorkerLife {
    flush_state: Arc<(Mutex<FlushState>, Condvar)>,
}

impl WorkerLife {
    fn new(flush_state: Arc<(Mutex<FlushState>, Condvar)>) -> Self {
        Self { flush_state }
    }
}

impl Drop for WorkerLife {
    fn drop(&mut self) {
        mark_worker_stopped(&self.flush_state);
    }
}
