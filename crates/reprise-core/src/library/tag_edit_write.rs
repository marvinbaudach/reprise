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

use super::tag_edit::{TagBatchReport, TagWriteFailure, TrackEditPatch};
use super::tag_mutation::{
    prepare_tag_mutation, validate_registered_track, PreparedTagMutation, WriteErrorKind,
};
use super::tag_write_job::{
    execute_tag_write_file, finish_tag_write_job, prepare_tag_write_job, TagWriteJobSpec,
};

/// One track's write request: the effective patch to apply plus enough
/// identity (`id`/`path`) to validate, write, and reconcile it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackWrite {
    pub id: i64,
    pub path: PathBuf,
    pub patch: TrackEditPatch,
}

fn push_failure(
    report: &mut TagBatchReport,
    id: i64,
    path: &std::path::Path,
    kind: WriteErrorKind,
    error: String,
) {
    report.failures.push(TagWriteFailure {
        id,
        path: path.to_path_buf(),
        kind,
        error,
    });
}

fn apply_rating_only(conn: &mut Connection, write: &TrackWrite, report: &mut TagBatchReport) {
    let Some(rating) = write.patch.rating else {
        return;
    };
    match crate::library::stats::set_rating_for_registered_track(
        conn,
        write.id,
        &write.path,
        rating,
    ) {
        Ok(true) => report.updated_ids.push(write.id),
        Ok(false) => push_failure(
            report,
            write.id,
            &write.path,
            WriteErrorKind::Io,
            "track path changed before rating; refusing stale request".into(),
        ),
        Err(error) => push_failure(
            report,
            write.id,
            &write.path,
            WriteErrorKind::Io,
            format!("could not save rating: {error}"),
        ),
    }
}

/// Applies each of `writes` in order, reporting `(processed, total)` via
/// `progress` after every one — success, no-op skip, or failure alike, so a
/// caller streaming "Saving… x/N" always reaches `total` at the end.
pub fn apply_track_writes(
    conn: &mut Connection,
    writes: &[TrackWrite],
    progress: &mut dyn FnMut(usize, usize),
) -> TagBatchReport {
    apply_track_writes_inner(conn, writes, progress, &mut |_, _, _| {})
}

