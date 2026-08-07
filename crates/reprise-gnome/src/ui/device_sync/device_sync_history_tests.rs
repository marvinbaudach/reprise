use super::*;
use reprise_core::device_sync::sync_log::{DeviationKind, RunOutcome};

fn run(outcome: RunOutcome) -> RunRecord {
    RunRecord {
        id: 1,
        device_serial: "mtp".into(),
        device_name: "Phone".into(),
        transfer_profile: "opus_160".into(),
        // 2026-07-27 20:13:59 UTC
        started_at: 1_785_183_239,
        finished_at: Some(1_785_183_899),
        outcome,
        planned: 200,
        copied: 198,
        skipped: 1,
        deleted: 3,
        failed: 1,
        bytes_copied: 512_000_000,
        detail: None,
    }
}

#[test]
fn mtp_20_a_run_headline_names_when_it_ran_and_how_it_ended() {
    let headline = run_headline(&run(RunOutcome::Completed));

    assert!(headline.contains("2026"), "{headline}");
    assert!(headline.contains("Completed"), "{headline}");
}

#[test]
fn mtp_20_a_run_with_copies_and_failures_reports_both() {
    let mut partial = run(RunOutcome::Completed);
    partial.planned = 5;
    partial.copied = 3;
    partial.skipped = 0;
    partial.failed = 2;
    partial.deleted = 0;

    let balance = run_balance(&partial);

    assert_eq!(balance, "3 of 5 copied · 2 failed");
}

#[test]
fn mtp_20_a_delete_only_run_omits_a_zero_copy_count() {
    let mut deletion = run(RunOutcome::Completed);
    deletion.planned = 0;
    deletion.copied = 0;
    deletion.skipped = 0;
    deletion.failed = 0;
    deletion.deleted = 4;

    let balance = run_balance(&deletion);

    assert_eq!(balance, "4 removed");
    assert!(!balance.contains("0 of 0 copied"), "{balance}");
}

#[test]
fn mtp_20_an_empty_run_has_a_non_blank_balance() {
    let mut empty = run(RunOutcome::Completed);
    empty.planned = 0;
    empty.copied = 0;
    empty.skipped = 0;
    empty.failed = 0;
    empty.deleted = 0;

    let balance = run_balance(&empty);

    assert_eq!(balance, "Nothing to transfer");
    assert!(!balance.trim().is_empty());
}

#[test]
fn mtp_20_a_copy_only_run_reports_its_copy_count() {
    let mut copy = run(RunOutcome::Completed);
    copy.planned = 5;
    copy.copied = 5;
    copy.skipped = 0;
    copy.failed = 0;
    copy.deleted = 0;

    let balance = run_balance(&copy);

    assert_eq!(balance, "5 of 5 copied");
}

#[test]
fn mtp_20_an_interrupted_run_is_named_as_such_rather_than_looking_finished() {
    let mut lost = run(RunOutcome::Interrupted);
    lost.finished_at = None;

    let headline = run_headline(&lost);

    assert!(headline.contains("Interrupted"), "{headline}");
}

#[test]
fn mtp_20_a_failure_reason_is_carried_into_the_balance() {
    let mut failed = run(RunOutcome::Failed);
    failed.detail = Some("device disconnected".into());

    let balance = run_balance(&failed);

    assert!(balance.contains("device disconnected"), "{balance}");
}

#[test]
fn mtp_20_a_deviation_names_its_kind_the_file_and_the_reason() {
    let line = deviation_line(&Deviation {
        kind: DeviationKind::Failed,
        track_id: Some(7),
        device_path: "Music/Reprise/Artist/Track.opus".into(),
        detail: "copy failed: device is full".into(),
    });

    assert!(line.contains("Failed"), "{line}");
    assert!(line.contains("Music/Reprise/Artist/Track.opus"), "{line}");
    assert!(line.contains("device is full"), "{line}");
}

#[test]
fn mtp_20_a_removal_reads_as_removed_rather_than_as_an_error() {
    let line = deviation_line(&Deviation {
        kind: DeviationKind::Deleted,
        track_id: None,
        device_path: "Music/Reprise/old.opus".into(),
        detail: "no longer covered by the selection".into(),
    });

    assert!(line.starts_with("Removed"), "{line}");
}
