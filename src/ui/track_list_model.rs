//! `TrackListModel`: a `gio::ListModel` subclass that lazily fetches SQL
//! windows from `queries::query_track_window` instead of holding the whole
//! result set in memory. `GtkColumnView` virtualizes the *widgets*; this
//! model virtualizes the *data* — the two together are what let the track
//! list scroll through a library of any size instead of the stage-1
//! 200-row cap.
//!
//! ## Why a `glib::Object` subclass, not `gio::ListStore`
//!
//! `gio::ListStore` always holds every item it has ever been given; there is
//! no hook to compute an item lazily on first access. `GListModel` itself,
//! however, is just an interface (`item_type`, `n_items`, `item(position)`),
//! so implementing it directly lets `item()` load its backing SQL window on
//! demand and cache only a bounded number of windows at a time.
//!
//! ## Windowing and cache
//!
//! `total` (row count for the current sort/filter) comes from
//! `queries::query_track_count` and is refreshed by `set_query`, which also
//! clears the cache and fires `items_changed(0, old_total, new_total)` so
//! `GtkColumnView`/`NoSelection` re-pull rows as needed. `item(position)`
//! maps `position` to a `WINDOW_SIZE`-row-aligned window start, serving from
//! `ModelState::cache` on a hit or synchronously running
//! `queries::query_track_window` on a miss. The cache is capped at
//! `MAX_CACHED_WINDOWS` entries; eviction on overflow removes the
//! lowest-indexed cached window — not true LRU, just a simple, deterministic
//! rule that suits the model's largely monotonic scroll access pattern.
//!
//! Every fallible path (count query, window query) logs via `tracing::error!`
//! and returns `None`/`0` rather than panicking — a broken DB connection must
//! never crash the UI thread.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::glib::subclass::prelude::ObjectSubclassIsExt;
use rusqlite::Connection;

use crate::models::Track;
use crate::queries;

/// Row count per lazily-loaded window. Carried over from the stage-1 fixed
/// page size (`track_list.rs`'s former `WINDOW_LIMIT`), now used as the unit
/// of lazy loading rather than the single page loaded in full every reload.
const WINDOW_SIZE: u32 = 200;

/// Maximum number of windows kept in `ModelState::cache` at once. Bounds
/// memory for a scroll session that has touched many parts of a huge
/// library; 8 * `WINDOW_SIZE` = 1600 rows is comfortably enough to cover a
/// user's visible scroll neighborhood without unbounded growth.
const MAX_CACHED_WINDOWS: usize = 8;

mod imp {
    use super::*;
    use gio::subclass::prelude::*;

    #[derive(Default)]
    pub struct ModelState {
        pub total: u32,
        pub sort_field: String,
        pub sort_dir: String,
        pub filter: String,
        pub cache: BTreeMap<u32, Vec<Track>>,
    }

    /// `conn` is `None` only in the brief instant between `glib::Object::new`
    /// and `TrackListModel::new` setting it — no other code holds a
    /// reference to the object during that window, so every other method
    /// treats a `None` connection as an internal-logic-error state to log
    /// and degrade from, never to panic on.
    #[derive(Default)]
    pub struct TrackListModel {
        pub conn: RefCell<Option<Rc<RefCell<Connection>>>>,
        pub state: RefCell<ModelState>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TrackListModel {
        const NAME: &'static str = "RepriseTrackListModel";
        type Type = super::TrackListModel;
        type ParentType = glib::Object;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for TrackListModel {}

    impl ListModelImpl for TrackListModel {
        fn item_type(&self) -> glib::Type {
            glib::BoxedAnyObject::static_type()
        }

        fn n_items(&self) -> u32 {
            self.state.borrow().total
        }

        fn item(&self, position: u32) -> Option<glib::Object> {
            self.obj()
                .track_at(position)
                .map(|track| glib::BoxedAnyObject::new(track).upcast())
        }
    }
}

glib::wrapper! {
    pub struct TrackListModel(ObjectSubclass<imp::TrackListModel>)
        @implements gio::ListModel;
}

impl TrackListModel {
    /// Builds an empty model (`n_items() == 0`) bound to `conn`. Call
    /// `set_query` to load the initial sort/filter — the model does not
    /// query anything until then.
    pub fn new(conn: Rc<RefCell<Connection>>) -> Self {
        let obj: Self = glib::Object::new();
        obj.imp().conn.replace(Some(conn));
        obj
    }

