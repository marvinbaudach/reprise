//! Per-track tag writes with progress reporting, an effective no-op skip,
//! and classified write errors (TAG-5: "Tracks = echte Datei-Writes" — a
//! track whose tags already match the requested patch must not be written
//! at all, so its file mtime stays untouched). Split out of `tag_edit.rs`
//! purely to stay under this crate's 800-line-per-file rule; the public
//! surface is re-exported at `library::tag_edit` so callers never see the
//! split.
//!
//! Watcher-ignore timing: [`crate::library::watcher::ignore_path`] is called
//! immediately before the one file write it protects, not upfront for the
//! whole batch — a caller with a large batch would otherwise leave earlier
//! files' ignore windows ticking down (and potentially expiring) while later
//! files are still being processed. The re-read path this write triggers
//! (`file_mtime=-1` + a targeted `scan_folder`) stays inside that same
//! per-file window, and is idempotent against the watcher's own echo of the
//! write regardless (see `watcher::event_is_relevant`'s `Access` handling).

use std::collections::HashMap;
use std::path::PathBuf;

use rusqlite::Connection;

use crate::db::Db;
use crate::library::TagWriteLockAttempt;

use super::tag_edit::{TagBatchReport, TrackEditPatch};
use super::tag_edit_write_report::{apply_rating_only, push_failure};
use super::tag_file_write_pool::parallel_file_writes;
use super::tag_mutation::{
    prepare_tag_mutation, validate_registered_track, PreparedTagMutation, TagMutationFailure,
    WriteErrorKind,
};
use super::tag_write_job::{
    begin_tag_write_file, complete_tag_write_file, finish_tag_write_job,
    prepare_tag_write_job_with_lock, validate_tag_write_file, JournaledTagMutation,
    TagWriteJobSpec,
};

/// One track's write request: the effective patch to apply plus enough
/// identity (`id`/`path`) to validate, write, and reconcile it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackWrite {
    pub id: i64,
    pub path: PathBuf,
    pub patch: TrackEditPatch,
}

/// Applies each of `writes` in order, reporting `(processed, total)` via
/// `progress` after every one — success, no-op skip, or failure alike, so a
/// caller streaming "Saving… x/N" always reaches `total` at the end.
pub fn apply_track_writes_with_lock(
    db: &Db,
    writes: &[TrackWrite],
    lock_attempt: TagWriteLockAttempt,
    progress: &mut dyn FnMut(usize, usize),
) -> TagBatchReport {
    let conn = db.conn();
    apply_track_writes_inner_with_lock(conn, writes, lock_attempt, progress, &mut |_, _, _| {})
}

#[cfg(not(test))]
pub use apply_track_writes_with_lock as apply_track_writes;

#[cfg(test)]
pub fn apply_track_writes(
    db: &Db,
    writes: &[TrackWrite],
    progress: &mut dyn FnMut(usize, usize),
) -> TagBatchReport {
    apply_track_writes_with_lock(db, writes, TagWriteLockAttempt::Unenforceable, progress)
}

