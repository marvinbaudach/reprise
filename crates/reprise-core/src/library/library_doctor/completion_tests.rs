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

fn scan_selection(db: &crate::db::Db, track_ids: Vec<i64>) -> (DoctorScan, Vec<DoctorScanSummary>) {
    let mut published = Vec::new();
    let outcome = LibraryDoctor::new(db)
        .scan_local(
            &LocalScanRequest {
                scope: DoctorScopeRequest::Selection { track_ids },
            },
            |progress| {
                published.push(progress.summary);
                ScanControl::Continue
            },
        )
        .unwrap();
    let DoctorScanOutcome::Completed(scan) = outcome else {
        panic!("scan must complete")
    };
    (scan, published)
}

fn track_id(db: &crate::db::Db, path: &Path) -> i64 {
    db.conn()
        .query_row(
            "SELECT id FROM tracks WHERE path=?1",
            [path.to_string_lossy().as_ref()],
            |row| row.get(0),
        )
        .unwrap()
}

/// Every number on the result page describes the scan that produced it.
///
/// Three tracks share one normalised artist key: two spell it "Artist One",
/// one "artist one". Across all three the majority settles it and there is no
/// conflict at all. Across only the first two it is a tie, and the conflict is
/// real — with exactly the two scanned tracks as its members. Widening or
/// narrowing the scope therefore has to change the count, which is the whole
/// claim: the number is not a property of the library, it is a property of the
/// scan.
#[test]
fn doc_9a_the_conflict_count_is_a_property_of_the_scanned_scope() {
    let dir = tempfile::tempdir().unwrap();
    let first = fixture(dir.path(), "one.flac", "Artist One");
    let second = fixture(dir.path(), "two.flac", "artist one");
    let third = fixture(dir.path(), "three.flac", "Artist One");
    let db = crate::db::Db::open_in_memory().unwrap();
    crate::library::scanner::scan_folder(&db, dir.path()).unwrap();
    let (first, second, third) = (
        track_id(&db, &first),
        track_id(&db, &second),
        track_id(&db, &third),
    );

    let (tied, published) = scan_selection(&db, vec![first, second]);
    // The fixture writes the same text to artist and album artist, so the tie
    // shows up once per field. Pin the artist one and read its membership.
    let artist_groups = tied
        .unresolved_groups
        .iter()
        .filter(|group| group.field == DoctorField::Artist)
        .collect::<Vec<_>>();
    assert_eq!(
        artist_groups.len(),
        1,
        "two spellings, one each — nothing decides between them"
    );
    let members = &artist_groups[0].members;
    assert_eq!(members.len(), 2, "only the scanned tracks may be counted");
    let mut counted = members
        .iter()
        .map(|member| member.track_id)
        .collect::<Vec<_>>();
    counted.sort_unstable();
    assert_eq!(counted, vec![first.min(second), first.max(second)]);
    assert_eq!(
        scan_summary(&tied, tied.options.remote_enabled).unresolved_groups,
        tied.unresolved_groups.len(),
        "the summary reports the scan's own groups, not a library-wide count"
    );

    let (decided, _) = scan_selection(&db, vec![first, second, third]);
    assert_eq!(
        decided.unresolved_groups.len(),
        0,
        "with the third track in scope the majority spelling wins outright"
    );

    // While the scan runs the answer is not knowable yet — no track on its own
    // can disagree with another — so the live summary reports none rather than
    // a number that contradicts the tracks-checked count beside it.
    assert!(
        published
            .iter()
            .all(|summary| summary.unresolved_groups == 0),
        "a running scan must not publish a conflict count"
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

/// The fix that is already on disk is not a finding any more.
///
/// Writing a tag moves the file's mtime, and a moved mtime is exactly how the
/// scan detects "changed under us" — so on the next load every track the quiet
/// job touched read as stale, stale rows fall out of the quiet tier, and the
/// result page offered the very changes it had just applied. The review list
/// showed them with identical current and proposed values, and the sidebar
/// count (which did exclude them) disagreed with the page.
#[test]
fn doc_9a_a_written_fix_does_not_come_back_as_a_finding_after_a_reload() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture(dir.path(), "written.flac", " Artist ");
    let db = crate::db::Db::open_in_memory().unwrap();
    let scan = scan_file(&db, &path);
    assert!(
        !scan.proposals.is_empty(),
        "the fixture has stray spaces to fix"
    );

    let report = LibraryDoctor::new(&db)
        .apply_auto_tier(&scan, |_| DoctorWriteControl::Continue)
        .unwrap()
        .unwrap();
    assert!(report
        .rows
        .iter()
        .any(|row| row.state == DoctorWriteRowState::Applied));

    let reloaded = LibraryDoctor::new(&db)
        .last_complete_scan()
        .unwrap()
        .expect("the scan is still the last complete one");
    assert!(
        reloaded.proposals.is_empty(),
        "a written proposal is finished, not pending: {:?}",
        reloaded.proposals
    );
    let summary = scan_summary(&reloaded, reloaded.options.remote_enabled);
    assert_eq!(summary.review_changes, 0, "nothing is waiting for the user");
    assert_eq!(summary.auto_applied_changes, 0);
    assert_eq!(
        crate::queries::count_pending_doctor_findings(&db).unwrap(),
        0,
        "the sidebar count and the page it badges have to agree"
    );
}

/// …and it is a finding again the moment it is undone.
#[test]
fn doc_9a_an_undone_fix_is_a_finding_again() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture(dir.path(), "undone.flac", " Artist ");
    let db = crate::db::Db::open_in_memory().unwrap();
    let scan = scan_file(&db, &path);
    let written = scan.proposals.len();
    LibraryDoctor::new(&db)
        .apply_auto_tier(&scan, |_| DoctorWriteControl::Continue)
        .unwrap()
        .unwrap();
    assert!(LibraryDoctor::new(&db)
        .last_complete_scan()
        .unwrap()
        .unwrap()
        .proposals
        .is_empty());

    LibraryDoctor::new(&db)
        .revert_last_cleanup(|_| DoctorWriteControl::Continue)
        .unwrap()
        .expect("there is a cleanup to revert");

    let reloaded = LibraryDoctor::new(&db)
        .last_complete_scan()
        .unwrap()
        .unwrap();
    assert_eq!(
        reloaded.proposals.len(),
        written,
        "the revert put the tags back, so the proposals are open again"
    );
}
