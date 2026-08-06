//! Tag-mutation regression tests split from `tag_mutation.rs` for the code-file size gate.

use std::path::{Path, PathBuf};

use super::*;
use crate::library::tag_edit::{read_editable_tags, TagPatch};
use crate::spectrogram::{TrackSourceFingerprint, TrackSpectrogram};

fn fixture_copy(dir: &Path, name: &str) -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    let destination = dir.join(name);
    std::fs::copy(source, &destination).unwrap();
    destination
}

fn seeded_track(dir: &Path, name: &str) -> (crate::db::Db, i64, PathBuf) {
    let path = fixture_copy(dir, name);
    let mut tagged = lofty::read_from_path(&path).unwrap();
    tagged
        .primary_tag_mut()
        .unwrap()
        .set_title("Original title".into());
    tagged
        .primary_tag()
        .unwrap()
        .save_to_path(&path, lofty::config::WriteOptions::default())
        .unwrap();

    let conn = crate::db::Db::open_in_memory().unwrap();
    crate::library::scanner::scan_folder(&conn, &path).unwrap();
    let path_text = path.to_string_lossy().to_string();
    let id = conn
        .conn()
        .query_row("SELECT id FROM tracks WHERE path=?1", [&path_text], |row| {
            row.get(0)
        })
        .unwrap();
    (conn, id, path)
}

fn current_fingerprint(db: &crate::db::Db, id: i64) -> TrackSourceFingerprint {
    db.conn()
        .query_row(
            "SELECT file_mtime, file_size, device, inode FROM tracks WHERE id=?1",
            [id],
            |row| {
                Ok(TrackSourceFingerprint {
                    mtime_seconds: row.get(0)?,
                    size_bytes: row.get(1)?,
                    device: row.get(2)?,
                    inode: row.get(3)?,
                })
            },
        )
        .unwrap()
}

#[test]
fn a_tag_write_keeps_the_rendering_data_it_did_not_invalidate() {
    let dir = tempfile::tempdir().unwrap();
    let (conn, id, path) = seeded_track(dir.path(), "retagged.flac");
    let spectrogram = TrackSpectrogram::from_cells(vec![5; 48]).unwrap();
    crate::db::set_waveform_peaks(&conn, id, &[3, 4, 5]).unwrap();
    crate::db::set_track_spectrogram(&conn, id, current_fingerprint(&conn, id), &spectrogram)
        .unwrap();

    let prepared = prepare_tag_mutation(
        conn.conn(),
        id,
        &path,
        &TagPatch {
            title: Some("Retagged".into()),
            ..TagPatch::default()
        },
    )
    .unwrap()
    .unwrap();
    commit_tag_mutation(conn.conn(), &prepared, false).unwrap();

    assert_eq!(read_editable_tags(&path).unwrap().title, "Retagged");
    assert_eq!(
        crate::db::get_waveform_peaks(&conn, id).unwrap(),
        Some(vec![3, 4, 5]),
        "a tag write must not throw away the waveform"
    );
    assert_eq!(
        crate::db::get_track_spectrogram(&conn, id).unwrap(),
        Some(spectrogram),
        "a tag write must not throw away the spectrogram"
    );
    assert!(
        crate::db::pending_render_data_tracks(&conn)
            .unwrap()
            .is_empty(),
        "a tag write must not queue the track for a pointless recomputation"
    );
}

#[test]
fn a_replaced_file_still_loses_its_rendering_data() {
    let dir = tempfile::tempdir().unwrap();
    let (conn, id, _path) = seeded_track(dir.path(), "replaced.flac");
    crate::db::set_waveform_peaks(&conn, id, &[3, 4, 5]).unwrap();
    crate::db::set_track_spectrogram(
        &conn,
        id,
        current_fingerprint(&conn, id),
        &TrackSpectrogram::from_cells(vec![5; 48]).unwrap(),
    )
    .unwrap();

    conn.conn()
        .execute(
            "UPDATE tracks SET file_size = file_size + 1 WHERE id=?1",
            [id],
        )
        .unwrap();

    assert_eq!(crate::db::get_waveform_peaks(&conn, id).unwrap(), None);
    assert_eq!(crate::db::get_track_spectrogram(&conn, id).unwrap(), None);
}

