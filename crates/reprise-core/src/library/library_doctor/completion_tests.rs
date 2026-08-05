use std::path::{Path, PathBuf};

use lofty::prelude::*;
use lofty::tag::ItemKey;

use super::*;

fn fixture(dir: &Path, name: &str, artist: &str) -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    let path = dir.join(name);
    std::fs::copy(source, &path).unwrap();
    let mut tagged = lofty::read_from_path(&path).unwrap();
    let tag = tagged.primary_tag_mut().unwrap();
    tag.set_title("Track".into());
    tag.set_artist(artist.to_owned());
    tag.set_album("Album".into());
    tag.insert_text(ItemKey::AlbumArtist, artist.trim().to_owned());
    tag.set_genre("Rock".into());
    tagged
        .primary_tag()
        .unwrap()
        .save_to_path(&path, lofty::config::WriteOptions::default())
        .unwrap();
    path
}

fn scan_file(db: &crate::db::Db, path: &Path) -> DoctorScan {
    crate::library::scanner::scan_folder(db, path).unwrap();
    let track_id = db
        .conn()
        .query_row(
            "SELECT id FROM tracks WHERE path=?1",
            [path.to_string_lossy().as_ref()],
            |row| row.get(0),
        )
        .unwrap();
    let outcome = LibraryDoctor::new(db)
        .scan_local(
            &LocalScanRequest {
                scope: DoctorScopeRequest::Selection {
                    track_ids: vec![track_id],
                },
            },
            |_| ScanControl::Continue,
        )
        .unwrap();
    let DoctorScanOutcome::Completed(scan) = outcome else {
        panic!("scan must complete")
    };
    scan
}

#[test]
fn doc_8b_scan_completion_enqueues_the_auto_applied_job_before_the_summary() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture(dir.path(), "auto.flac", " Artist ");
    let db = crate::db::Db::open_in_memory().unwrap();
    let scan = scan_file(&db, &path);

    let report = LibraryDoctor::new(&db)
        .apply_auto_tier(&scan, |_| DoctorWriteControl::Continue)
        .unwrap()
        .unwrap();

    let applied_changes = report
        .rows
        .iter()
        .filter(|row| row.state == DoctorWriteRowState::Applied)
        .count();
    assert_eq!(
        applied_changes,
        scan_summary(&scan, scan.options.remote_enabled).auto_applied_changes
    );
    assert_eq!(
        db.conn()
            .query_row(
                "SELECT COUNT(*) FROM tag_write_jobs WHERE kind='doctor_apply' AND scan_id=?1",
                [scan.id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );
}

#[test]
fn doc_8b_a_scan_with_no_auto_rows_creates_no_job() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture(dir.path(), "clean.flac", "Artist");
    let db = crate::db::Db::open_in_memory().unwrap();
    let scan = scan_file(&db, &path);

    let report = LibraryDoctor::new(&db)
        .apply_auto_tier(&scan, |_| DoctorWriteControl::Continue)
        .unwrap();

    assert!(report.is_none());
    assert_eq!(
        db.conn()
            .query_row("SELECT COUNT(*) FROM tag_write_jobs", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        0
    );
}
