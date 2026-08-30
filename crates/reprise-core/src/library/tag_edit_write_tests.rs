use super::*;
use crate::library::tag_edit::{classify_write_error, read_editable_tags, TagEditError, TagPatch};
use std::path::Path;

const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x62, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
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

fn seeded_track(dir: &Path, name: &str) -> (crate::db::Db, i64, PathBuf) {
    let path = fixture_copy(dir, name);
    seed_full_tag(&path);
    let conn = crate::db::Db::open_in_memory().unwrap();
    crate::library::scanner::scan_folder(&conn, &path).unwrap();
    let path_text = path.to_string_lossy().to_string();
    let id: i64 = conn
        .conn()
        .query_row("SELECT id FROM tracks WHERE path=?1", [&path_text], |row| {
            row.get(0)
        })
        .unwrap();
    (conn, id, path)
}

/// Copies the broken-tag MP3 fixture (a valid MPEG stream carrying an APE
/// container with an invalid item size, exactly the "unreadable_tags"
/// scanner import failure) into a scanned in-memory library.
fn seeded_broken_track(dir: &Path, name: &str) -> (crate::db::Db, i64, PathBuf) {
    let path = dir.join(name);
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/broken-tags.mp3"),
        &path,
    )
    .unwrap();
    let conn = crate::db::Db::open_in_memory().unwrap();
    // Register the row directly instead of scanning: the scanner now
    // auto-repairs damaged containers, which would fix this file before the
    // test can exercise the editor's own repair path — keep it broken here.
    let path_text = path.to_string_lossy().to_string();
    conn.conn()
        .execute(
            "INSERT INTO tracks (path, title, untagged, added_at) VALUES (?1, ?2, 1, 0)",
            rusqlite::params![path_text, name],
        )
        .unwrap();
    let id = conn.conn().last_insert_rowid();
    (conn, id, path)
}

#[test]
fn tag_editor_repairs_an_unreadable_container_by_stripping_and_rewriting() {
    let dir = tempfile::tempdir().unwrap();
    let (conn, id, path) = seeded_broken_track(dir.path(), "broken.mp3");

    // Precondition: the strict read the write path relies on genuinely
    // fails on this container, which is why editing used to be impossible.
    assert!(
        matches!(read_editable_tags(&path), Err(TagEditError::Lofty(_))),
        "fixture must have an unreadable tag container"
    );

    let write = TrackWrite {
        id,
        path: path.clone(),
        patch: TrackEditPatch {
            tags: TagPatch {
                title: Some("Repaired".into()),
                artist: Some("Tester".into()),
                ..TagPatch::default()
            },
            rating: None,
        },
    };
    let report = apply_track_writes(&conn, &[write], &mut |_, _| {});

    assert!(
        report.failures.is_empty(),
        "unexpected failures: {:?}",
        report.failures
    );
    assert_eq!(report.updated_ids, vec![id]);

    // The container is now strictly readable and carries the new values.
    let tags = read_editable_tags(&path).unwrap();
    assert_eq!(tags.title, "Repaired");
    assert_eq!(tags.artist, "Tester");
}

