//! Live refresh of external database changes (multi-frontend-core, package C).
//!
//! A second process — the CLI or the MCP server — writes to the same SQLite
//! file through the same core facades. Each mutation appends a `change_log` row
//! in its own transaction (P0) and bumps `PRAGMA data_version`. This runtime
//! turns those foreign writes into silent, coarse UI refreshes, so CLI/MCP
//! changes appear in the running app without a restart (UX rules EXT-1a..EXT-4).
//!
//! The shape mirrors `ui/scan/scan_watcher.rs`: a background source — here
//! [`reprise_core::events::Notifier`], its own thread and own connection —
//! feeds an `async_channel`, and a `glib::spawn_future_local` drain applies the
//! result on the GTK main thread. The read + filter + coalesce step is the
//! pure, headless-testable [`read_and_plan`] / [`coalesce::plan_for`]; only the
//! async plumbing needs a display, and it is covered by the single `ext_1a_…`
//! test.
//!
//! Failure is never fatal (the watcher's degradation shape): if the read
//! connection cannot be opened or the notifier cannot start, the app runs on
//! without live updates — no panic, no dialog.
//!
//! This module takes ids/closures, not widgets: per `ui/mod.rs`'s containment
//! policy, a non-widgetry module keeps GTK types out of its signatures. The
//! caller (`window_runtime_wiring`) supplies the closure that maps a
//! [`RefreshPlan`] onto `Sidebar::refresh` / `TrackList::reload`.

use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

use gtk4::glib;
use reprise_core::events::{self, WriterToken};
use rusqlite::Connection;

mod coalesce;
pub(in crate::ui) use coalesce::RefreshPlan;

#[cfg(test)]
mod tests;

/// Reads every `change_log` row after `since`, advances the cursor past all of
/// them, and folds the foreign ones into a coarse [`RefreshPlan`]. Returns the
/// new cursor and the plan. Pure and synchronous — the headless tests drive it
/// directly; [`start`]'s wake callback is a thin wrapper.
///
/// The cursor advances past *every* row read, the app's own writes included, so
/// a burst of the app's own mutations never forces a growing re-scan on the
/// next wake; only the coarse plan drops them (via `excluded`).
pub(in crate::ui) fn read_and_plan(
    conn: &Connection,
    since: i64,
    excluded: Option<WriterToken>,
) -> (i64, RefreshPlan) {
    match events::read_since(conn, since, None) {
        Ok(changes) => {
            let cursor = changes.last().map_or(since, |change| change.id);
            (cursor, coalesce::plan_for(&changes, excluded))
        }
        Err(error) => {
            tracing::warn!(%error, "external-changes: failed to read the change log");
            (since, RefreshPlan::default())
        }
    }
}

/// The current highest `change_log` id — the cursor baseline at startup, so
/// history already reflected in the freshly loaded UI is not replayed on the
/// first wake.
fn current_cursor(conn: &Connection) -> i64 {
    match events::read_since(conn, 0, None) {
        Ok(changes) => changes.last().map_or(0, |change| change.id),
        Err(error) => {
            tracing::warn!(
                %error,
                "external-changes: cannot read the change-log baseline; starting at 0"
            );
            0
        }
    }
}

/// Starts the live-refresh runtime for the database at `db_path`.
///
/// `excluded` is the app's own writer token ([`events::writer_token`]), so the
/// app's already-self-refreshed writes are filtered out. `apply` runs on the
/// GTK main thread with each coalesced [`RefreshPlan`]; wire it to
/// `Sidebar::refresh` / `TrackList::reload`.
///
/// The spawned drain future owns the notifier handle, so the runtime lives for
/// the application's lifetime with no caller-side storage and stops when the
/// main loop is torn down at exit. It returns without effect — after a log —
/// when the read connection or the notifier cannot start: the app keeps
/// working, just without live updates.
pub(in crate::ui) fn start(
    db_path: &Path,
    excluded: Option<WriterToken>,
    apply: Rc<dyn Fn(RefreshPlan)>,
) {
    let read_conn = match reprise_core::db::open(Some(db_path)) {
        Ok(conn) => conn,
        Err(error) => {
            tracing::warn!(
                %error,
                db = %db_path.display(),
                "external-changes: cannot open a read connection; continuing without live updates"
            );
            return;
        }
    };

    let cursor = Cell::new(current_cursor(&read_conn));
    let (sender, receiver) = async_channel::unbounded::<RefreshPlan>();

    // Runs on the notifier's own thread: read the new rows, advance the cursor,
    // and forward a coarse plan. No shared `RefCell`/`Rc` crosses the thread
    // boundary — the read connection, cursor, and sender are all owned here.
    let on_wake = move || {
        let (next, plan) = read_and_plan(&read_conn, cursor.get(), excluded);
        cursor.set(next);
        if plan.is_empty() {
            return;
        }
        if let Err(error) = sender.send_blocking(plan) {
            tracing::warn!(%error, "external-changes: refresh receiver is gone");
        }
    };

    let Some(notifier) = events::Notifier::start(db_path, on_wake) else {
        // `on_wake` — and with it the sender — drops here, closing the channel;
        // there is nothing to drain. The app runs without live updates.
        tracing::info!("external-changes: notifier unavailable; live updates disabled");
        return;
    };

    glib::spawn_future_local(async move {
        // Own the notifier for the app's lifetime: the future is suspended on
        // `recv` until the main loop is torn down at exit, which drops it and
        // stops the background thread.
        let _notifier = notifier;
        while let Ok(plan) = receiver.recv().await {
            apply(plan);
        }
        tracing::debug!("external-changes: drain loop ended");
    });
}