    /// Re-counts rows for `(sort_field, sort_dir, filter)`, clears the
    /// window cache, and fires `items_changed(0, old_total, new_total)`.
    /// Mutates and drops the `state` borrow *before* emitting the signal:
    /// `items_changed` can synchronously re-enter this object (`GtkColumnView`
    /// / `NoSelection` typically re-read `n_items`/`item` right away), so no
    /// borrow may still be held when it fires.
    pub fn set_query(&self, sort_field: &str, sort_dir: &str, filter: &str) {
        let old_total = self.imp().state.borrow().total;

        let Some(conn) = self.imp().conn.borrow().clone() else {
            tracing::error!("TrackListModel::set_query: connection not set");
            return;
        };

        let new_total = {
            let conn_ref = conn.borrow();
            match queries::query_track_count(&conn_ref, filter) {
                Ok(n) => n.max(0) as u32,
                Err(error) => {
                    tracing::error!(%error, sort_field, sort_dir, filter, "failed to count tracks for query");
                    0
                }
            }
        };

        {
            let mut state = self.imp().state.borrow_mut();
            state.sort_field = sort_field.to_string();
            state.sort_dir = sort_dir.to_string();
            state.filter = filter.to_string();
            state.total = new_total;
            state.cache.clear();
        }

        tracing::debug!(
            total = new_total,
            sort_field,
            sort_dir,
            filter,
            "model query set total={new_total}"
        );

        self.items_changed(0, old_total, new_total);
    }

    /// Returns a clone of the `Track` at `position` (for row activation and,
    /// later, rating updates), loading its window from `queries` on a cache
    /// miss. `None` on an out-of-range position or a query failure — never
    /// panics.
    pub fn track_at(&self, position: u32) -> Option<Track> {
        let window_start = (position / WINDOW_SIZE) * WINDOW_SIZE;
        let offset_in_window = (position - window_start) as usize;

        if let Some(track) = self
            .imp()
            .state
            .borrow()
            .cache
            .get(&window_start)
            .and_then(|window| window.get(offset_in_window))
        {
            return Some(track.clone());
        }

        let Some(conn) = self.imp().conn.borrow().clone() else {
            tracing::error!("TrackListModel::track_at: connection not set");
            return None;
        };

        let (sort_field, sort_dir, filter) = {
            let state = self.imp().state.borrow();
            (
                state.sort_field.clone(),
                state.sort_dir.clone(),
                state.filter.clone(),
            )
        };

        let rows = {
            let mut conn = conn.borrow_mut();
            queries::query_track_window(
                &mut conn,
                &sort_field,
                &sort_dir,
                &filter,
                i64::from(window_start),
                i64::from(WINDOW_SIZE),
            )
        };

        let rows = match rows {
            Ok(rows) => rows,
            Err(error) => {
                tracing::error!(%error, position, window_start, "failed to load track window");
                return None;
            }
        };

        let track = rows.get(offset_in_window).cloned();

        let mut state = self.imp().state.borrow_mut();
        if !state.cache.contains_key(&window_start) && state.cache.len() >= MAX_CACHED_WINDOWS {
            // Deterministic, not true LRU: drop the lowest-indexed cached
            // window. Scroll access is largely monotonic, so the
            // lowest-indexed window is usually also the least recently
            // touched; a real LRU would need per-entry recency bookkeeping
            // this model has no other use for.
            if let Some(&oldest) = state.cache.keys().next() {
                state.cache.remove(&oldest);
            }
        }
        state.cache.insert(window_start, rows);

        track
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded_model(rows: &[(&str, &str)]) -> TrackListModel {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        for (t, a) in rows {
            conn.execute(
                "INSERT INTO tracks (path, title, artist, added_at) VALUES (?1, ?2, ?3, 0)",
                rusqlite::params![format!("/x/{t}.flac"), t, a],
            )
            .unwrap();
        }
        TrackListModel::new(Rc::new(RefCell::new(conn)))
    }

    #[test]
    fn set_query_updates_n_items_from_count() {
        let model = seeded_model(&[("Zulu", "AAA"), ("Alpha", "BBB"), ("Mid", "CCC")]);
        assert_eq!(model.n_items(), 0);
        model.set_query("title", "asc", "");
        assert_eq!(model.n_items(), 3);
    }

    #[test]
    fn set_query_applies_filter_to_count_and_rows() {
        let model = seeded_model(&[("Zulu", "AAA"), ("Alpha", "BBB"), ("Mid", "CCC")]);
        model.set_query("title", "asc", "zu");
        assert_eq!(model.n_items(), 1);
        assert_eq!(model.track_at(0).unwrap().title, "Zulu");
    }

    #[test]
    fn track_at_loads_in_sorted_order() {
        let model = seeded_model(&[("Zulu", "AAA"), ("Alpha", "BBB"), ("Mid", "CCC")]);
        model.set_query("title", "asc", "");
        assert_eq!(model.track_at(0).unwrap().title, "Alpha");
        assert_eq!(model.track_at(1).unwrap().title, "Mid");
        assert_eq!(model.track_at(2).unwrap().title, "Zulu");
    }

    #[test]
    fn track_at_out_of_range_returns_none() {
        let model = seeded_model(&[("Alpha", "BBB")]);
        model.set_query("title", "asc", "");
        assert!(model.track_at(5).is_none());
    }

    #[test]
    fn set_query_clears_stale_cache_between_queries() {
        let model = seeded_model(&[("Zulu", "AAA"), ("Alpha", "BBB")]);
        model.set_query("title", "asc", "");
        assert_eq!(model.track_at(0).unwrap().title, "Alpha");
        model.set_query("title", "desc", "");
        assert_eq!(model.track_at(0).unwrap().title, "Zulu");
    }
}
