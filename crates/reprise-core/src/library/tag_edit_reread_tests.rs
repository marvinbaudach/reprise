//! Immediate tag-editor reread and stale-identity regression tests.

use std::path::{Path, PathBuf};

use lofty::prelude::*;

use super::{apply_patch_batch, prepare_tag_reconciliation, TagPatch};

fn readable_fixture(dir: &Path) -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    let path = dir.join("healed-tags.flac");
    std::fs::copy(source, &path).unwrap();
    let mut tagged = lofty::read_from_path(&path).unwrap();
    tagged
        .primary_tag_mut()
        .unwrap()
        .set_title("Old title".into());
    tagged.primary_tag_mut().unwrap().set_disk(2);
    tagged
        .primary_tag()
        .unwrap()
        .save_to_path(&path, lofty::config::WriteOptions::default())
        .unwrap();
    path
}

#[test]
fn tag_editor_save_rereads_tags_and_clears_the_untagged_import_hint_immediately() {
    let dir = tempfile::tempdir().unwrap();
    let path = readable_fixture(dir.path());
    let mut conn = crate::db::open_migrated(None).unwrap();
    crate::library::scanner::scan_folder(&mut conn, &path).unwrap();
    let path_text = path.to_string_lossy().to_string();
    let id: i64 = conn
        .query_row("SELECT id FROM tracks WHERE path=?1", [&path_text], |row| {
            row.get(0)
        })
        .unwrap();
    conn.execute("UPDATE tracks SET untagged=1 WHERE id=?1", [id])
        .unwrap();
    conn.execute(
        "INSERT INTO import_errors \
         (path,reason_kind,reason_detail,first_seen,last_seen) \
         VALUES (?1,'unreadable_tags','broken tags',1,1)",
        [&path_text],
    )
    .unwrap();

    let report = apply_patch_batch(
        &mut conn,
        &[(id, path)],
        &TagPatch {
            title: Some("Readable again".into()),
            ..TagPatch::default()
        },
    );

    assert_eq!(report.updated_ids, vec![id]);
    assert!(report.failures.is_empty());
    let (title, disc_no, untagged): (String, Option<i64>, i64) = conn
        .query_row(
            "SELECT title,disc_no,untagged FROM tracks WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(title, "Readable again");
    assert_eq!(disc_no, Some(2));
    assert_eq!(untagged, 0);
    let hints: i64 = conn
        .query_row(
            "SELECT count(*) FROM import_errors WHERE path=?1",
            [&path_text],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(hints, 0);
    assert_eq!(
        crate::library::settings::get_last_scan_relinked(&conn).unwrap(),
        None,
        "a targeted tag reread is not a library-scan completion"
    );
}

#[test]
fn tag_reconciliation_rechecks_id_path_identity_after_the_file_write() {
    let conn = crate::db::open_migrated(None).unwrap();
    conn.execute(
        "INSERT INTO tracks (id,path,title,artist,added_at,file_mtime) \
         VALUES (7,'/old.flac','Old','Artist',0,123)",
        [],
    )
    .unwrap();
    conn.execute("UPDATE tracks SET path='/relinked.flac' WHERE id=7", [])
        .unwrap();

    let error = prepare_tag_reconciliation(&conn, 7, Path::new("/old.flac")).unwrap_err();

    assert!(error.contains("path changed"));
    let file_mtime: i64 = conn
        .query_row("SELECT file_mtime FROM tracks WHERE id=7", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(file_mtime, 123);
}
