//! GTK heartbeat and realised-row probe for track-list row loss.

use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use gtk4::prelude::*;

use super::diagnostic_trail::Event;
use super::row_loss_watchdog_state::{
    append_self_heal_outcome, retain_newest, self_heal_enabled, write_dump_file, DumpSnapshot,
    RecoveryOutcome, TickInput, WatchdogState,
};
use super::{Shared, STACK_PAGE_LIST};

const ROW_SEARCH_DEPTH: u8 = 6;

/// Returns `1` as soon as the realised widget tree contains a row, otherwise
/// `0`. The watchdog only needs presence, so stopping on the first row keeps
/// the healthy heartbeat bounded independently of viewport size.
pub(crate) fn realized_row_count(column_view: &gtk4::ColumnView) -> usize {
    usize::from(contains_row(column_view.upcast_ref(), 0))
}

fn contains_row(widget: &gtk4::Widget, depth: u8) -> bool {
    if widget.css_name() == "row" {
        return true;
    }
    if depth >= ROW_SEARCH_DEPTH {
        return false;
    }

    let mut child = widget.first_child();
    while let Some(current) = child {
        if contains_row(&current, depth + 1) {
            return true;
        }
        child = current.next_sibling();
    }
    false
}

pub(super) fn install(shared: &Rc<Shared>) {
    let shared = Rc::downgrade(shared);
    let started = Instant::now();
    let self_heal = self_heal_enabled(std::env::var("REPRISE_ROW_WATCHDOG").ok().as_deref());
    let mut state = WatchdogState::default();
    let mut dump_path: Option<PathBuf> = None;
    gtk4::glib::timeout_add_seconds_local(2, move || {
        let Some(shared) = shared.upgrade() else {
            return gtk4::glib::ControlFlow::Break;
        };
        run_tick(
            &shared,
            &mut state,
            &mut dump_path,
            started.elapsed().as_millis() as u64,
            self_heal,
        );
        gtk4::glib::ControlFlow::Continue
    });
}

fn run_tick(
    shared: &Shared,
    state: &mut WatchdogState,
    dump_path: &mut Option<PathBuf>,
    now_ms: u64,
    self_heal: bool,
) {
    let rows = realized_row_count(&shared.column_view);
    let n_items = shared.model.n_items();
    let stack_page = stack_page(shared);
    let suspicious = shared.column_view.is_mapped()
        && stack_page == STACK_PAGE_LIST
        && shared.column_view.height() > 0
        && n_items > 0
        && rows == 0;
    let decision = state.tick(
        TickInput {
            suspicious,
            rows,
            now_ms,
        },
        self_heal,
    );

    if decision.confirmed {
        shared.diagnostic_trail.record(Event::RowLoss { n_items });
        tracing::error!(
            n_items,
            stack_page,
            "track-list row loss confirmed; writing diagnostic dump"
        );
        let now = chrono::Local::now();
        let directory = gtk4::glib::user_data_dir().join("reprise/diagnostics");
        match write_dump_file(
            &directory,
            &now.format("%Y%m%d-%H%M%S").to_string(),
            &capture_dump(shared, n_items, &stack_page, now.to_rfc3339()),
        ) {
            Ok(path) => {
                *dump_path = Some(path);
                if let Err(error) = retain_newest(&directory, 20) {
                    tracing::error!(%error, "failed to apply row-loss diagnostic retention");
                }
            }
            Err(error) => {
                tracing::error!(%error, directory = %directory.display(), "failed to write row-loss diagnostic dump");
            }
        }
        if decision.request_self_heal {
            shared.model.items_changed(0, n_items, n_items);
        }
    }

    if let Some(outcome) = decision.self_heal_outcome {
        let outcome_text = outcome.as_str();
        shared.diagnostic_trail.record(Event::SelfHeal {
            recovery: outcome_text.into(),
        });
        let trail_line = shared
            .diagnostic_trail
            .snapshot()
            .last()
            .cloned()
            .unwrap_or_else(|| format!("{now_ms}ms SelfHeal recovery={outcome_text}"));
        match outcome {
            RecoveryOutcome::Worked => tracing::info!(
                recovery = outcome_text,
                "track-list row-loss self-heal outcome"
            ),
            RecoveryOutcome::Failed => tracing::error!(
                recovery = outcome_text,
                "track-list row-loss self-heal outcome"
            ),
        }
        if let Some(path) = dump_path.take() {
            if let Err(error) = append_self_heal_outcome(&path, outcome, &trail_line) {
                tracing::error!(%error, path = %path.display(), "failed to append row-loss self-heal outcome");
            } else {
                *dump_path = Some(path);
            }
        }
    }

    if let Some(recovery) = decision.recovered {
        shared.diagnostic_trail.record(Event::RowLossRecovered {
            after_ms: recovery.after_ms,
            rows: recovery.rows,
        });
        tracing::info!(
            after_ms = recovery.after_ms,
            rows = recovery.rows,
            "track-list rows recovered"
        );
        *dump_path = None;
    }
}

fn stack_page(shared: &Shared) -> String {
    shared
        .stack
        .visible_child_name()
        .map_or_else(|| "none".into(), |name| name.to_string())
}

fn capture_dump(
    shared: &Shared,
    n_items: u32,
    stack_page: &str,
    wall_clock: String,
) -> DumpSnapshot {
    let sort = shared.sort.borrow().clone();
    let source = shared.source.borrow().label();
    let filter = shared.filter.borrow().clone();
    let browse = shared.browse_filter.borrow().clone();
    let exclude_ai = shared.browse_bar.exclude_ai()
        && matches!(
            *shared.source.borrow(),
            reprise_core::view_source::ViewSource::Library
        );
    let adjustment = shared.scrolled.vadjustment();
    let (window_query_error_count, last_window_query_error) = shared.model.window_query_error();
    DumpSnapshot {
        app_version: env!("CARGO_PKG_VERSION").into(),
        git_sha: option_env!("REPRISE_GIT_SHA").unwrap_or("<unknown>").into(),
        wall_clock,
        n_items,
        stack_page: stack_page.into(),
        source,
        sort_field: sort.field,
        sort_dir: sort.dir,
        filter,
        browse: format!("{browse:?}"),
        exclude_ai,
        adjustment_value: adjustment.value(),
        adjustment_lower: adjustment.lower(),
        adjustment_upper: adjustment.upper(),
        adjustment_page_size: adjustment.page_size(),
        column_mapped: shared.column_view.is_mapped(),
        column_realized: shared.column_view.is_realized(),
        column_visible: shared.column_view.is_visible(),
        column_opacity: shared.column_view.opacity(),
        column_width: shared.column_view.width(),
        column_height: shared.column_view.height(),
        scrolled_width: shared.scrolled.width(),
        scrolled_height: shared.scrolled.height(),
        window_query_error_count,
        last_window_query_error,
        gdk_backend: environment("GDK_BACKEND"),
        gsk_renderer: environment("GSK_RENDERER"),
        animations_enabled: crate::ui::motion::animations_enabled(),
        trail: shared.diagnostic_trail.snapshot(),
    }
}

fn environment(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| "<unset>".into())
}

#[cfg(test)]
#[path = "row_loss_watchdog_display_tests.rs"]
mod display_tests;