#[test]
fn tag_5_noop_write_is_skipped_file_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let (conn, id, path) = seeded_track(dir.path(), "noop.flac");
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
    let report = apply_track_writes(&conn, &[write], &mut |done, total| {
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
    let (conn, id, path) = seeded_track(dir.path(), "rating-only.flac");
    let before_bytes = std::fs::read(&path).unwrap();

    let write = TrackWrite {
        id,
        path: path.clone(),
        patch: TrackEditPatch {
            tags: TagPatch::default(),
            rating: Some(5),
        },
    };

    let report = apply_track_writes(&conn, &[write], &mut |_, _| {});

    assert_eq!(report.updated_ids, vec![id]);
    assert!(report.failures.is_empty());
    assert_eq!(std::fs::read(&path).unwrap(), before_bytes);
    let rating: i32 = conn
        .conn()
        .query_row("SELECT rating FROM tracks WHERE id=?1", [id], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(rating, 5);
}

/// Defense in depth for the tag editor's year-wipe bug: the UI is what
/// used to send `year: Some(None)` for an untouched Mixed field, but the
/// write layer is where the damage happened, so it states the contract
/// itself. A patch that carries no year must leave the file's date alone;
/// only an explicit `Some(None)` may remove it. Both branches are checked
/// here so neither can rot into the other.
#[test]
fn a_patch_without_a_year_leaves_the_date_on_disk_intact() {
    let dir = tempfile::tempdir().unwrap();
    let (conn, id, path) = seeded_track(dir.path(), "keeps-year.flac");
    assert_eq!(read_editable_tags(&path).unwrap().year, Some(1999));

    let report = apply_track_writes(
        &conn,
        &[TrackWrite {
            id,
            path: path.clone(),
            patch: TrackEditPatch {
                tags: TagPatch {
                    title: Some("Renamed, still 1999".into()),
                    ..TagPatch::default()
                },
                rating: Some(5),
            },
        }],
        &mut |_, _| {},
    );
    assert!(report.failures.is_empty(), "{:?}", report.failures);

    let tags = read_editable_tags(&path).unwrap();
    assert_eq!(tags.title, "Renamed, still 1999");
    assert_eq!(
        tags.year,
        Some(1999),
        "a patch with no year must not touch the file's date"
    );

    // The counter-check: an explicit clear still clears, so the guard
    // above is about `None` vs `Some(None)`, not about years being
    // unremovable.
    let report = apply_track_writes(
        &conn,
        &[TrackWrite {
            id,
            path: path.clone(),
            patch: TrackEditPatch {
                tags: TagPatch {
                    year: Some(None),
                    ..TagPatch::default()
                },
                rating: None,
            },
        }],
        &mut |_, _| {},
    );
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(read_editable_tags(&path).unwrap().year, None);
}

#[test]
fn tag_5_progress_reports_written_over_total() {
    let dir = tempfile::tempdir().unwrap();
    let (conn, id_a, path_a) = seeded_track(dir.path(), "progress-a.flac");
    let path_b = fixture_copy(dir.path(), "progress-b.flac");
    seed_full_tag(&path_b);
    crate::library::scanner::scan_folder(&conn, &path_b).unwrap();
    let path_b_text = path_b.to_string_lossy().to_string();
    let id_b: i64 = conn
        .conn()
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
    let report = apply_track_writes(&conn, &writes, &mut |done, total| {
        progress_calls.push((done, total));
    });

    assert_eq!(report.updated_ids.len(), 2);
    assert_eq!(progress_calls, vec![(1, 2), (2, 2)]);
    let journal_order = conn
        .conn()
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
    let error = TagEditError::Lofty(
        std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied").into(),
    );
    assert_eq!(
        classify_write_error(&error),
        WriteErrorKind::PermissionDenied
    );
}

#[test]
fn write_error_classification_maps_not_found() {
    let read_error = TagEditError::Lofty(
        std::io::Error::new(std::io::ErrorKind::NotFound, "gone while reading").into(),
    );
    let write_error = TagEditError::LoftyWrite(
        std::io::Error::new(std::io::ErrorKind::NotFound, "gone while writing").into(),
    );
    assert_eq!(classify_write_error(&read_error), WriteErrorKind::NotFound);
    assert_eq!(classify_write_error(&write_error), WriteErrorKind::NotFound);
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
    let error = TagEditError::LoftyWrite(lofty::error::UnsupportedTagError.into());
    assert_eq!(
        classify_write_error(&error),
        WriteErrorKind::UnsupportedFormat
    );
}

#[test]
fn write_error_classification_defaults_to_io() {
    // Same story as UnsupportedTag above: too much data for the format is
    // a write-size failure, not tags that cannot be read back.
    let error = TagEditError::LoftyWrite(lofty::error::TooMuchDataError.into());
    assert_eq!(classify_write_error(&error), WriteErrorKind::Io);
}

#[test]
fn write_error_classification_shares_the_scanner_view_of_damaged_tags() {
    // The long tail of Lofty parse failures is NOT re-enumerated here —
    // it comes from `import_errors::classify_lofty`, so a damaged
    // container tells the same story in the scanner and the tag editor.
    // Lofty 0.25's PictureParseError has no public constructor. SizeMismatchError
    // is the closest publicly constructible container-damage fixture and must
    // share the same UnreadableTags fallback as the old NotAPicture case.
    let error = TagEditError::Lofty(lofty::error::SizeMismatchError.into());
    assert_eq!(classify_write_error(&error), WriteErrorKind::UnreadableTags);
}

#[test]
fn a_stale_track_row_fails_before_touching_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let (conn, id, _path) = seeded_track(dir.path(), "stale.flac");
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

    let report = apply_track_writes(&conn, &[write], &mut |_, _| {});

    assert!(report.updated_ids.is_empty());
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].kind, WriteErrorKind::Io);
    assert_eq!(std::fs::read(&other).unwrap(), before);
}

