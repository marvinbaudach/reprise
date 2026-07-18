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

use reprise_core::models::Track;
use reprise_core::queries::{self, BrowseFilter};
use reprise_core::view_source::ViewSource;

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
        pub source: ViewSource,
        pub sort_field: String,
        pub sort_dir: String,
        pub filter: String,
        pub browse: BrowseFilter,
        /// Only meaningful when `source == ViewSource::Queue` — see
        /// `TrackListModel::set_query`'s doc comment. Empty (and ignored)
        /// for every other source.
        pub queue_ids: Vec<i64>,
        pub cache: BTreeMap<u32, Vec<Track>>,
        /// QUE-1 section ranges (half-open, model coordinates) for the
        /// Queue source; empty = the whole model is one section. Set via
        /// `TrackListModel::set_sections` BEFORE the query swap whose
        /// `items_changed` makes GTK re-read sections.
        pub sections: Vec<(u32, u32)>,
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
        // The `gtk::SectionModel` interface (QUE-1 queue headers) is
        // test-gated OFF: its `interface_init` asserts the registering
        // thread called `gtk4::init()`, and `cargo test`'s worker threads
        // race for that — whichever unit test constructs the first model on
        // a non-GTK thread would panic. Production always registers on the
        // main thread after `gtk4::init()`. The section MATH stays fully
        // unit-tested (`queue_sections::section_ranges` + `section_for`
        // below); the live interface is exercised by the headless E2E runs.
        #[cfg(not(test))]
        type Interfaces = (gio::ListModel, gtk4::SectionModel);
        #[cfg(test)]
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for TrackListModel {}

    /// The `SectionModel::section` contract, as a plain function so unit
    /// tests cover it without the GTK interface (see `Interfaces` above).
    pub(super) fn section_for(sections: &[(u32, u32)], total: u32, position: u32) -> (u32, u32) {
        for &(start, end) in sections {
            if position >= start && position < end {
                return (start, end);
            }
        }
        // No sections declared (every non-Queue source): the whole model is
        // one section. A position PAST the declared ranges (a transient
        // sections/total mismatch, e.g. GTK re-reading sections mid-reload):
        // tile the uncovered tail as its own section. This must NEVER answer
        // a range overlapping a declared one — GTK matches a header widget
        // per section start, and an overlapping answer trips the fatal
        // `header->widget == NULL` assertion in
        // gtk_list_item_manager_ensure_items (seen live: abort on switching
        // to the Queue view from a deep-scrolled larger view).
        let last_end = sections.iter().map(|&(_, end)| end).max().unwrap_or(0);
        (last_end, total.max(position.saturating_add(1)))
    }

    #[cfg(not(test))]
    impl gtk4::subclass::prelude::SectionModelImpl for TrackListModel {
        fn section(&self, position: u32) -> (u32, u32) {
            let state = self.state.borrow();
            section_for(&state.sections, state.total, position)
        }
    }

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

