//! Rule-named acceptance coverage for mount evidence and playback faults.

use super::*;
use crate::library::scanner::{self, ScanOutcome};
use crate::library::settings::{self, AutoCleanSetting};
use crate::models::MissingReason;
use crate::playback::{playback_fault_policy, PlaybackFaultNotice};
use std::path::Path;

fn seed_track(
    conn: &Connection,
    id: i64,
    path: &std::path::Path,
    reason: Option<MissingReason>,
    mount_point: Option<&str>,
    removed_at: Option<i64>,
) {
    conn.execute(
        "INSERT INTO tracks \
         (id,path,title,artist,added_at,missing_since,missing_reason,mount_point,removed_at) \
         VALUES (?1,?2,?3,'Artist',0,CASE WHEN ?4 IS NULL THEN NULL ELSE 1 END,?4,?5,?6)",
        rusqlite::params![
            id,
            path.to_string_lossy(),
            format!("Track {id}"),
            reason.map(|reason| reason.as_str()),
            mount_point,
            removed_at,
        ],
    )
    .unwrap();
}

fn missing_state(conn: &Connection, id: i64) -> (Option<i64>, Option<String>) {
    conn.query_row(
        "SELECT missing_since,missing_reason FROM tracks WHERE id=?1",
        [id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .unwrap()
}

// UX P-6: filesystem and mount events apply only evidence: existing
// unmounted/unknown paths heal, an ejected mount marks only its live rows,
// and unavailable guesses never become an automatic deletion basis.
#[test]
fn p_6_mount_evidence_heals_existing_marks_ejected_and_never_deletes_guesses() {
    let dir = tempfile::tempdir().unwrap();
    let mount = dir.path().to_string_lossy();
    let existing_unmounted = dir.path().join("unmounted.flac");
    let existing_unknown = dir.path().join("unknown.flac");
    std::fs::write(&existing_unmounted, b"present").unwrap();
    std::fs::write(&existing_unknown, b"present").unwrap();
    let db = crate::db::Db::open_in_memory().unwrap();
    let conn = db.conn();

    seed_track(
        conn,
        1,
        &existing_unmounted,
        Some(MissingReason::Unmounted),
        Some(&mount),
        None,
    );
    seed_track(
        conn,
        2,
        &existing_unknown,
        Some(MissingReason::Unknown),
        None,
        None,
    );
    seed_track(
        conn,
        3,
        &dir.path().join("still-gone.flac"),
        Some(MissingReason::Unmounted),
        Some(&mount),
        None,
    );
    seed_track(
        conn,
        4,
        &dir.path().join("present-on-ejected.flac"),
        None,
        Some(&mount),
        None,
    );
    seed_track(
        conn,
        5,
        Path::new("/media/other/track.flac"),
        None,
        Some("/media/other"),
        None,
    );
    seed_track(
        conn,
        6,
        &dir.path().join("tombstoned.flac"),
        None,
        Some(&mount),
        Some(9),
    );
    seed_track(
        conn,
        7,
        &dir.path().join("stale-mount-column.flac"),
        None,
        Some("/stale/mount"),
        None,
    );
    seed_track(
        conn,
        8,
        Path::new("/outside/incorrect-column.flac"),
        None,
        Some(&mount),
        None,
    );

    assert_eq!(verify_unmounted_tracks(&db).unwrap(), vec![1, 2]);
    assert_eq!(missing_state(conn, 1), (None, None));
    assert_eq!(missing_state(conn, 2), (None, None));
    assert_eq!(missing_state(conn, 3).1.as_deref(), Some("unmounted"));

    assert_eq!(
        mark_mount_unavailable(&db, &mount, 500).unwrap(),
        4,
        "path containment, not a possibly stale mount_point column, is the evidence"
    );
    assert_eq!(
        missing_state(conn, 1),
        (Some(500), Some("unmounted".into()))
    );
    assert_eq!(
        missing_state(conn, 2),
        (Some(500), Some("unmounted".into()))
    );
    assert_eq!(
        missing_state(conn, 4),
        (Some(500), Some("unmounted".into()))
    );
    assert_eq!(missing_state(conn, 5), (None, None));
    assert_eq!(missing_state(conn, 6), (None, None));
    assert_eq!(
        missing_state(conn, 7),
        (Some(500), Some("unmounted".into()))
    );
    assert_eq!(missing_state(conn, 8), (None, None));

    settings::set_missing_auto_clean(&db, AutoCleanSetting::Days(30)).unwrap();
    settings::set_auto_clean_armed_at(&db, 0).unwrap();
    assert!(auto_clean_eligible(&db, 60 * 86_400).unwrap().is_empty());
}

// UX FB-6: an external deletion is reported as one aggregate reconcile and
// increments the Missing badge without a per-file notice; only a fault of
// the playing track yields exactly one unavailable notice and a skip.
#[test]
fn fb_6_watcher_is_silent_and_playing_track_fault_has_one_notice_then_skips() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    let path = dir.path().join("gone.flac");
    std::fs::copy(fixture, &path).unwrap();
    let db = crate::db::Db::open_in_memory().unwrap();
    let first = scanner::scan_folder(&db, dir.path()).unwrap();
    assert!(matches!(first, ScanOutcome::Completed(_)));
    settings::set_last_viewed_missing(&db, 0).unwrap();
    std::fs::remove_file(&path).unwrap();

    let report = match scanner::scan_folder(&db, dir.path()).unwrap() {
        ScanOutcome::Completed(report) => report,
        ScanOutcome::RootUnavailable { root } => panic!("unexpected unavailable root: {root:?}"),
    };
    assert_eq!(
        report.vanished, 1,
        "the watcher-facing result is aggregate-only"
    );
    assert_eq!(count_new_missing(&db, 0).unwrap(), 1);

    let policy = playback_fault_policy(false);
    assert!(policy.mark_missing);
    assert!(policy.skip);
    assert_eq!(
        policy.notices,
        [PlaybackFaultNotice::TrackUnavailableSkipped]
    );
}

#[test]
fn playback_fault_mark_rechecks_the_selected_identity_and_disk_state() {
    let dir = tempfile::tempdir().unwrap();
    let old_path = dir.path().join("old.flac");
    let new_path = dir.path().join("new.flac");
    std::fs::write(&new_path, b"returned").unwrap();
    let db = crate::db::Db::open_in_memory().unwrap();
    let conn = db.conn();
    seed_track(conn, 1, &old_path, None, None, None);

    conn.execute(
        "UPDATE tracks SET path=?1 WHERE id=1",
        [new_path.to_string_lossy().to_string()],
    )
    .unwrap();
    assert!(!mark_track_missing_if_current(&db, 1, &old_path).unwrap());
    assert_eq!(missing_state(conn, 1), (None, None));

    assert!(!mark_track_missing_if_current(&db, 1, &new_path).unwrap());
    assert_eq!(missing_state(conn, 1), (None, None));
}
