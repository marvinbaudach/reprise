use super::sync_log::{
    close_orphaned_runs, deviations, finish_run, note_deviation, recent_runs, start_run, summarize,
    update_planned, Deviation, DeviationKind, RunCounters, RunOutcome, RunStart, RunSummary,
    GLOBAL_RETAINED_RUNS, RETAINED_RUNS,
};
use crate::device_sync::machine::SyncOutcome;

fn database() -> crate::db::Db {
    crate::db::Db::open_in_memory().unwrap()
}

#[test]
fn mtp_20_a_running_run_accepts_its_real_planned_count_without_changing_identity() {
    let conn = database();
    let mut begin = start();
    begin.planned = 0;
    let run = start_run(&conn, &begin).unwrap();

    update_planned(&conn, run, 37).unwrap();

    let loaded = recent_runs(&conn, 10).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].id, run);
    assert_eq!(loaded[0].planned, 37);
    assert_eq!(loaded[0].outcome, RunOutcome::Running);
}

fn start() -> RunStart {
    RunStart {
        device_serial: "mtp".into(),
        device_name: "Phone".into(),
        transfer_profile: "opus_160".into(),
        started_at: 1_785_183_239,
        planned: 200,
    }
}

fn summary(copied: u32, skipped: u32, failed: u32) -> RunSummary {
    RunSummary {
        finished_at: 1_785_183_899,
        outcome: RunOutcome::Completed,
        copied,
        skipped,
        deleted: 0,
        failed,
        listens_applied: 7,
        ratings_applied: 3,
        bytes_copied: 512_000_000,
        detail: None,
    }
}

#[test]
fn mtp_20_a_run_records_its_balance_and_every_deviation() {
    let conn = database();
    let run = start_run(&conn, &start()).unwrap();

    note_deviation(
        &conn,
        run,
        &Deviation {
            kind: DeviationKind::Skipped,
            track_id: Some(7),
            device_path: "Music/Reprise/Artist/Track.opus".into(),
            detail: "device full".into(),
        },
    )
    .unwrap();
    note_deviation(
        &conn,
        run,
        &Deviation {
            kind: DeviationKind::Failed,
            track_id: Some(9),
            device_path: "Music/Reprise/Artist/Other.opus".into(),
            detail: "transcode exited with 1".into(),
        },
    )
    .unwrap();
    finish_run(&conn, run, &summary(198, 1, 1)).unwrap();

    let runs = recent_runs(&conn, 10).unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].planned, 200);
    assert_eq!(runs[0].copied, 198);
    assert_eq!(runs[0].skipped, 1);
    assert_eq!(runs[0].failed, 1);
    assert_eq!(runs[0].listens_applied, 7);
    assert_eq!(runs[0].ratings_applied, 3);
    assert_eq!(runs[0].outcome, RunOutcome::Completed);
    assert_eq!(runs[0].transfer_profile, "opus_160");

    let found = deviations(&conn, run).unwrap();
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].kind, DeviationKind::Skipped);
    assert_eq!(found[0].track_id, Some(7));
    assert_eq!(found[1].detail, "transcode exited with 1");
}

#[test]
fn mtp_20_a_successful_file_leaves_no_entry_behind() {
    let conn = database();
    let run = start_run(&conn, &start()).unwrap();
    finish_run(&conn, run, &summary(200, 0, 0)).unwrap();

    // The balance covers the 200 copies; only deviations are itemized.
    assert!(deviations(&conn, run).unwrap().is_empty());
    assert_eq!(recent_runs(&conn, 10).unwrap()[0].copied, 200);
}

#[test]
fn mtp_20_starting_a_run_does_not_interrupt_another_devices_live_run() {
    let conn = database();
    let mut first_start = start();
    first_start.device_serial = "first".into();
    let first = start_run(&conn, &first_start).unwrap();

    let mut second_start = start();
    second_start.device_serial = "second".into();
    let second = start_run(&conn, &second_start).unwrap();

    let runs = recent_runs(&conn, 10).unwrap();
    let still_live = runs.iter().find(|run| run.id == first).unwrap();
    assert_eq!(still_live.outcome, RunOutcome::Running);
    assert!(still_live.finished_at.is_none());
    assert_eq!(
        runs.iter().find(|run| run.id == second).unwrap().outcome,
        RunOutcome::Running
    );
}