#[test]
fn shared_tag_mutation_skips_exact_noop_without_touching_file() {
    let dir = tempfile::tempdir().unwrap();
    let (conn, id, path) = seeded_track(dir.path(), "noop.flac");
    let before = std::fs::read(&path).unwrap();

    let prepared = prepare_tag_mutation(
        conn.conn(),
        id,
        &path,
        &TagPatch {
            title: Some("Original title".into()),
            ..TagPatch::default()
        },
    )
    .unwrap();

    assert!(prepared.is_none());
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn prepared_mutation_captures_actual_before_and_effective_after() {
    let dir = tempfile::tempdir().unwrap();
    let (conn, id, path) = seeded_track(dir.path(), "capture.flac");

    let prepared = prepare_tag_mutation(
        conn.conn(),
        id,
        &path,
        &TagPatch {
            title: Some("New title".into()),
            artist: Some(String::new()),
            ..TagPatch::default()
        },
    )
    .unwrap()
    .unwrap();

    assert_eq!(prepared.before.title, "Original title");
    assert_eq!(prepared.patch.title.as_deref(), Some("New title"));
    assert!(prepared.patch.artist.is_none());
}

#[test]
fn shared_tag_mutation_rejects_stale_id_path_before_touching_file() {
    let dir = tempfile::tempdir().unwrap();
    let (conn, id, _registered) = seeded_track(dir.path(), "registered.flac");
    let other = fixture_copy(dir.path(), "other.flac");
    let before = std::fs::read(&other).unwrap();

    let error = prepare_tag_mutation(
        conn.conn(),
        id,
        &other,
        &TagPatch {
            title: Some("Must not be written".into()),
            ..TagPatch::default()
        },
    )
    .unwrap_err();

    assert_eq!(error.kind, WriteErrorKind::Io);
    assert_eq!(std::fs::read(other).unwrap(), before);
}

#[test]
fn commit_reconciles_using_id_and_path() {
    let dir = tempfile::tempdir().unwrap();
    let (conn, id, path) = seeded_track(dir.path(), "moved.flac");
    let prepared = prepare_tag_mutation(
        conn.conn(),
        id,
        &path,
        &TagPatch {
            title: Some("New title".into()),
            ..TagPatch::default()
        },
    )
    .unwrap()
    .unwrap();
    let replacement = dir.path().join("replacement.flac");
    conn.conn()
        .execute(
            "UPDATE tracks SET path=?1, file_mtime=123 WHERE id=?2",
            rusqlite::params![replacement.to_string_lossy(), id],
        )
        .unwrap();

    let error = commit_tag_mutation(conn.conn(), &prepared, false).unwrap_err();

    assert_eq!(error.kind, WriteErrorKind::Io);
    assert_eq!(read_editable_tags(&path).unwrap().title, "Original title");
    let file_mtime: i64 = conn
        .conn()
        .query_row("SELECT file_mtime FROM tracks WHERE id=?1", [id], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(file_mtime, 123);
    assert!(!error.file_written);
}

#[test]
fn commit_rejects_an_affected_field_changed_after_prepare() {
    let dir = tempfile::tempdir().unwrap();
    let (conn, id, path) = seeded_track(dir.path(), "external-change.flac");
    let prepared = prepare_tag_mutation(
        conn.conn(),
        id,
        &path,
        &TagPatch {
            title: Some("Doctor title".into()),
            ..TagPatch::default()
        },
    )
    .unwrap()
    .unwrap();
    let mut tagged = lofty::read_from_path(&path).unwrap();
    tagged
        .primary_tag_mut()
        .unwrap()
        .set_title("External title".into());
    tagged
        .primary_tag()
        .unwrap()
        .save_to_path(&path, lofty::config::WriteOptions::default())
        .unwrap();

    let error = commit_tag_mutation(conn.conn(), &prepared, false).unwrap_err();

    assert!(!error.file_written);
    assert_eq!(read_editable_tags(&path).unwrap().title, "External title");
}

#[test]
fn reconciliation_failure_reports_that_the_file_was_written() {
    let dir = tempfile::tempdir().unwrap();
    let (conn, id, path) = seeded_track(dir.path(), "reconcile-failure.flac");
    let prepared = prepare_tag_mutation(
        conn.conn(),
        id,
        &path,
        &TagPatch {
            title: Some("Written title".into()),
            ..TagPatch::default()
        },
    )
    .unwrap()
    .unwrap();
    conn.conn()
        .execute_batch(
            "CREATE TRIGGER reject_tag_reconcile
             BEFORE UPDATE OF file_mtime ON tracks
             WHEN NEW.file_mtime = -1
             BEGIN
               SELECT RAISE(FAIL, 'injected reconcile failure');
             END;",
        )
        .unwrap();

    let error = commit_tag_mutation(conn.conn(), &prepared, false).unwrap_err();

    assert!(error.file_written);
    assert_eq!(read_editable_tags(&path).unwrap().title, "Written title");
}
