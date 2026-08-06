//! Bounded worker pool for the filesystem phase of a tag-write batch.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

use super::tag_mutation::{TagMutationFailure, WriteErrorKind};
use super::tag_write_job::{write_tag_write_file, JournaledTagMutation};

/// Measured on a 13-track album of 16 MB MP3s (cold page cache, real library):
/// serial 1.85 s, four workers 1.17 s, eight workers 0.80 s. The gain is I/O
/// bandwidth rather than CPU, so the cap is deliberately above the point where
/// four workers left throughput on the table, and low enough that a batch never
/// has more than eight large rewrites in flight at once.
const MAX_FILE_WRITE_WORKERS: usize = 8;

fn file_write_worker_count(task_count: usize, available: usize) -> usize {
    task_count.min(available.max(1)).min(MAX_FILE_WRITE_WORKERS)
}

fn parallel_map<T, R, F, C>(
    items: &[T],
    available: usize,
    operation: F,
    mut on_complete: C,
) -> Vec<(usize, std::thread::Result<R>)>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync,
    C: FnMut(),
{
    let worker_count = file_write_worker_count(items.len(), available);
    let next = AtomicUsize::new(0);
    let (sender, receiver) = mpsc::channel();
    let completed = std::thread::scope(|scope| {
        let operation = &operation;
        for _ in 0..worker_count {
            let sender = sender.clone();
            let next = &next;
            scope.spawn(move || loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(item) = items.get(index) else {
                    break;
                };
                // Catch here, not at the scope: an escaping worker panic is
                // resumed by `std::thread::scope` when the scope closes, which
                // throws away every result the batch had already collected.
                // The caller then never reconciles the files that DID succeed,
                // leaving their journal rows `running` forever — and a single
                // stuck job holds the global tag-write slot, so every later
                // edit in the session is refused as busy. One damaged file must
                // cost one file.
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| operation(item)));
                if sender.send((index, result)).is_err() {
                    break;
                }
            });
        }
        drop(sender);
        let mut completed = Vec::with_capacity(items.len());
        for result in receiver {
            on_complete();
            completed.push(result);
        }
        completed
    });
    completed
}

pub(super) fn parallel_file_writes(
    files: &[&JournaledTagMutation],
    on_complete: &mut dyn FnMut(),
) -> Vec<(usize, Result<(), TagMutationFailure>)> {
    let available = std::thread::available_parallelism().map_or(1, usize::from);
    parallel_map(
        files,
        available,
        |file| write_tag_write_file(file, true),
        on_complete,
    )
    .into_iter()
    .map(|(index, outcome)| (index, outcome.unwrap_or_else(|payload| panicked(&payload))))
    .collect()
}

/// Turns a caught worker panic into an ordinary per-file failure. `file_written`
/// is reported as true deliberately: a panic can strike part-way through
/// `save_to_path`, so the file must be treated as possibly modified — the
/// recovery path can re-check a file it did not need to, but must never skip
/// one it did.
fn panicked(payload: &Box<dyn std::any::Any + Send>) -> Result<(), TagMutationFailure> {
    let detail = payload
        .downcast_ref::<&str>()
        .map(|message| (*message).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_string());
    Err(TagMutationFailure {
        kind: WriteErrorKind::Io,
        error: format!("the tag write panicked: {detail}"),
        file_written: true,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use super::*;

    #[test]
    fn tag_batch_file_writes_use_eight_workers_without_exceeding_the_cap() {
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let tasks = (0..12).collect::<Vec<_>>();

        let completed = parallel_map(
            &tasks,
            16,
            |_| {
                let now_active = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now_active, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(20));
                active.fetch_sub(1, Ordering::SeqCst);
            },
            || {},
        );

        assert_eq!(completed.len(), tasks.len());
        assert_eq!(peak.load(Ordering::SeqCst), 8);
        assert_eq!(file_write_worker_count(13, 2), 2);
        assert_eq!(file_write_worker_count(13, 16), 8);
        assert_eq!(file_write_worker_count(1, 8), 1);
        assert_eq!(file_write_worker_count(0, 8), 0);
    }

    /// A tag write that panics must fail only its own file. Before this was
    /// guarded, `std::thread::scope` resumed the worker's panic when the scope
    /// closed, unwinding out of the whole batch: every already-claimed journal
    /// row stayed `running`, `finish_tag_write_job` never ran, and because
    /// `claim_tag_write_slot` treats any `running` job as holding the single
    /// global slot, every later tag edit in the session was refused as busy.
    /// Serially, one bad file cost exactly one file.
    #[test]
    fn a_panicking_file_write_fails_only_its_own_file() {
        let tasks = (0..12).collect::<Vec<usize>>();

        let completed = parallel_map(
            &tasks,
            8,
            |task| {
                if *task == 5 {
                    panic!("simulated tag-parser panic");
                }
                Ok::<usize, String>(*task)
            },
            || {},
        );

        assert_eq!(
            completed.len(),
            tasks.len(),
            "every task must report a result, panicking or not"
        );
        let failed: Vec<usize> = completed
            .iter()
            .filter(|(_, outcome)| outcome.is_err())
            .map(|(index, _)| *index)
            .collect();
        assert_eq!(failed, vec![5], "only the panicking task may fail");
    }
}