#[test]
fn mtp_20_startup_sweep_closes_an_orphan_with_an_end_time() {
    let conn = database();
    let orphan = start_run(&conn, &start()).unwrap();

    assert_eq!(close_orphaned_runs(&conn).unwrap(), 1);

    let runs = recent_runs(&conn, 10).unwrap();
    let closed = runs.iter().find(|run| run.id == orphan).unwrap();
    assert_eq!(closed.outcome, RunOutcome::Interrupted);
    assert!(closed.finished_at.is_some());
}

#[test]
fn mtp_20_startup_sweep_reports_zero_without_touching_a_closed_run() {
    let conn = database();
    let run = start_run(&conn, &start()).unwrap();
    finish_run(&conn, run, &summary(200, 0, 0)).unwrap();

    assert_eq!(close_orphaned_runs(&conn).unwrap(), 0);

    let closed = recent_runs(&conn, 10).unwrap().remove(0);
    assert_eq!(closed.id, run);
    assert_eq!(closed.outcome, RunOutcome::Completed);
    assert_eq!(closed.finished_at, Some(1_785_183_899));
}

#[test]
fn mtp_20_a_running_sync_is_visible_before_it_finishes() {
    let conn = database();
    let run = start_run(&conn, &start()).unwrap();

    let runs = recent_runs(&conn, 10).unwrap();

    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].id, run);
    assert_eq!(runs[0].outcome, RunOutcome::Running);
    assert_eq!(runs[0].planned, 200);
}

#[test]
fn mtp_20_only_the_most_recent_runs_are_kept() {
    let conn = database();
    let mut first = None;
    for index in 0..(RETAINED_RUNS + 5) {
        let mut begin = start();
        begin.started_at += index as i64;
        let run = start_run(&conn, &begin).unwrap();
        if index == 0 {
            first = Some(run);
        }
        note_deviation(
            &conn,
            run,
            &Deviation {
                kind: DeviationKind::Deleted,
                track_id: None,
                device_path: "Music/Reprise/gone.opus".into(),
                detail: "no longer selected".into(),
            },
        )
        .unwrap();
        finish_run(&conn, run, &summary(1, 0, 0)).unwrap();
    }

    let runs = recent_runs(&conn, 1_000).unwrap();

    assert_eq!(runs.len(), RETAINED_RUNS);
    assert!(
        !runs.iter().any(|run| Some(run.id) == first),
        "the oldest run must age out"
    );
    assert!(
        deviations(&conn, first.unwrap()).unwrap().is_empty(),
        "an aged-out run must not leave its deviations behind"
    );
}

#[test]
fn mtp_20_one_noisy_device_does_not_evict_another_devices_oldest_run() {
    let conn = database();
    let mut quiet_start = start();
    quiet_start.device_serial = "quiet".into();
    quiet_start.started_at = 1;
    let quiet = start_run(&conn, &quiet_start).unwrap();
    finish_run(&conn, quiet, &summary(1, 0, 0)).unwrap();

    for index in 0..(RETAINED_RUNS + 5) {
        let mut noisy_start = start();
        noisy_start.device_serial = "noisy".into();
        noisy_start.started_at = 2 + index as i64;
        let run = start_run(&conn, &noisy_start).unwrap();
        finish_run(&conn, run, &summary(1, 0, 0)).unwrap();
    }

    let runs = recent_runs(&conn, GLOBAL_RETAINED_RUNS + 1).unwrap();
    assert!(
        runs.iter().any(|run| run.id == quiet),
        "another device's retained run must stay available"
    );
    assert_eq!(
        runs.iter()
            .filter(|run| run.device_serial == "noisy")
            .count(),
        RETAINED_RUNS
    );
}

