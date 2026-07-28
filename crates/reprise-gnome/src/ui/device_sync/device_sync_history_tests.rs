use super::*;

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
fn mtp_20_the_balance_leads_with_what_arrived_and_what_did_not() {
    let balance = run_balance(&run(RunOutcome::Completed));

    assert!(balance.contains("198 of 200 copied"), "{balance}");
    assert!(balance.contains("1 failed"), "{balance}");
    assert!(balance.contains("3 removed"), "{balance}");
}

#[test]
fn mtp_20_a_clean_run_says_so_without_listing_zeroes() {
    let mut clean = run(RunOutcome::Completed);
    clean.copied = 200;
    clean.skipped = 0;
    clean.failed = 0;
    clean.deleted = 0;

    let balance = run_balance(&clean);

    assert!(balance.contains("200 of 200 copied"), "{balance}");
    assert!(!balance.contains("failed"), "{balance}");
    assert!(!balance.contains("removed"), "{balance}");
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
