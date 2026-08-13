//! Forced end-to-end proof of the row-loss diagnostic dump.
//!
//! Run only on an isolated display:
//! `dbus-run-session -- xvfb-run -a env XDG_DATA_HOME=$(mktemp -d)
//! XDG_CACHE_HOME=$(mktemp -d) GDK_BACKEND=x11 WAYLAND_DISPLAY=
//! REPRISE_AUDIO_SINK=fakesink cargo run -p reprise-gnome --example
//! row_loss_dump_repro`.
//!
//! The example keeps a real 1,821-item GListModel while its injected realised-
//! row probe reports zero. It drives the same pure watchdog state and dump
//! renderer as production, then exits non-zero unless exactly one complete,
//! ordered diagnostic survives.

use gtk4::gio;
use gtk4::prelude::*;

#[allow(dead_code)]
#[path = "../src/ui/track_list/diagnostic_trail.rs"]
mod diagnostic_trail;
#[allow(dead_code)]
#[path = "../src/ui/track_list/row_loss_watchdog_state.rs"]
mod row_loss_watchdog_state;

use diagnostic_trail::{DiagnosticTrail, Event};
use row_loss_watchdog_state::{
    append_self_heal_outcome, write_dump_file, DumpSnapshot, RecoveryOutcome, TickInput,
    WatchdogState,
};

const TOTAL: u32 = 1_821;

fn input(rows: usize, now_ms: u64) -> TickInput {
    TickInput {
        suspicious: rows == 0,
        row_widgets_present: rows,
        row_widgets_allocated: rows,
        now_ms,
    }
}

fn snapshot(trail: &DiagnosticTrail) -> DumpSnapshot {
    DumpSnapshot {
        app_version: env!("CARGO_PKG_VERSION").into(),
        git_sha: option_env!("REPRISE_GIT_SHA").unwrap_or("<unknown>").into(),
        wall_clock: "2026-08-04T13:22:00+02:00".into(),
        n_items: TOTAL,
        row_widgets_present: 0,
        row_widgets_allocated: 0,
        stack_page: "list".into(),
        source: "Music".into(),
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
        last_window_query_error: Some("forced query diagnostic".into()),
        gdk_backend: std::env::var("GDK_BACKEND").unwrap_or_else(|_| "<unset>".into()),
        gsk_renderer: std::env::var("GSK_RENDERER").unwrap_or_else(|_| "<unset>".into()),
        animations_enabled: true,
        trail: trail.snapshot(),
    }
}

fn assert_complete(text: &str) {
    for field in [
        "app_version=",
        "git_sha=",
        "timestamp=2026-08-04T13:22:00+02:00",
        "n_items=1821",
        "row_widgets_present=0",
        "row_widgets_allocated=0",
        "stack_page=list",
        "source=Music",
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
        "window_query_error.last=forced query diagnostic",
        "GDK_BACKEND=x11",
        "GSK_RENDERER=",
        "animations_enabled=true",
        "trail:",
        "self_heal.recovery=failed",
    ] {
        assert!(text.contains(field), "missing {field} in:\n{text}");
    }

    let query = text.find("QuerySet total=1821").unwrap();
    let changed = text.find("ItemsChanged position=0").unwrap();
    let loss = text.find("RowLoss n_items=1821").unwrap();
    let self_heal = text.find("SelfHeal recovery=failed").unwrap();
    assert!(query < changed && changed < loss && loss < self_heal);
}

fn main() {
    gtk4::init().unwrap();
    diagnostic_trail::mark_process_start();

    let model = gio::ListStore::new::<gtk4::glib::BoxedAnyObject>();
    for track_id in 0..TOTAL {
        model.append(&gtk4::glib::BoxedAnyObject::new(track_id));
    }
    assert_eq!(model.n_items(), TOTAL);

    let row_counter = || 0_usize;
    let trail = DiagnosticTrail::default();
    trail.record(Event::QuerySet {
        total: model.n_items(),
        source: "Music".into(),
        sort_field: "artist".into(),
        sort_dir: "asc".into(),
        filter_len: 0,
        exclude_ai: false,
    });

    let mut state = WatchdogState::default();
    assert!(!state.tick(input(row_counter(), 2_000), true).confirmed);
    trail.record(Event::ItemsChanged {
        position: 0,
        removed: TOTAL,
        added: TOTAL,
    });
    let confirmation = state.tick(input(row_counter(), 4_000), true);
    assert!(confirmation.confirmed && confirmation.request_self_heal);
    trail.record(Event::RowLoss {
        n_items: model.n_items(),
    });

    let temp = tempfile::tempdir().unwrap();
    let path = write_dump_file(temp.path(), "20260804-132200", &snapshot(&trail)).unwrap();

    let outcome = state
        .tick(input(row_counter(), 6_000), true)
        .self_heal_outcome
        .unwrap();
    assert_eq!(outcome, RecoveryOutcome::Failed);
    trail.record(Event::SelfHeal {
        recovery: outcome.as_str().into(),
    });
    let trail_line = trail.snapshot().pop().unwrap();
    append_self_heal_outcome(&path, outcome, &trail_line).unwrap();
    assert!(!state.tick(input(row_counter(), 8_000), true).confirmed);

    let dumps = std::fs::read_dir(temp.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("row-loss-"))
        .collect::<Vec<_>>();
    assert_eq!(dumps.len(), 1, "one fault episode must write one dump");
    assert_eq!(dumps[0].path(), path);
    assert_complete(&std::fs::read_to_string(path).unwrap());
    println!("OK: one complete row-loss diagnostic dump was written");
}