#[test]
fn mtp_20_volatile_device_identities_cannot_exceed_the_global_ceiling() {
    let conn = database();
    let mut oldest = None;
    for index in 0..(GLOBAL_RETAINED_RUNS + 5) {
        let mut begin = start();
        begin.device_serial = format!("mtp://connection/{index}");
        begin.started_at = index as i64;
        let run = start_run(&conn, &begin).unwrap();
        if index == 0 {
            oldest = Some(run);
            note_deviation(
                &conn,
                run,
                &Deviation {
                    kind: DeviationKind::Failed,
                    track_id: None,
                    device_path: "Music/Reprise/old.opus".into(),
                    detail: "old connection failed".into(),
                },
            )
            .unwrap();
        }
        finish_run(&conn, run, &summary(1, 0, 0)).unwrap();
    }

    let runs = recent_runs(&conn, GLOBAL_RETAINED_RUNS + 10).unwrap();
    assert_eq!(runs.len(), GLOBAL_RETAINED_RUNS);
    assert!(
        !runs.iter().any(|run| Some(run.id) == oldest),
        "the oldest volatile-identity run must age out"
    );
    assert!(
        deviations(&conn, oldest.unwrap()).unwrap().is_empty(),
        "the global ceiling must age deviations out with their run"
    );
}

#[test]
fn mtp_20_runs_are_reported_newest_first() {
    let conn = database();
    let mut older = start();
    older.started_at = 1_000;
    let first = start_run(&conn, &older).unwrap();
    let mut newer = start();
    newer.started_at = 2_000;
    let second = start_run(&conn, &newer).unwrap();
    finish_run(&conn, second, &summary(1, 0, 0)).unwrap();

    let runs = recent_runs(&conn, 10).unwrap();

    assert_eq!(runs[0].id, second);
    assert_eq!(runs[1].id, first);
}

fn counters() -> RunCounters {
    RunCounters {
        copied: 198,
        skipped: 1,
        deleted: 3,
        failed: 1,
        listens_applied: 7,
        ratings_applied: 3,
        bytes_copied: 12_345,
    }
}

#[test]
fn mtp_20_a_completed_run_summarizes_without_a_reason() {
    let summary = summarize(
        &SyncOutcome::Completed {
            verified_sources: Vec::new(),
        },
        counters(),
        99,
    );

    assert_eq!(summary.outcome, RunOutcome::Completed);
    assert_eq!(summary.detail, None);
    assert_eq!(summary.copied, 198);
    assert_eq!(summary.deleted, 3);
    assert_eq!(summary.listens_applied, 7);
    assert_eq!(summary.ratings_applied, 3);
    assert_eq!(summary.bytes_copied, 12_345);
    assert_eq!(summary.finished_at, 99);
}

#[test]
fn mtp_20_a_cancelled_run_is_recorded_as_cancelled() {
    let summary = summarize(&SyncOutcome::Cancelled, counters(), 99);

    assert_eq!(summary.outcome, RunOutcome::Cancelled);
    assert_eq!(summary.detail, None);
}

#[test]
fn mtp_20_a_failed_run_keeps_the_reason_it_failed_for() {
    let summary = summarize(
        &SyncOutcome::Failed {
            terminal_error: Some("device disconnected".into()),
            failed_tracks: vec![1, 2],
            verified_sources: Vec::new(),
        },
        counters(),
        99,
    );

    assert_eq!(summary.outcome, RunOutcome::Failed);
    assert_eq!(summary.detail.as_deref(), Some("device disconnected"));
}

#[test]
fn mtp_20_a_run_that_only_lost_tracks_says_so_instead_of_staying_silent() {
    // No stage failed outright — individual tracks did. Without a note the
    // entry would look like an unexplained failure.
    let summary = summarize(
        &SyncOutcome::Failed {
            terminal_error: None,
            failed_tracks: vec![1, 2, 3],
            verified_sources: Vec::new(),
        },
        counters(),
        99,
    );

    assert_eq!(summary.outcome, RunOutcome::Failed);
    assert_eq!(summary.detail.as_deref(), Some("3 tracks failed"));
}
