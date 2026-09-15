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

#[derive(Clone)]
struct PendingSnapshot {
    sequence: u64,
    queue: Queue,
}

#[derive(Default)]
struct FlushState {
    committed_sequence: u64,
    worker_alive: bool,
}

pub(crate) struct QueuePersister {
    pending: Option<Sender<PendingSnapshot>>,
    shutting_down: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    next_sequence: AtomicU64,
    snapshot_file: QueueSnapshotFile,
    #[cfg(test)]
    flush_state: Arc<(Mutex<FlushState>, Condvar)>,
}

impl QueuePersister {
    pub(crate) fn spawn(database_path: &Path, writer: Arc<Mutex<Db>>) -> io::Result<Self> {
        let snapshot_file = QueueSnapshotFile::new(database_path)?;
        let next_sequence = snapshot_file
            .read()
            .map_or(1, |(sequence, _)| sequence.saturating_add(1));
        let worker_file = QueueSnapshotFile::new(database_path)?;
        let (pending, queued) = mpsc::channel();
        let shutting_down = Arc::new(AtomicBool::new(false));
        let worker_flag = Arc::clone(&shutting_down);
        let flush_state = Arc::new((
            Mutex::new(FlushState {
                committed_sequence: 0,
                worker_alive: true,
            }),
            Condvar::new(),
        ));
        let worker_flush_state = Arc::clone(&flush_state);
        let worker = std::thread::Builder::new()
            .name("reprise-android-queue".to_owned())
            .spawn(move || {
                drain_snapshots(
                    &writer,
                    &worker_file,
                    &queued,
                    &worker_flag,
                    &worker_flush_state,
                );
                mark_worker_stopped(&worker_flush_state);
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
            snapshot_file,
            #[cfg(test)]
            flush_state,
        })
    }

    pub(crate) fn persist(&self, queue: &Queue) -> io::Result<()> {
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
        let target = self.next_sequence.load(Ordering::Relaxed).saturating_sub(1);
        let (state, changed) = &*self.flush_state;
        let mut state = state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while state.worker_alive && state.committed_sequence < target {
            state = changed
                .wait(state)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
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
            latest = match queued.recv() {
                Ok(snapshot) => Some(snapshot),
                Err(_) => return,
            };
        }
        take_latest(queued, &mut latest);
        if shutting_down.load(Ordering::Relaxed) {
            return;
        }
        let mut committed = false;
        for wait in RETRY_BACKOFFS {
            if try_commit(writer, latest.as_ref().expect("latest snapshot exists")) {
                committed = true;
                break;
            }
            match queued.recv_timeout(wait) {
                Ok(snapshot) => {
                    latest = Some(snapshot);
                    take_latest(queued, &mut latest);
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            if shutting_down.load(Ordering::Relaxed) {
                return;
            }
        }
        if !committed {
            committed = try_commit(writer, latest.as_ref().expect("latest snapshot exists"));
        }
        if committed {
            let sequence = latest.as_ref().expect("latest snapshot exists").sequence;
            snapshot_file.remove_if_sequence(sequence);
            mark_committed(flush_state, sequence);
            latest = None;
            continue;
        }
        match queued.recv_timeout(RETRY_WAKEUP) {
            Ok(snapshot) => latest = Some(snapshot),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn take_latest(queued: &Receiver<PendingSnapshot>, latest: &mut Option<PendingSnapshot>) {
    for snapshot in queued.try_iter() {
        *latest = Some(snapshot);
    }
}

fn try_commit(writer: &Mutex<Db>, snapshot: &PendingSnapshot) -> bool {
    let database = match writer.try_lock() {
        Ok(database) => database,
        Err(TryLockError::WouldBlock) => return false,
        Err(TryLockError::Poisoned(_)) => {
            tracing::warn!(
                sequence = snapshot.sequence,
                "kept an Android queue snapshot: the shared writer was poisoned",
            );
            return false;
        }
    };
    match queue_persistence::save(&database, &snapshot.queue) {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(
                %error,
                sequence = snapshot.sequence,
                "kept an Android queue snapshot after a database write failure",
            );
            false
        }
    }
}

fn mark_committed(flush_state: &Arc<(Mutex<FlushState>, Condvar)>, sequence: u64) {
    let (state, changed) = &**flush_state;
    let mut state = state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    state.committed_sequence = state.committed_sequence.max(sequence);
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
