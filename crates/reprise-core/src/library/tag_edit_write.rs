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

use std::path::PathBuf;

use rusqlite::Connection;

use super::tag_edit::{TagBatchReport, TagWriteFailure, TrackEditPatch};
use super::tag_mutation::{
    commit_tag_mutation, prepare_tag_mutation, validate_registered_track, WriteErrorKind,
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

/// Writes exactly one track's pending patch, dispatching into `report`.
/// No-op tracks (nothing effectively changes) are silently skipped: neither
/// `updated_ids` nor `failures` gains an entry, and the file is never
/// touched (TAG-5's mtime guarantee).
fn write_one_track(conn: &mut Connection, write: &TrackWrite, report: &mut TagBatchReport) {
    if let Err(error) = validate_registered_track(conn, write.id, &write.path) {
        push_failure(report, write.id, &write.path, WriteErrorKind::Io, error);
        return;
    }

    let tag_written = if write.patch.tags.is_empty() {
        false
    } else {
        let prepared = match prepare_tag_mutation(conn, write.id, &write.path, &write.patch.tags) {
            Ok(Some(prepared)) => prepared,
            Ok(None) => return apply_rating_only(conn, write, report),
            Err(failure) => {
                let (kind, error, _) = failure.into_parts();
                push_failure(report, write.id, &write.path, kind, error);
                return;
            }
        };
        if let Err(failure) = commit_tag_mutation(conn, &prepared, true) {
            let (kind, error, _) = failure.into_parts();
            push_failure(report, write.id, &write.path, kind, error);
            return;
        }
        true
    };

    let rating_written = match write.patch.rating {
        Some(rating) => match crate::library::stats::set_rating(conn, write.id, rating) {
            Ok(()) => true,
            Err(error) => {
                push_failure(
                    report,
                    write.id,
                    &write.path,
                    WriteErrorKind::Io,
                    format!("could not save rating: {error}"),
                );
                return;
            }
        },
        None => false,
    };

    if tag_written || rating_written {
        report.updated_ids.push(write.id);
    }
}

fn apply_rating_only(conn: &mut Connection, write: &TrackWrite, report: &mut TagBatchReport) {
    let Some(rating) = write.patch.rating else {
        return;
    };
    match crate::library::stats::set_rating(conn, write.id, rating) {
        Ok(()) => report.updated_ids.push(write.id),
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
    let mut report = TagBatchReport::default();
    let total = writes.len();
    for (index, write) in writes.iter().enumerate() {
        write_one_track(conn, write, &mut report);
        progress(index + 1, total);
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::tag_edit::{classify_write_error, TagEditError, TagPatch};
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
}
