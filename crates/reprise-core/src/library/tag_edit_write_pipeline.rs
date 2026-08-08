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

use super::tag_edit::{TagBatchReport, TrackEditPatch};
use super::tag_edit_write_report::{apply_rating_only, push_failure};
use super::tag_file_write_pool::parallel_file_writes;
use super::tag_mutation::{
    prepare_tag_mutation, validate_registered_track, PreparedTagMutation, WriteErrorKind,
};
use super::tag_write_job::{
    begin_tag_write_file, complete_tag_write_file, finish_tag_write_job, prepare_tag_write_job,
    validate_tag_write_file, TagWriteJobSpec,
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
pub fn apply_track_writes(
    db: &Db,
    writes: &[TrackWrite],
    progress: &mut dyn FnMut(usize, usize),
) -> TagBatchReport {
    let conn = db.conn();
    apply_track_writes_inner(conn, writes, progress, &mut |_, _, _| {})
}

pub(super) fn apply_track_writes_inner(
    conn: &Connection,
    writes: &[TrackWrite],
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
        match prepare_tag_write_job(conn, TagWriteJobSpec::tag_editor(), &prepared) {
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

    let mut file_results = job
        .as_ref()
        .map(|job| (0..job.files.len()).map(|_| None).collect::<Vec<_>>())
        .unwrap_or_default();
    let mut file_claimed = vec![false; file_results.len()];
    let mut file_progress_reported = vec![false; file_results.len()];
    let mut active_file_indices = Vec::new();
    let mut completed = 0;
    if let Some(job) = &job {
        for (file_index, file) in job.files.iter().enumerate() {
            match begin_tag_write_file(conn, job.id, file, before_save) {
                Ok(()) => {
                    file_claimed[file_index] = true;
                    match validate_tag_write_file(conn, file) {
                        Ok(()) => active_file_indices.push(file_index),
                        Err(failure) => file_results[file_index] = Some(Err(failure)),
                    }
                }
                Err(failure) => file_results[file_index] = Some(Err(failure)),
            }
        }
        let active_files = active_file_indices
            .iter()
            .map(|index| &job.files[*index])
            .collect::<Vec<_>>();
        let mut report_file_completion = || {
            completed += 1;
            progress(completed, total);
        };
        for (active_index, result) in
            parallel_file_writes(&active_files, &mut report_file_completion)
        {
            file_progress_reported[active_file_indices[active_index]] = true;
            file_results[active_file_indices[active_index]] = Some(result);
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
            let result = file_results[file_index]
                .take()
                .expect("every claimed tag-write file has a terminal file result");
            let result = if file_claimed[file_index] {
                complete_tag_write_file(conn, file, result)
            } else {
                result
            };
            if let Err(failure) = result {
                let (kind, error, _) = failure.into_parts();
                push_failure(&mut report, write.id, &write.path, kind, error);
                if !file_progress_reported[file_index] {
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
            if file_progress_reported[file_index] {
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
                let write = &writes[file.position];
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