#[cfg(not(test))]
glib::wrapper! {
    pub struct TrackListModel(ObjectSubclass<imp::TrackListModel>)
        @implements gio::ListModel, gtk4::SectionModel;
}
#[cfg(test)]
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

    /// Re-counts rows for `(source, sort_field, sort_dir, filter)`, clears
    /// the window cache, and fires `items_changed(0, old_total, new_total)`.
    /// `queue_ids` is only meaningful (and only read) when `source ==
    /// ViewSource::Queue` (see `queries::query_track_window`'s doc comment
    /// for why the Queue source needs an explicit id list rather than a
    /// `WHERE` clause); every other source ignores it, so callers may pass
    /// `&[]`. Mutates and drops the `state` borrow *before* emitting the
    /// signal: `items_changed` can synchronously re-enter this object
    /// (`GtkColumnView`/`NoSelection` typically re-read `n_items`/`item`
    /// right away), so no borrow may still be held when it fires.
    /// Declares the QUE-1 section ranges the next query swap renders.
    /// Call BEFORE `set_query`/`set_query_browsed` — their full-range
    /// `items_changed` is what makes GTK re-read `section()`. Pass an empty
    /// vec for every non-Queue source (one whole-model section).
    pub fn set_sections(&self, sections: Vec<(u32, u32)>) {
        self.imp().state.borrow_mut().sections = sections;
    }

    pub fn set_query(
        &self,
        source: &ViewSource,
        sort_field: &str,
        sort_dir: &str,
        filter: &str,
        queue_ids: &[i64],
    ) {
        self.set_query_browsed(
            source,
            sort_field,
            sort_dir,
            filter,
            &BrowseFilter::default(),
            queue_ids,
        );
    }

    pub fn set_query_browsed(
        &self,
        source: &ViewSource,
        sort_field: &str,
        sort_dir: &str,
        filter: &str,
        browse: &BrowseFilter,
        queue_ids: &[i64],
    ) {
        let old_total = self.imp().state.borrow().total;

        let Some(conn) = self.imp().conn.borrow().clone() else {
            tracing::error!("TrackListModel::set_query: connection not set");
            return;
        };

        let new_total = {
            let conn_ref = conn.borrow();
            match queries::query_track_count_browsed(&conn_ref, source, filter, browse, queue_ids) {
                Ok(n) => n.max(0) as u32,
                Err(error) => {
                    tracing::error!(%error, source = %source.label(), sort_field, sort_dir, filter, "failed to count tracks for query");
                    0
                }
            }
        };

        {
            let mut state = self.imp().state.borrow_mut();
            state.source = source.clone();
            state.sort_field = sort_field.to_string();
            state.sort_dir = sort_dir.to_string();
            state.filter = filter.to_string();
            state.browse = browse.clone();
            state.queue_ids = queue_ids.to_vec();
            state.total = new_total;
            state.cache.clear();
        }

        tracing::debug!(
            total = new_total,
            source = %source.label(),
            sort_field,
            sort_dir,
            filter,
            "model query set"
        );

        self.items_changed(0, old_total, new_total);
    }

    /// Returns a clone of the `Track` at `position` (for row activation and,
    /// later, rating updates), loading its window from `queries` on a cache
    /// miss. `None` on an out-of-range position or a query failure — never
    /// panics.
    pub fn track_at(&self, position: u32) -> Option<Track> {
        let total = self.imp().state.borrow().total;
        if position >= total {
            return None;
        }

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

        let (source, sort_field, sort_dir, filter, browse, queue_ids) = {
            let state = self.imp().state.borrow();
            (
                state.source.clone(),
                state.sort_field.clone(),
                state.sort_dir.clone(),
                state.filter.clone(),
                state.browse.clone(),
                state.queue_ids.clone(),
            )
        };

        let rows = {
            let mut conn = conn.borrow_mut();
            queries::query_track_window_browsed(
                &mut conn,
                &source,
                &sort_field,
                &sort_dir,
                &filter,
                &browse,
                i64::from(window_start),
                i64::from(WINDOW_SIZE),
                &queue_ids,
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

    /// Invalidates the cached window covering `position` and fires
    /// `items_changed(position, 1, 1)` so anything bound to the model
    /// (`GtkColumnView`/`NoSelection`) re-pulls that exact row via `item()`.
    /// Used after a rating write (`track_list.rs`): the database now holds
    /// the new value, but the model's cache still holds the `Track` clone
    /// from before the write, and dropping the *whole* cache (as
    /// `set_query` does) would be a much heavier hammer for a one-row
    /// change. Out-of-range positions are logged and ignored rather than
    /// panicking, matching every other fallible path on this type.
    pub fn invalidate_window_at(&self, position: u32) {
        let total = self.imp().state.borrow().total;
        if position >= total {
            tracing::warn!(
                position,
                total,
                "invalidate_window_at: position out of range"
            );
            return;
        }

        let window_start = (position / WINDOW_SIZE) * WINDOW_SIZE;
        self.imp().state.borrow_mut().cache.remove(&window_start);

        self.items_changed(position, 1, 1);
    }

    /// Test-only accessor exposing the set of currently cached window-start
    /// keys, so eviction behavior can be asserted without reaching into
    /// private state. Not part of the public API.
    #[cfg(test)]
    fn cached_windows(&self) -> Vec<u32> {
        self.imp().state.borrow().cache.keys().copied().collect()
    }

    /// Performance diagnostics for the generated-metadata benchmark: number
    /// of cached SQL windows and total `Track` rows retained by them. Kept
    /// crate-private so normal UI behavior cannot couple to cache internals.
    pub(in crate::ui) fn cache_usage(&self) -> (usize, usize) {
        let state = self.imp().state.borrow();
        let rows = state.cache.values().map(Vec::len).sum();
        (state.cache.len(), rows)
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn section_for_answers_ranges_and_full_model_fallback() {
        let ranges = [(0u32, 1u32), (1, 3), (3, 6)];
        assert_eq!(super::imp::section_for(&ranges, 6, 0), (0, 1));
        assert_eq!(super::imp::section_for(&ranges, 6, 2), (1, 3));
        assert_eq!(super::imp::section_for(&ranges, 6, 5), (3, 6));
        // Past the declared ranges (transient sections/total mismatch): the
        // answer must be a NON-overlapping tail section, never one starting
        // at 0 — an answer overlapping an already-matched header is exactly
        // what GTK's gtk_list_item_manager_ensure_items asserts on (seen
        // live: abort on switching to the Queue view from a deep-scrolled
        // larger view).
        assert_eq!(super::imp::section_for(&ranges, 6, 9), (6, 10));
        // The live crash shape: a 2-row queue's ranges against a still-500-
        // row model, viewport tracking position 499.
        assert_eq!(
            super::imp::section_for(&[(0, 1), (1, 2)], 500, 499),
            (2, 500)
        );
        // No sections declared: the whole model stays one section.
        assert_eq!(super::imp::section_for(&[], 42, 7), (0, 42));
    }
    use super::*;

    fn seeded_model(rows: &[(&str, &str)]) -> TrackListModel {
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        for (t, a) in rows {
            conn.execute(
                "INSERT INTO tracks (path, title, artist, added_at) VALUES (?1, ?2, ?3, 0)",
                rusqlite::params![format!("/x/{t}.flac"), t, a],
            )
            .unwrap();
        }
        TrackListModel::new(Rc::new(RefCell::new(conn)))
    }

    /// Sortable, zero-padded title for row `i` (e.g. `track-00042`), used by
    /// the bulk-seeding tests below so the expected sort order is a trivial
    /// function of the row index.
    fn bulk_title(i: u32) -> String {
        format!("track-{i:05}")
    }

    /// Seeds `count` rows in a single transaction (fast even for thousands
    /// of rows) with titles from `bulk_title`, so ascending title sort order
    /// matches ascending insertion/index order.
    fn seeded_model_bulk(count: u32) -> TrackListModel {
        let mut conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        {
            let tx = conn.transaction().unwrap();
            for i in 0..count {
                let title = bulk_title(i);
                tx.execute(
                    "INSERT INTO tracks (path, title, artist, added_at) VALUES (?1, ?2, ?3, 0)",
                    rusqlite::params![format!("/x/{i:05}.flac"), title, "Bulk Artist"],
                )
                .unwrap();
            }
            tx.commit().unwrap();
        }
        TrackListModel::new(Rc::new(RefCell::new(conn)))
    }

    #[test]
    fn set_query_updates_n_items_from_count() {
        let model = seeded_model(&[("Zulu", "AAA"), ("Alpha", "BBB"), ("Mid", "CCC")]);
        assert_eq!(model.n_items(), 0);
        model.set_query(&ViewSource::Library, "title", "asc", "", &[]);
        assert_eq!(model.n_items(), 3);
    }

    #[test]
    fn set_query_applies_filter_to_count_and_rows() {
        let model = seeded_model(&[("Zulu", "AAA"), ("Alpha", "BBB"), ("Mid", "CCC")]);
        model.set_query(&ViewSource::Library, "title", "asc", "zu", &[]);
        assert_eq!(model.n_items(), 1);
        assert_eq!(model.track_at(0).unwrap().title, "Zulu");
    }

    #[test]
    fn track_at_loads_in_sorted_order() {
        let model = seeded_model(&[("Zulu", "AAA"), ("Alpha", "BBB"), ("Mid", "CCC")]);
        model.set_query(&ViewSource::Library, "title", "asc", "", &[]);
        assert_eq!(model.track_at(0).unwrap().title, "Alpha");
        assert_eq!(model.track_at(1).unwrap().title, "Mid");
        assert_eq!(model.track_at(2).unwrap().title, "Zulu");
    }

    #[test]
    fn track_at_out_of_range_returns_none() {
        let model = seeded_model(&[("Alpha", "BBB")]);
        model.set_query(&ViewSource::Library, "title", "asc", "", &[]);
        assert!(model.track_at(5).is_none());
    }

    #[test]
    fn set_query_clears_stale_cache_between_queries() {
        let model = seeded_model(&[("Zulu", "AAA"), ("Alpha", "BBB")]);
        model.set_query(&ViewSource::Library, "title", "asc", "", &[]);
        assert_eq!(model.track_at(0).unwrap().title, "Alpha");
        model.set_query(&ViewSource::Library, "title", "desc", "", &[]);
        assert_eq!(model.track_at(0).unwrap().title, "Zulu");
    }

    /// Regression test: all prior tests seeded <=3 rows, so every `track_at`
    /// call stayed inside window 0 and never exercised the window-boundary
    /// math (`window_start`/`offset_in_window`) for a second or later
    /// window. Seed >200 rows (WINDOW_SIZE) so position 200 falls in window
    /// 1 and position 449 falls in window 2, and check both land on the
    /// title the sort order predicts.
    #[test]
    fn track_at_spans_multiple_windows_in_sorted_order() {
        const ROW_COUNT: u32 = 450;
        let model = seeded_model_bulk(ROW_COUNT);
        model.set_query(&ViewSource::Library, "title", "asc", "", &[]);
        assert_eq!(model.n_items(), ROW_COUNT);

        assert_eq!(model.track_at(0).unwrap().title, bulk_title(0));
        assert_eq!(model.track_at(200).unwrap().title, bulk_title(200));
        assert_eq!(model.track_at(449).unwrap().title, bulk_title(449));

        assert!(model.track_at(ROW_COUNT).is_none());
    }

    #[test]
    fn invalidate_window_at_forces_a_fresh_read_of_that_row() {
        let model = seeded_model(&[("Zulu", "AAA"), ("Alpha", "BBB")]);
        model.set_query(&ViewSource::Library, "title", "asc", "", &[]);
        assert_eq!(model.track_at(0).unwrap().rating, 0);

        // Mutate the underlying row directly (simulating a rating write
        // elsewhere), bypassing the model entirely.
        {
            let conn = model.imp().conn.borrow().clone().unwrap();
            conn.borrow()
                .execute("UPDATE tracks SET rating = 4 WHERE title = 'Alpha'", [])
                .unwrap();
        }

        // Without invalidation the cached clone is still stale.
        assert_eq!(model.track_at(0).unwrap().rating, 0);

        model.invalidate_window_at(0);
        assert_eq!(model.track_at(0).unwrap().rating, 4);
    }

    #[test]
    fn invalidate_window_at_out_of_range_is_a_no_op() {
        let model = seeded_model(&[("Alpha", "BBB")]);
        model.set_query(&ViewSource::Library, "title", "asc", "", &[]);
        // Must not panic.
        model.invalidate_window_at(5);
    }

    /// Regression test: eviction (`MAX_CACHED_WINDOWS` = 8, drop the
    /// lowest-indexed cached window) was never exercised because no test
    /// touched more than one window. Seed 1700 rows (9 windows) and touch
    /// one position per window in ascending order so the 9th touch forces
    /// an eviction; assert the cache holds exactly 8 windows, window 0 was
    /// evicted, and the just-loaded window 8 is present.
    #[test]
    fn track_at_evicts_lowest_window_past_cache_capacity() {
        const ROW_COUNT: u32 = 1700;
        const WINDOW_COUNT: u32 = 9;
        let model = seeded_model_bulk(ROW_COUNT);
        model.set_query(&ViewSource::Library, "title", "asc", "", &[]);

        for window in 0..WINDOW_COUNT {
            let position = window * WINDOW_SIZE;
            assert_eq!(
                model.track_at(position).unwrap().title,
                bulk_title(position)
            );
        }

        let mut cached = model.cached_windows();
        cached.sort_unstable();
        assert_eq!(cached.len(), MAX_CACHED_WINDOWS);
        assert_eq!(cached, vec![200, 400, 600, 800, 1000, 1200, 1400, 1600]);
        assert!(!cached.contains(&0), "window 0 should have been evicted");
        assert!(
            cached.contains(&(8 * WINDOW_SIZE)),
            "just-loaded window 8 should be present"
        );
    }

    /// Stage 3 Task 3: `set_query`/`track_at` must plumb a non-Library
    /// `ViewSource` through to `queries::query_track_window`/`query_track_
    /// count` unchanged — exercised here end-to-end through the model
    /// (`queries.rs`'s own test module covers each source's SQL directly).
    #[test]
    fn set_query_with_missing_source_shows_only_missing_rows() {
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        for (t, missing_since) in [("Alpha", None), ("Beta", Some(1))] {
            conn.execute(
                "INSERT INTO tracks (path, title, artist, added_at, missing_since) \
                 VALUES (?1, ?2, '', 0, ?3)",
                rusqlite::params![format!("/x/{t}.flac"), t, missing_since],
            )
            .unwrap();
        }
        let model = TrackListModel::new(Rc::new(RefCell::new(conn)));

        model.set_query(&ViewSource::Missing, "title", "asc", "", &[]);
        assert_eq!(model.n_items(), 1);
        assert_eq!(model.track_at(0).unwrap().title, "Beta");
    }

    /// `ViewSource::Queue` reads its rows from the `queue_ids` param, not
    /// from any `WHERE` clause — this pins that `set_query`/`track_at`
    /// actually thread that slice through to `queries::query_track_window`
    /// and preserve its order.
    #[test]
    fn set_query_with_queue_source_follows_queue_ids_order() {
        let model = seeded_model(&[("Zulu", "AAA"), ("Alpha", "BBB"), ("Mid", "CCC")]);
        let ids: Vec<i64> = {
            let conn = model.imp().conn.borrow().clone().unwrap();
            let conn = conn.borrow();
            let mut stmt = conn
                .prepare("SELECT id FROM tracks ORDER BY title")
                .unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        // ids sorted by title are [Alpha, Mid, Zulu]; reverse them so the
        // Queue order is the opposite of any column sort.
        let queue_ids: Vec<i64> = ids.into_iter().rev().collect();

        model.set_query(&ViewSource::Queue, "ignored", "ignored", "", &queue_ids);
        assert_eq!(model.n_items(), 3);
        assert_eq!(model.track_at(0).unwrap().title, "Zulu");
        assert_eq!(model.track_at(1).unwrap().title, "Mid");
        assert_eq!(model.track_at(2).unwrap().title, "Alpha");
    }
}

#[cfg(test)]
#[path = "track_list_model_scalability_tests.rs"]
mod scalability_tests;
