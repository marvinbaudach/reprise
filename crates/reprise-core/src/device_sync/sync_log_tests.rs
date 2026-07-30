use super::sync_log::{
    deviations, finish_run, note_deviation, recent_runs, start_run, summarize, Deviation,
    DeviationKind, RunCounters, RunOutcome, RunStart, RunSummary, RETAINED_RUNS,
};
use crate::device_sync::machine::SyncOutcome;

fn database() -> crate::db::Db {
    crate::db::Db::open_in_memory().unwrap()
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
fn mtp_20_a_run_that_never_finished_stays_visible_as_interrupted() {
    let conn = database();
    let abandoned = start_run(&conn, &start()).unwrap();

    // The app died mid-sync: no finish_run ever arrives, and the next run
    // starts. The lost run must not linger as "running" or disappear.
    let next = start_run(&conn, &start()).unwrap();
    finish_run(&conn, next, &summary(200, 0, 0)).unwrap();

    let runs = recent_runs(&conn, 10).unwrap();
    let lost = runs.iter().find(|run| run.id == abandoned).unwrap();
    assert_eq!(lost.outcome, RunOutcome::Interrupted);
    assert!(lost.finished_at.is_none());
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
        },
        counters(),
        99,
    );

    assert_eq!(summary.outcome, RunOutcome::Failed);
    assert_eq!(summary.detail.as_deref(), Some("3 tracks failed"));
}
