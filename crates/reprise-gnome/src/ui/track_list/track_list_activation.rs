//! Row-activation wiring for `ui::track_list`: `wire_activate` (double-click/
//! Enter routing to the `on_activate` callback) and the two helpers that
//! build the playback queue an activation starts from — `queue_ids_for_
//! activation` (the full source/sort/filter id list, capped at `QUEUE_LIMIT`)
//! and `current_queue_ids` (the fresh queue snapshot for the `Queue` source).
//! Split out of `track_list.rs`; `pub(in crate::ui)` so both `TrackList::new` and the
//! smoke hooks reach it as `track_list_activation::…`.

use std::rc::Rc;

use crate::ui::track_list::Shared;
use reprise_core::models::Track;
use reprise_core::queries;
use reprise_core::view_source::ViewSource;

/// Row activation (double-click or Enter on a focused row): resolve the
/// row's `Track` via `TrackListModel::track_at`, build its queue via
/// `queue_ids_for_activation`, and hand both to the `on_activate` callback
/// (which `window::build` routes to the player).
pub(in crate::ui) fn wire_activate(column_view: &gtk4::ColumnView, shared: &Rc<Shared>) {
    let shared = shared.clone();
    column_view.connect_activate(move |_view, position| {
        let Some(track) = shared.model.track_at(position) else {
            tracing::warn!(position, "track list activate: no item at position");
            return;
        };
        activate_track(&shared, position, &track);
    });
}

pub(in crate::ui) fn activate_track(shared: &Rc<Shared>, position: u32, track: &Track) {
    tracing::info!(path = %track.path, "activate track");
    // The user is starting playback from the table itself, so the row is
    // already on screen — arm the one-shot marker that makes the follow-up
    // now-playing selection skip the viewport centering (see the
    // `Shared::suppress_follow_scroll` doc comment).
    shared.suppress_follow_scroll.set(Some(track.id));
    if matches!(*shared.source.borrow(), ViewSource::Queue) {
        let callback = shared.on_queue_activate.borrow().clone();
        match callback {
            Some(callback) => callback(position as usize),
            None => tracing::warn!("queue activation callback is not wired"),
        }
        return;
    }
    let (ids, start_index) = queue_ids_for_activation(shared, position, track.id);
    let source = shared.source.borrow().clone();
    (shared.on_activate)(track, ids, start_index, source);
}

/// Builds the `(ids, start_index)` pair `OnActivate` carries: every track id
/// in the activated row's *current* source/sort/filter view, via
/// `queries::query_track_ids` — deliberately not `TrackListModel::
/// track_at`/`query_track_window`, which are windowed and capped at
/// `MAX_WINDOW_LIMIT` (500, sized for one `ColumnView` page) rather than a
/// whole playback queue (`QUEUE_LIMIT`, 10,000). `shared.source`/`shared.
/// sort`/`shared.filter` are read here rather than reaching into
/// `TrackListModel`'s private state (see the module doc comment on why the
/// model's `imp()` state isn't exposed) — `Shared` is the one place both the
/// model's query and this activation path already agree on the current
/// source/sort/filter, so it's the natural seam for a second query using
/// the same state. When `source` is `ViewSource::Queue`, `queue_ids` is
/// fetched fresh from `current_queue_ids` (same as `reload`) so re-
/// activating a row while already viewing the queue re-queues that exact
/// list, starting at the clicked position.
///
/// `position` doubles as `start_index` into `ids`: activation always uses
/// the unfiltered-by-cap ordering, so the row the user clicked is always the
/// same index in this ids list as it is in the `ColumnView` — as long as the
/// query wasn't truncated by `QUEUE_LIMIT` before reaching that row, which
/// `is_queue_capped` can't fully rule out but is exceedingly unlikely (a
/// 10,000+ track library with the activated row past the cap). On a query
/// failure, degrades to a single-track queue (`[activated_id]`, index 0) so
/// the click still plays something instead of silently doing nothing.
pub(in crate::ui) fn queue_ids_for_activation(
    shared: &Rc<Shared>,
    position: u32,
    activated_id: i64,
) -> (Vec<i64>, usize) {
    let sort = shared.sort.borrow().clone();
    let filter = shared.filter.borrow().clone();
    let source = shared.source.borrow().clone();
    let browse = shared.browse_filter.borrow().clone();

    let queue_ids = if matches!(source, ViewSource::Queue) {
        current_queue_ids(shared)
    } else {
        Vec::new()
    };

    let ids = {
        let conn = shared.conn.borrow();
        queries::query_track_ids_browsed(
            &conn,
            &source,
            &sort.field,
            &sort.dir,
            &filter,
            &browse,
            &queue_ids,
        )
    };

    match ids {
        Ok(ids) => {
            if queries::is_queue_capped(ids.len()) {
                tracing::warn!(
                    limit = queries::QUEUE_LIMIT,
                    "queue capped at {} tracks",
                    queries::QUEUE_LIMIT
                );
            }
            (ids, position as usize)
        }
        Err(error) => {
            tracing::error!(
                %error,
                "failed to build queue ids for activation; falling back to a single-track queue"
            );
            (vec![activated_id], 0)
        }
    }
}

/// Fetches the current queue's ids (in play order) via `shared.queue_ids_
/// provider`, for `reload`/`queue_ids_for_activation` to pass through to the
/// `queries` layer when `source` is `ViewSource::Queue`. Every call site
/// already checks `source` first, so this is only ever invoked when a fresh
/// snapshot is actually needed.
pub(in crate::ui) fn current_queue_ids(shared: &Shared) -> Vec<i64> {
    (shared.queue_ids_provider)().ids
}