#[test]
fn combined_tag_and_rating_write_reconciles_both() {
    let dir = tempfile::tempdir().unwrap();
    let (conn, id, path) = seeded_track(dir.path(), "combined.flac");

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

    let report = apply_track_writes(&conn, &[write], &mut |_, _| {});

    assert_eq!(report.updated_ids, vec![id]);
    assert!(report.failures.is_empty());
    let row: (String, i32) = conn
        .conn()
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
    let (conn, id, path) = seeded_track(dir.path(), "journaled.flac");
    conn.conn()
        .execute(
            "INSERT INTO library_doctor_scans \
         (scope_kind, created_at, remote_enabled, checked_tracks, skipped_tracks) \
         VALUES ('selection', 0, 0, 1, 0)",
            [],
        )
        .unwrap();
    let scan_id = conn.conn().last_insert_rowid();
    conn.conn()
        .execute(
            "UPDATE library_doctor_state SET last_complete_scan_id=?1 WHERE singleton=1",
            [scan_id],
        )
        .unwrap();

    let report = apply_track_writes(
        &conn,
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
        .conn()
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
        .conn()
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
    let (conn, id, path) = seeded_track(dir.path(), "journal-error.flac");
    conn.conn()
        .execute_batch(
            "CREATE TRIGGER reject_journal_reconcile
         BEFORE UPDATE OF file_mtime ON tracks
         WHEN NEW.file_mtime = -1
         BEGIN
           SELECT RAISE(FAIL, 'injected reconcile failure');
         END;",
        )
        .unwrap();

    let report = apply_track_writes(
        &conn,
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
        .conn()
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
    let (conn, id, path) = seeded_track(dir.path(), "pre-save-hook.flac");
    let mut hook_called = false;
    let report = apply_track_writes_inner(
        conn.conn(),
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

#[test]
fn tag_write_lock_brackets_the_journal_from_prepare_through_finalization() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_copy(dir.path(), "locked-job.flac");
    seed_full_tag(&path);
    let database = dir.path().join("reprise.db");
    let db = crate::db::Db::open_migrated(Some(&database)).unwrap();
    crate::library::scanner::scan_folder(&db, &path).unwrap();
    let id = db
        .conn()
        .query_row(
            "SELECT id FROM tracks WHERE path=?1",
            [path.to_string_lossy().as_ref()],
            |row| row.get(0),
        )
        .unwrap();
    let attempt = crate::library::TagWriteLock::acquire(dir.path()).unwrap();
    let mut observed_live = false;

    let report = apply_track_writes_with_lock(
        &db,
        &[TrackWrite {
            id,
            path,
            patch: TrackEditPatch {
                tags: TagPatch {
                    title: Some("Held through completion".into()),
                    ..TagPatch::default()
                },
                rating: None,
            },
        }],
        attempt,
        &mut |_, _| {
            observed_live = true;
            assert_eq!(
                crate::library::TagWriteLock::probe(dir.path()),
                crate::library::TagWriteLiveness::Live
            );
        },
    );

    assert!(report.failures.is_empty());
    assert!(observed_live);
    assert_eq!(
        crate::library::TagWriteLock::probe(dir.path()),
        crate::library::TagWriteLiveness::Absent
    );
    let state: String = db
        .conn()
        .query_row("SELECT state FROM tag_write_jobs", [], |row| row.get(0))
        .unwrap();
    assert_eq!(state, "completed");
}