pub(super) fn apply_track_writes_inner_with_lock(
    conn: &Connection,
    writes: &[TrackWrite],
    lock_attempt: TagWriteLockAttempt,
    progress: &mut dyn FnMut(usize, usize),
    before_save: &mut dyn FnMut(&Connection, i64, i64),
) -> TagBatchReport {
    let mut report = TagBatchReport::default();
    let total = writes.len();
    let mut prepared = Vec::<(usize, PreparedTagMutation)>::new();
    let mut preparation_failures = (0..total).map(|_| None).collect::<Vec<_>>();
    let mut id_counts = HashMap::<i64, usize>::new();
    for write in writes {
        *id_counts.entry(write.id).or_default() += 1;
    }
    for (position, write) in writes.iter().enumerate() {
        if id_counts.get(&write.id).copied().unwrap_or_default() > 1 {
            preparation_failures[position] = Some((
                WriteErrorKind::Io,
                "duplicate track request in one tag-write job".into(),
            ));
            continue;
        }
        if write.patch.tags.is_empty() {
            if let Err(error) = validate_registered_track(conn, write.id, &write.path) {
                preparation_failures[position] = Some((WriteErrorKind::Io, error));
            }
            continue;
        }
        match prepare_tag_mutation(conn, write.id, &write.path, &write.patch.tags) {
            Ok(Some(mutation)) => prepared.push((position, mutation)),
            Ok(None) => {}
            Err(failure) => {
                let (kind, error, _) = failure.into_parts();
                preparation_failures[position] = Some((kind, error));
            }
        }
    }

    let job = if prepared.is_empty() {
        None
    } else {
        match prepare_tag_write_job_with_lock(
            conn,
            lock_attempt,
            TagWriteJobSpec::tag_editor(),
            &prepared,
        ) {
            Ok(job) => Some(job),
            Err(error) => {
                for (position, _) in &prepared {
                    preparation_failures[*position] = Some((
                        WriteErrorKind::Io,
                        format!("could not prepare tag-write journal: {error}"),
                    ));
                }
                None
            }
        }
    };

    let mut file_states = job
        .as_ref()
        .map(|job| {
            job.files
                .iter()
                .map(|_| FileState::default())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut completed = 0;
    if let Some(job) = &job {
        let mut active_files = Vec::new();
        for (file_index, (file, state)) in job.files.iter().zip(file_states.iter_mut()).enumerate()
        {
            match begin_tag_write_file(conn, job.id, file, before_save) {
                Ok(()) => {
                    state.claimed = true;
                    match validate_tag_write_file(conn, file) {
                        Ok(()) => active_files.push((file_index, file)),
                        Err(failure) => state.result = Some(Err(failure)),
                    }
                }
                Err(failure) => state.result = Some(Err(failure)),
            }
        }
        let files_to_write = active_files
            .iter()
            .map(|(_, file)| *file)
            .collect::<Vec<_>>();
        let mut report_file_completion = || {
            completed += 1;
            progress(completed, total);
        };
        for (active_index, result) in
            parallel_file_writes(&files_to_write, &mut report_file_completion)
        {
            let state = active_files
                .get(active_index)
                .and_then(|(file_index, _)| file_states.get_mut(*file_index));
            if let Some(state) = state {
                state.progress_reported = true;
                state.result = Some(result);
            }
        }
    }

    for (index, write) in writes.iter().enumerate() {
        if let Some((kind, error)) = preparation_failures[index].take() {
            push_failure(&mut report, write.id, &write.path, kind, error);
            completed += 1;
            progress(completed, total);
            continue;
        }
        let journaled = job.as_ref().and_then(|job| {
            job.files
                .iter()
                .enumerate()
                .find(|(_, file)| file.position == index)
        });
        if let Some((file_index, file)) = journaled {
            let (claimed, progress_reported, recorded) = match file_states.get_mut(file_index) {
                Some(state) => (state.claimed, state.progress_reported, state.result.take()),
                None => (false, false, None),
            };
            let result = settle_file_result(conn, file, recorded, claimed);
            if let Err(failure) = result {
                let (kind, error, _) = failure.into_parts();
                push_failure(&mut report, write.id, &write.path, kind, error);
                if !progress_reported {
                    completed += 1;
                    progress(completed, total);
                }
                continue;
            }
            if write.patch.rating.is_some() {
                apply_rating_only(conn, write, &mut report);
            } else {
                report.updated_ids.push(write.id);
            }
            if progress_reported {
                continue;
            }
        } else {
            apply_rating_only(conn, write, &mut report);
        }
        completed += 1;
        progress(completed, total);
    }
    if let Some(job) = job {
        if let Err(error) = finish_tag_write_job(conn, job.id) {
            for file in job.files {
                let Some(write) = writes.get(file.position) else {
                    continue;
                };
                if !report.failures.iter().any(|failure| failure.id == write.id) {
                    report.updated_ids.retain(|id| *id != write.id);
                    push_failure(
                        &mut report,
                        write.id,
                        &write.path,
                        WriteErrorKind::Io,
                        format!("could not complete tag-write journal: {error}"),
                    );
                }
            }
        }
    }
    report
}

/// What one journaled file's write ended in: the outcome of its write and
/// reconcile, whether this run claimed its journal row, and whether the write
/// pool already reported its progress.
#[derive(Default)]
struct FileState {
    result: Option<Result<(), TagMutationFailure>>,
    claimed: bool,
    progress_reported: bool,
}

/// Every claimed file ends with a result. A missing one is that file's failure,
/// and a claimed file still goes through completion so its journal row ends
/// terminal instead of staying claimed for startup recovery to replay or revert.
pub(super) fn settle_file_result(
    conn: &Connection,
    file: &JournaledTagMutation,
    result: Option<Result<(), TagMutationFailure>>,
    claimed: bool,
) -> Result<(), TagMutationFailure> {
    let result = result.unwrap_or_else(|| {
        Err(TagMutationFailure {
            kind: WriteErrorKind::Io,
            error: "the tag-write file finished without a result".into(),
            file_written: false,
        })
    });
    if claimed {
        complete_tag_write_file(conn, file, result)
    } else {
        result
    }
}

#[cfg(test)]
pub(super) fn apply_track_writes_inner(
    conn: &Connection,
    writes: &[TrackWrite],
    progress: &mut dyn FnMut(usize, usize),
    before_save: &mut dyn FnMut(&Connection, i64, i64),
) -> TagBatchReport {
    apply_track_writes_inner_with_lock(
        conn,
        writes,
        TagWriteLockAttempt::Unenforceable,
        progress,
        before_save,
    )
}
