use reprise_core::library::settings::{self, AutoCleanSetting};
use reprise_core::models::MissingReason;
use reprise_core::queries::MissingGroupKind;

use super::*;

#[test]
fn missing_group_copy_keeps_unknown_actionless_and_honest() {
    let copy = group_copy(&MissingGroupKind::Unavailable { mount_point: None }, 3);
    assert_eq!(copy.title, "On unavailable drive");
    assert_eq!(copy.meta, "unknown location — 3 tracks");
    assert_eq!(copy.note, "will be verified on next scan");
    assert!(!copy.actionable);
}

// UX SET-4: enabling destructive auto-clean applies immediately but an
// existing overdue backlog requires the named cascade choice.
#[test]
fn set_4_auto_clean_activation_names_cascade_and_can_start_today() {
    let mut conn = reprise_core::db::open_migrated(None).unwrap();
    conn.execute(
        "INSERT INTO tracks (id,path,title,artist,added_at,missing_since,missing_reason) \
         VALUES (1,'/gone.flac','Gone','',0,1,'deleted')",
        [],
    )
    .unwrap();
    let now = 40 * 86_400;

    let plan = activate_auto_clean(&conn, AutoCleanSetting::Days(30), now).unwrap();
    assert_eq!(
        plan,
        AutoCleanActivation::ConfirmBacklog {
            days: 30,
            eligible: 1
        }
    );
    assert_eq!(
        settings::get_missing_auto_clean(&conn),
        AutoCleanSetting::Days(30),
        "the setting itself takes effect immediately"
    );
    let body = auto_clean_confirmation_body(1, 30);
    assert!(body.contains("ratings and listening history go with them"));

    start_auto_clean_counting_today(&conn, now).unwrap();
    assert_eq!(settings::get_auto_clean_armed_at(&conn).unwrap(), Some(now));

    assert_eq!(
        remove_auto_clean_backlog_now(&mut conn, now).unwrap(),
        vec![1]
    );
    assert_eq!(settings::get_auto_clean_armed_at(&conn).unwrap(), Some(now));
}

#[test]
fn deleted_card_is_the_only_actionable_missing_group() {
    let deleted = group_copy(&MissingGroupKind::Deleted, 2);
    assert!(deleted.actionable);
    assert!(remove_confirmation_body(2).contains("ratings and listening history go with them"));
    let unavailable = group_copy(
        &MissingGroupKind::Unavailable {
            mount_point: Some("/media/NAS".into()),
        },
        2,
    );
    assert!(!unavailable.actionable);
}

#[test]
fn locate_actions_cover_deleted_and_unknown_but_never_unmounted_tracks() {
    assert_eq!(
        locate_actions(&MissingGroupKind::Deleted),
        LocateActions {
            row: true,
            folder: true,
        }
    );
    assert_eq!(
        locate_actions(&MissingGroupKind::Unavailable { mount_point: None }),
        LocateActions {
            row: true,
            folder: false,
        }
    );
    assert_eq!(
        locate_actions(&MissingGroupKind::Unavailable {
            mount_point: Some("/media/NAS".into()),
        }),
        LocateActions {
            row: false,
            folder: false,
        }
    );
}

#[test]
fn missing_since_copy_uses_a_short_calendar_date() {
    assert_eq!(missing_since_label(1_752_278_400), "since Jul 12");
    assert_eq!(MissingReason::Deleted.as_str(), "deleted");
}

#[test]
fn startup_purge_commits_a_tombstone_left_by_a_closed_window() {
    let conn = reprise_core::db::open_migrated(None).unwrap();
    conn.execute(
        "INSERT INTO tracks (id,path,title,artist,added_at,removed_at) \
         VALUES (1,'/removed.flac','Removed','',0,100)",
        [],
    )
    .unwrap();
    let conn = Rc::new(RefCell::new(conn));
    assert_eq!(purge_startup_tombstones(&conn).unwrap(), vec![1]);
    let count: i64 = conn
        .borrow()
        .query_row("SELECT count(*) FROM tracks", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn stale_tombstone_request_skips_track_resurrected_while_dialog_was_open() {
    let mut conn = reprise_core::db::open_migrated(None).unwrap();
    for id in [1, 2] {
        conn.execute(
            "INSERT INTO tracks \
             (id,path,title,artist,added_at,missing_since,missing_reason) \
             VALUES (?1,?2,?3,'',0,1,'deleted')",
            rusqlite::params![id, format!("/x/{id}.flac"), format!("Track {id}"),],
        )
        .unwrap();
    }
    let stale_ids = vec![1, 2];

    // Simulate a scan/mount event proving track 1 is present while the
    // confirmation dialog remains open.
    conn.execute(
        "UPDATE tracks SET missing_since=NULL,missing_reason=NULL WHERE id=1",
        [],
    )
    .unwrap();

    assert_eq!(
        tombstone_still_deleted(&mut conn, &stale_ids, 100).unwrap(),
        vec![2]
    );

    let states: Vec<(i64, Option<i64>)> = conn
        .prepare("SELECT id,removed_at FROM tracks ORDER BY id")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        states,
        vec![(1, None), (2, Some(100))],
        "resurrected tracks survive; tracks still proven deleted are tombstoned"
    );
}
