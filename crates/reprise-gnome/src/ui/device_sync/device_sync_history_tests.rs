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
fn an_unrememberable_phone_explains_what_is_tied_to_the_connection() {
    let (title, detail) = history_warning_copy(false).expect("warning copy");

    assert!(title.contains("no stable identifier"), "{title}");
    assert!(detail.contains("settings and history"), "{detail}");
    assert!(detail.contains("tied to this connection"), "{detail}");
    assert!(detail.contains("may not be found again"), "{detail}");
    assert!(!title.contains("not written"), "{title}");
    assert!(!detail.contains("stable session"), "{detail}");
    assert!(history_warning_copy(true).is_none());
}

#[test]
fn a_running_record_renders_as_live_state_not_as_a_dated_result() {
    let mut running = run(RunOutcome::Running);
    running.finished_at = None;
    running.copied = 0;
    let progress = RunningProgress {
        title: "Step 1 of 2 · Downloading 17 of 60 · 79%".into(),
        fraction: 0.79,
    };

    let copy = run_row_copy(&running, Some(&progress));

    assert!(
        copy.headline.starts_with("Running since "),
        "{}",
        copy.headline
    );
    assert!(!copy.headline.contains("2026"), "{}", copy.headline);
    assert_eq!(copy.subtitle, progress.title);
    assert_eq!(copy.percent, Some(79));
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

/// A progress tick must move the numbers, not the widgets. Rebuilding the list
/// on every `notify()` would collapse an expanded row and throw keyboard focus
/// out of it several times a second while a sync runs — the reason `sync` keeps
/// a signature at all.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mtp_20_a_progress_tick_updates_the_live_row_instead_of_rebuilding_the_list() {
    gtk4::init().expect("GTK test display");
    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let state = HistoryState::default();
    let runs = vec![(run(RunOutcome::Running), Vec::new())];

    let first = RunningProgress {
        title: "Step 1 of 2 · 17 of 60 downloaded".into(),
        fraction: 0.28,
    };
    sync(&container, &state, &runs, true, Some(&first));
    let card = container.first_child().expect("history card");
    let row_before = state
        .live
        .borrow()
        .as_ref()
        .map(|live| live.row.clone())
        .expect("a running run must produce a live row");
    assert_eq!(row_before.subtitle().as_str(), first.title);

    let later = RunningProgress {
        title: "Step 2 of 2 · 41 of 60 copied".into(),
        fraction: 0.79,
    };
    sync(&container, &state, &runs, true, Some(&later));

    assert_eq!(
        container.first_child().as_ref(),
        Some(&card),
        "the card must survive a tick that only moved the percentage"
    );
    let row_after = state
        .live
        .borrow()
        .as_ref()
        .map(|live| live.row.clone())
        .expect("the live row must still be tracked");
    assert_eq!(row_after, row_before, "the row object must be the same one");
    assert_eq!(row_after.subtitle().as_str(), later.title);
    assert_eq!(
        state
            .live
            .borrow()
            .as_ref()
            .map(|live| live.percent.label().to_string()),
        Some("79 %".to_string())
    );

    // A run that actually ended is a different list, and must be rebuilt.
    let finished = vec![(run(RunOutcome::Completed), Vec::new())];
    sync(&container, &state, &finished, true, None);
    assert_ne!(
        container.first_child().as_ref(),
        Some(&card),
        "a changed run set must rebuild the card"
    );
}
