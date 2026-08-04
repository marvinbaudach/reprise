use std::fs;

use super::*;

fn tick(suspicious: bool, rows: usize, now_ms: u64) -> TickInput {
    TickInput {
        suspicious,
        rows,
        now_ms,
    }
}

#[test]
fn two_suspicious_ticks_confirm_only_once_per_episode() {
    let mut state = WatchdogState::default();
    assert_eq!(state.tick(tick(true, 0, 10), true), TickDecision::default());
    let confirmed = state.tick(tick(true, 0, 20), true);
    assert!(confirmed.confirmed);
    assert!(confirmed.request_self_heal);
    assert!(!state.tick(tick(true, 0, 30), true).confirmed);
    assert!(!state.tick(tick(true, 0, 40), true).confirmed);
}

#[test]
fn a_transient_suspicious_tick_never_confirms() {
    let mut state = WatchdogState::default();
    state.tick(tick(true, 0, 10), true);
    state.tick(tick(false, 0, 20), true);
    assert!(!state.tick(tick(true, 0, 30), true).confirmed);
}

#[test]
fn recovery_rearms_the_next_episode() {
    let mut state = WatchdogState::default();
    state.tick(tick(true, 0, 100), false);
    assert!(state.tick(tick(true, 0, 200), false).confirmed);
    let recovery = state.tick(tick(false, 3, 350), false).recovered.unwrap();
    assert_eq!(
        recovery,
        Recovery {
            after_ms: 250,
            rows: 3
        }
    );
    state.tick(tick(true, 0, 400), false);
    assert!(state.tick(tick(true, 0, 500), false).confirmed);
}

#[test]
fn self_heal_outcome_is_decided_on_the_following_tick() {
    let mut worked = WatchdogState::default();
    worked.tick(tick(true, 0, 10), true);
    worked.tick(tick(true, 0, 20), true);
    assert_eq!(
        worked.tick(tick(false, 1, 30), true).self_heal_outcome,
        Some(RecoveryOutcome::Worked)
    );

    let mut failed = WatchdogState::default();
    failed.tick(tick(true, 0, 10), true);
    failed.tick(tick(true, 0, 20), true);
    assert_eq!(
        failed.tick(tick(true, 0, 30), true).self_heal_outcome,
        Some(RecoveryOutcome::Failed)
    );
}

#[test]
fn diagnose_only_disables_the_self_heal_request() {
    assert!(!self_heal_enabled(Some("diagnose-only")));
    assert!(self_heal_enabled(None));
    let mut state = WatchdogState::default();
    state.tick(tick(true, 0, 10), false);
    assert!(!state.tick(tick(true, 0, 20), false).request_self_heal);
}

fn snapshot() -> DumpSnapshot {
    DumpSnapshot {
        app_version: "0.1.1".into(),
        git_sha: "abc123".into(),
        wall_clock: "2026-08-04T13:22:00+02:00".into(),
        n_items: 1_821,
        stack_page: "list".into(),
        source: "library".into(),
        sort_field: "artist".into(),
        sort_dir: "asc".into(),
        filter: "".into(),
        browse: "BrowseFilter { genre: None }".into(),
        exclude_ai: false,
        adjustment_value: 156.0,
        adjustment_lower: 0.0,
        adjustment_upper: 10_000.0,
        adjustment_page_size: 500.0,
        column_mapped: true,
        column_realized: true,
        column_visible: true,
        column_opacity: 1.0,
        column_width: 900,
        column_height: 600,
        scrolled_width: 900,
        scrolled_height: 600,
        window_query_error_count: 1,
        last_window_query_error: Some("database busy".into()),
        gdk_backend: "x11".into(),
        gsk_renderer: "cairo".into(),
        animations_enabled: true,
        trail: vec![
            "1ms QuerySet total=1821".into(),
            "2ms RowLoss n_items=1821".into(),
        ],
    }
}

#[test]
fn dump_rendering_contains_every_field_and_ordered_trail() {
    let text = render_dump(&snapshot());
    for field in [
        "app_version=0.1.1",
        "git_sha=abc123",
        "timestamp=2026-08-04",
        "n_items=1821",
        "stack_page=list",
        "source=library",
        "sort_field=artist",
        "sort_dir=asc",
        "filter=",
        "browse=BrowseFilter",
        "exclude_ai=false",
        "vadjustment.value=156.000",
        "vadjustment.lower=0.000",
        "vadjustment.upper=10000.000",
        "vadjustment.page_size=500.000",
        "column_view.is_mapped=true",
        "column_view.is_realized=true",
        "column_view.is_visible=true",
        "column_view.opacity=1.000",
        "column_view.width=900",
        "column_view.height=600",
        "scrolled_window.width=900",
        "scrolled_window.height=600",
        "window_query_error.count=1",
        "window_query_error.last=database busy",
        "GDK_BACKEND=x11",
        "GSK_RENDERER=cairo",
        "animations_enabled=true",
        "trail:",
    ] {
        assert!(text.contains(field), "missing {field} in:\n{text}");
    }
    assert!(text.find("1ms QuerySet").unwrap() < text.find("2ms RowLoss").unwrap());
}

#[test]
fn retention_keeps_the_newest_twenty_row_loss_dumps() {
    let temp = tempfile::tempdir().unwrap();
    for second in 0..23 {
        fs::write(
            temp.path()
                .join(format!("row-loss-20260804-1322{second:02}.log")),
            second.to_string(),
        )
        .unwrap();
    }
    fs::write(temp.path().join("unrelated.txt"), "keep").unwrap();

    retain_newest(temp.path(), 20).unwrap();
    let mut dumps = fs::read_dir(temp.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("row-loss-"))
        .map(|entry| entry.file_name())
        .collect::<Vec<_>>();
    dumps.sort();
    assert_eq!(dumps.len(), 20);
    assert_eq!(dumps[0].to_string_lossy(), "row-loss-20260804-132203.log");
    assert!(temp.path().join("unrelated.txt").exists());
}

#[test]
fn self_heal_outcome_is_appended_as_a_field_and_trail_line() {
    let temp = tempfile::tempdir().unwrap();
    let path = write_dump_file(temp.path(), "20260804-132200", &snapshot()).unwrap();
    append_self_heal_outcome(
        &path,
        RecoveryOutcome::Failed,
        "4000ms SelfHeal recovery=failed",
    )
    .unwrap();

    let text = fs::read_to_string(path).unwrap();
    assert!(text.contains("self_heal.recovery=failed\n"));
    assert!(text.ends_with("4000ms SelfHeal recovery=failed\n"));
}