fn apply_track_writes_inner(
    conn: &mut Connection,
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

    for (index, write) in writes.iter().enumerate() {
        if let Some((kind, error)) = preparation_failures[index].take() {
            push_failure(&mut report, write.id, &write.path, kind, error);
            progress(index + 1, total);
            continue;
        }
        let journaled = job
            .as_ref()
            .and_then(|job| job.files.iter().find(|file| file.position == index));
        if let Some(file) = journaled {
            if let Err(failure) =
                execute_tag_write_file(conn, job.as_ref().unwrap().id, file, true, before_save)
            {
                let (kind, error, _) = failure.into_parts();
                push_failure(&mut report, write.id, &write.path, kind, error);
                progress(index + 1, total);
                continue;
            }
            if write.patch.rating.is_some() {
                apply_rating_only(conn, write, &mut report);
            } else {
                report.updated_ids.push(write.id);
            }
        } else {
            apply_rating_only(conn, write, &mut report);
        }
        progress(index + 1, total);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::tag_edit::{
        classify_write_error, read_editable_tags, TagEditError, TagPatch,
    };
    use std::path::Path;

    const TINY_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x62, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    fn fixture_copy(dir: &Path, name: &str) -> PathBuf {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
        let destination = dir.join(name);
        std::fs::copy(source, &destination).unwrap();
        destination
    }

    fn seed_full_tag(path: &Path) {
        use lofty::picture::{MimeType, Picture, PictureType};
        use lofty::prelude::*;
        use lofty::tag::ItemKey;

        let mut tagged = lofty::read_from_path(path).unwrap();
        let tag = tagged.primary_tag_mut().unwrap();
        tag.set_title("Old title".into());
        tag.set_artist("Keep artist".into());
        tag.set_album("Keep album".into());
        tag.insert_text(ItemKey::AlbumArtist, "Keep album artist".into());
        tag.set_date(lofty::tag::items::Timestamp {
            year: 1999,
            ..lofty::tag::items::Timestamp::default()
        });
        tag.set_track(7);
        tag.set_genre("Keep genre".into());
        tag.push_picture(
            Picture::unchecked(TINY_PNG.to_vec())
                .pic_type(PictureType::CoverFront)
                .mime_type(MimeType::Png)
                .build(),
        );
        tag.save_to_path(path, lofty::config::WriteOptions::default())
            .unwrap();
    }

    fn seeded_track(dir: &Path, name: &str) -> (Connection, i64, PathBuf) {
        let path = fixture_copy(dir, name);
        seed_full_tag(&path);
        let mut conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        crate::library::scanner::scan_folder(&mut conn, &path).unwrap();
        let path_text = path.to_string_lossy().to_string();
        let id: i64 = conn
            .query_row("SELECT id FROM tracks WHERE path=?1", [&path_text], |row| {
                row.get(0)
            })
            .unwrap();
        (conn, id, path)
    }

    #[test]
    fn tag_5_noop_write_is_skipped_file_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let (mut conn, id, path) = seeded_track(dir.path(), "noop.flac");
        let before_bytes = std::fs::read(&path).unwrap();
        let before_mtime = std::fs::metadata(&path).unwrap().modified().unwrap();

        let write = TrackWrite {
            id,
            path: path.clone(),
            patch: TrackEditPatch {
                tags: TagPatch {
                    // Same value the file already has: a real diff exists
                    // nowhere, so this must be skipped entirely.
                    title: Some("Old title".into()),
                    ..TagPatch::default()
                },
                rating: None,
            },
        };

        let mut progress_calls = Vec::new();
        let report = apply_track_writes(&mut conn, &[write], &mut |done, total| {
            progress_calls.push((done, total));
        });

        assert!(report.updated_ids.is_empty());
        assert!(report.failures.is_empty());
        assert_eq!(std::fs::read(&path).unwrap(), before_bytes);
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            before_mtime
        );
        assert_eq!(progress_calls, vec![(1, 1)]);
    }

    #[test]
    fn tag_5_rating_only_counts_but_writes_db_only() {
        let dir = tempfile::tempdir().unwrap();
        let (mut conn, id, path) = seeded_track(dir.path(), "rating-only.flac");
        let before_bytes = std::fs::read(&path).unwrap();

        let write = TrackWrite {
            id,
            path: path.clone(),
            patch: TrackEditPatch {
                tags: TagPatch::default(),
                rating: Some(5),
            },
        };

        let report = apply_track_writes(&mut conn, &[write], &mut |_, _| {});

        assert_eq!(report.updated_ids, vec![id]);
        assert!(report.failures.is_empty());
        assert_eq!(std::fs::read(&path).unwrap(), before_bytes);
        let rating: i32 = conn
            .query_row("SELECT rating FROM tracks WHERE id=?1", [id], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(rating, 5);
    }

    #[test]
    fn tag_5_progress_reports_written_over_total() {
        let dir = tempfile::tempdir().unwrap();
        let (mut conn, id_a, path_a) = seeded_track(dir.path(), "progress-a.flac");
        let path_b = fixture_copy(dir.path(), "progress-b.flac");
        seed_full_tag(&path_b);
        crate::library::scanner::scan_folder(&mut conn, &path_b).unwrap();
        let path_b_text = path_b.to_string_lossy().to_string();
        let id_b: i64 = conn
            .query_row(
                "SELECT id FROM tracks WHERE path=?1",
                [&path_b_text],
                |row| row.get(0),
            )
            .unwrap();

        let writes = vec![
            TrackWrite {
                id: id_a,
                path: path_a,
                patch: TrackEditPatch {
                    tags: TagPatch {
                        title: Some("New A".into()),
                        ..TagPatch::default()
                    },
                    rating: None,
                },
            },
            TrackWrite {
                id: id_b,
                path: path_b,
                patch: TrackEditPatch {
                    tags: TagPatch {
                        title: Some("New B".into()),
                        ..TagPatch::default()
                    },
                    rating: None,
                },
            },
        ];

        let mut progress_calls = Vec::new();
        let report = apply_track_writes(&mut conn, &writes, &mut |done, total| {
            progress_calls.push((done, total));
        });

        assert_eq!(report.updated_ids.len(), 2);
        assert_eq!(progress_calls, vec![(1, 2), (2, 2)]);
        let journal_order = conn
            .prepare(
                "SELECT track_id FROM tag_write_job_files \
                 WHERE job_id=(SELECT MAX(id) FROM tag_write_jobs) ORDER BY position",
            )
            .unwrap()
            .query_map([], |row| row.get::<_, i64>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(journal_order, vec![id_a, id_b]);
    }

    #[test]
    fn write_error_classification_maps_permission_denied() {
        let error =
            TagEditError::Lofty(lofty::error::LoftyError::new(lofty::error::ErrorKind::Io(
                std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
            )));
        assert_eq!(
            classify_write_error(&error),
            WriteErrorKind::PermissionDenied
        );
    }

    #[test]
    fn write_error_classification_maps_not_found() {
        let error = TagEditError::Lofty(lofty::error::LoftyError::new(
            lofty::error::ErrorKind::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "gone")),
        ));
        assert_eq!(classify_write_error(&error), WriteErrorKind::NotFound);
    }

    #[test]
    fn write_error_classification_maps_unsupported_format() {
        assert_eq!(
            classify_write_error(&TagEditError::NoWritableTag),
            WriteErrorKind::UnsupportedFormat
        );
        // Write-specific override of the shared classifier, which reads this
        // as container damage (`UnreadableTags`): writing, it means the
        // target format cannot hold this tag type. Deliberate divergence —
        // see `classify_write_error`'s doc comment before "fixing" it.
        let error = TagEditError::Lofty(lofty::error::LoftyError::new(
            lofty::error::ErrorKind::UnsupportedTag,
        ));
        assert_eq!(
            classify_write_error(&error),
            WriteErrorKind::UnsupportedFormat
        );
    }

    #[test]
    fn write_error_classification_defaults_to_io() {
        // Same story as UnsupportedTag above: too much data for the format is
        // a write-size failure, not tags that cannot be read back.
        let error = TagEditError::Lofty(lofty::error::LoftyError::new(
            lofty::error::ErrorKind::TooMuchData,
        ));
        assert_eq!(classify_write_error(&error), WriteErrorKind::Io);
    }

    #[test]
    fn write_error_classification_shares_the_scanner_view_of_damaged_tags() {
        // The long tail of Lofty parse failures is NOT re-enumerated here —
        // it comes from `import_errors::classify_lofty`, so a damaged
        // container tells the same story in the scanner and the tag editor.
        let error = TagEditError::Lofty(lofty::error::LoftyError::new(
            lofty::error::ErrorKind::NotAPicture,
        ));
        assert_eq!(classify_write_error(&error), WriteErrorKind::UnreadableTags);
    }

    #[test]
    fn a_stale_track_row_fails_before_touching_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let (mut conn, id, _path) = seeded_track(dir.path(), "stale.flac");
        let other = fixture_copy(dir.path(), "stale-other.flac");
        seed_full_tag(&other);
        let before = std::fs::read(&other).unwrap();

        let write = TrackWrite {
            id,
            path: other.clone(),
            patch: TrackEditPatch {
                tags: TagPatch {
                    title: Some("Must not be written".into()),
                    ..TagPatch::default()
                },
                rating: None,
            },
        };

        let report = apply_track_writes(&mut conn, &[write], &mut |_, _| {});

        assert!(report.updated_ids.is_empty());
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].kind, WriteErrorKind::Io);
        assert_eq!(std::fs::read(&other).unwrap(), before);
    }

    #[test]
    fn combined_tag_and_rating_write_reconciles_both() {
        let dir = tempfile::tempdir().unwrap();
        let (mut conn, id, path) = seeded_track(dir.path(), "combined.flac");

        let write = TrackWrite {
            id,
            path: path.clone(),
            patch: TrackEditPatch {
                tags: TagPatch {
                    title: Some("New combined title".into()),
                    ..TagPatch::default()
                },
                rating: Some(3),
            },
        };

        let report = apply_track_writes(&mut conn, &[write], &mut |_, _| {});

        assert_eq!(report.updated_ids, vec![id]);
        assert!(report.failures.is_empty());
        let row: (String, i32) = conn
            .query_row(
                "SELECT title, rating FROM tracks WHERE id=?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(row, ("New combined title".into(), 3));
    }

    #[test]
    fn tag_editor_adapter_completes_journal_without_moving_doctor_pointer() {
        let dir = tempfile::tempdir().unwrap();
        let (mut conn, id, path) = seeded_track(dir.path(), "journaled.flac");
        conn.execute(
            "INSERT INTO library_doctor_scans \
             (scope_kind, created_at, remote_enabled, checked_tracks, skipped_tracks) \
             VALUES ('selection', 0, 0, 1, 0)",
            [],
        )
        .unwrap();
        let scan_id = conn.last_insert_rowid();
        conn.execute(
            "UPDATE library_doctor_state SET last_complete_scan_id=?1 WHERE singleton=1",
            [scan_id],
        )
        .unwrap();

        let report = apply_track_writes(
            &mut conn,
            &[TrackWrite {
                id,
                path,
                patch: TrackEditPatch {
                    tags: TagPatch {
                        title: Some("Journaled title".into()),
                        ..TagPatch::default()
                    },
                    rating: None,
                },
            }],
            &mut |_, _| {},
        );

        assert_eq!(report.updated_ids, vec![id]);
        assert!(report.failures.is_empty());
        let journal: (String, String, String, i64, String, String) = conn
            .query_row(
                "SELECT j.kind, j.state, f.state, f.file_written, \
                        v.before_value, v.after_value \
                 FROM tag_write_jobs j \
                 JOIN tag_write_job_files f ON f.job_id=j.id \
                 JOIN tag_write_journal v ON v.file_id=f.id \
                 WHERE v.field='title'",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            journal,
            (
                "tag_editor".into(),
                "completed".into(),
                "complete".into(),
                1,
                "Old title".into(),
                "Journaled title".into(),
            )
        );
        let pointer: Option<i64> = conn
            .query_row(
                "SELECT last_complete_scan_id FROM library_doctor_state WHERE singleton=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(pointer, Some(scan_id));
    }

    #[test]
    fn journal_records_when_file_write_precedes_reconciliation_failure() {
        let dir = tempfile::tempdir().unwrap();
        let (mut conn, id, path) = seeded_track(dir.path(), "journal-error.flac");
        conn.execute_batch(
            "CREATE TRIGGER reject_journal_reconcile
             BEFORE UPDATE OF file_mtime ON tracks
             WHEN NEW.file_mtime = -1
             BEGIN
               SELECT RAISE(FAIL, 'injected reconcile failure');
             END;",
        )
        .unwrap();

        let report = apply_track_writes(
            &mut conn,
            &[TrackWrite {
                id,
                path: path.clone(),
                patch: TrackEditPatch {
                    tags: TagPatch {
                        title: Some("Written before failure".into()),
                        ..TagPatch::default()
                    },
                    rating: None,
                },
            }],
            &mut |_, _| {},
        );

        assert!(report.updated_ids.is_empty());
        assert_eq!(report.failures.len(), 1);
        assert_eq!(
            read_editable_tags(&path).unwrap().title,
            "Written before failure"
        );
        let file: (String, String, i64) = conn
            .query_row(
                "SELECT state, error_kind, file_written FROM tag_write_job_files",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(file, ("failed".into(), "io".into(), 1));
    }

    #[test]
    fn adapter_commits_and_claims_journal_before_first_save() {
        let dir = tempfile::tempdir().unwrap();
        let (mut conn, id, path) = seeded_track(dir.path(), "pre-save-hook.flac");
        let mut hook_called = false;
        let report = apply_track_writes_inner(
            &mut conn,
            &[TrackWrite {
                id,
                path: path.clone(),
                patch: TrackEditPatch {
                    tags: TagPatch {
                        title: Some("After hook".into()),
                        ..TagPatch::default()
                    },
                    rating: None,
                },
            }],
            &mut |_, _| {},
            &mut |conn, job_id, file_id| {
                hook_called = true;
                let states: (String, String, String) = conn
                    .query_row(
                        "SELECT j.state, f.state, v.outcome \
                         FROM tag_write_jobs j \
                         JOIN tag_write_job_files f ON f.job_id=j.id \
                         JOIN tag_write_journal v ON v.file_id=f.id \
                         WHERE j.id=?1 AND f.id=?2",
                        rusqlite::params![job_id, file_id],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .unwrap();
                assert_eq!(
                    states,
                    ("running".into(), "running".into(), "prepared".into())
                );
                assert_eq!(read_editable_tags(&path).unwrap().title, "Old title");
            },
        );

        assert!(hook_called);
        assert_eq!(report.updated_ids, vec![id]);
        assert_eq!(read_editable_tags(&path).unwrap().title, "After hook");
    }
}
