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
        /// FIL-7: hide AI-flagged tracks (only honored on the flat Library
        /// source, where the browse filter row lives).
        pub exclude_ai: bool,
        /// INST-10 / FIX-4: whether the windowed query projects the real `is_ai`
        /// column (the correlated provenance `EXISTS`) or a cheap literal `0`.
        /// Set to `experimental_enabled` when the query is (re)set — the AI badge
        /// only renders while the experimental switch is on, so with it off the
        /// hot windowed query pays no per-row provenance subquery.
        pub project_ai: bool,
        /// Only meaningful when `source == ViewSource::Queue` — see
        /// `TrackListModel::set_query`'s doc comment. Empty (and ignored)
        /// for every other source.
        pub queue_ids: Vec<i64>,
        /// QUE-7 queue projection whose context tail is fetched by bounded
        /// windows instead of retained as one id per context row.
        pub(super) virtual_queue: Option<super::super::queue_sections::QueueViewModel>,
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

/// Returns the one contiguous `items_changed` span between two queue
/// snapshots. Preserving the common prefix and suffix lets GTK keep their
/// existing row widgets; the frequent automatic-advance shape
/// `[current-next, ...] -> [...]` becomes one leading removal.
fn queue_snapshot_change(
    old: &super::queue_sections::QueueViewModel,
    new: &super::queue_sections::QueueViewModel,
) -> (u32, u32, u32) {
    new.leading_removal_change_from(old).unwrap_or((
        0,
        u32::try_from(old.total_len()).unwrap_or(u32::MAX),
        u32::try_from(new.total_len()).unwrap_or(u32::MAX),
    ))
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

    /// Installs an already-composed queue snapshot without a count query.
    /// The shared queue model is the authoritative row count; metadata stays
    /// lazy and is fetched only when `item()` asks for a visible window.
    pub(crate) fn set_queue_snapshot(
        &self,
        queue: &super::queue_sections::QueueViewModel,
        sections: Vec<(u32, u32)>,
    ) {
        let new_total = u32::try_from(queue.total_len()).unwrap_or(u32::MAX);
        let (position, removed, added) = {
            let mut state = self.imp().state.borrow_mut();
            let change = state
                .virtual_queue
                .as_ref()
                .map_or((0, state.total, new_total), |old_queue| {
                    queue_snapshot_change(old_queue, queue)
                });
            let sections_changed = state.sections != sections;
            state.source = ViewSource::Queue;
            state.sort_field.clear();
            state.sort_dir.clear();
            state.filter.clear();
            state.browse = BrowseFilter::default();
            state.queue_ids.clear();
            state.virtual_queue = Some(queue.clone());
            state.sections = sections;
            state.total = new_total;
            state.cache.clear();
            if change == (0, 0, 0) && sections_changed {
                (0, new_total, new_total)
            } else {
                change
            }
        };
        if removed != 0 || added != 0 {
            self.items_changed(position, removed, added);
        }
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
        self.set_query_browsed_ai(
            source, sort_field, sort_dir, filter, browse, queue_ids, false,
        );
    }

    /// Like [`set_query_browsed`](Self::set_query_browsed) but honoring the
    /// FIL-7 AI-exclude filter. When `exclude_ai` is set the window uses the
    /// core `*_ai` query and the row **count** uses the cheap core
    /// `query_track_count_browsed_ai` (a `COUNT(*)`), so the total is exact even
    /// for very large libraries — no longer the `QUEUE_LIMIT`-capped id-list
    /// length. When that count reaches the cap the view's "play all" queue will
    /// be truncated, so it logs the conventional `is_queue_capped` warning.
    #[allow(clippy::too_many_arguments)]
    pub fn set_query_browsed_ai(
        &self,
        source: &ViewSource,
        sort_field: &str,
        sort_dir: &str,
        filter: &str,
        browse: &BrowseFilter,
        queue_ids: &[i64],
        exclude_ai: bool,
    ) {
        let old_total = self.imp().state.borrow().total;

        let Some(conn) = self.imp().conn.borrow().clone() else {
            tracing::error!("TrackListModel::set_query: connection not set");
            return;
        };

        let new_total = if exclude_ai {
            let conn_ref = conn.borrow();
            queries::query_track_count_browsed_ai(
                &conn_ref, source, filter, browse, queue_ids, true,
            )
            .map_or_else(
                |error| {
                    tracing::error!(%error, "failed to count non-AI tracks for query");
                    0
                },
                |count| {
                    let total = count.max(0) as u32;
                    // The count is exact now, but the "play all" queue this view
                    // feeds still caps at QUEUE_LIMIT — warn per convention when
                    // the view is that large, so the truncation is not silent.
                    if queries::is_queue_capped(total as usize) {
                        tracing::warn!(
                            limit = queries::QUEUE_LIMIT,
                            "AI-filtered view queue capped at {} tracks",
                            queries::QUEUE_LIMIT
                        );
                    }
                    total
                },
            )
        } else {
            let conn_ref = conn.borrow();
            queries::query_track_count_browsed(&conn_ref, source, filter, browse, queue_ids)
                .map_or_else(
                    |error| {
                        tracing::error!(%error, source = %source.label(), "failed to count tracks for query");
                        0
                    },
                    |n| n.max(0) as u32,
                )
        };

        // INST-10 / FIX-4: the AI badge (and so the `is_ai` column) is needed
        // only while the experimental switch is on. Cache that here so the hot
        // windowed query pays the correlated provenance subquery only then.
        let project_ai = crate::ui::experimental::experimental_enabled(&conn.borrow());

        {
            let mut state = self.imp().state.borrow_mut();
            state.source = source.clone();
            state.sort_field = sort_field.to_string();
            state.sort_dir = sort_dir.to_string();
            state.filter = filter.to_string();
            state.browse = browse.clone();
            state.exclude_ai = exclude_ai;
            state.project_ai = project_ai;
            state.queue_ids = queue_ids.to_vec();
            state.virtual_queue = None;
            state.total = new_total;
            state.cache.clear();
        }

        tracing::debug!(
            total = new_total,
            source = %source.label(),
            sort_field,
            sort_dir,
            filter,
            exclude_ai,
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

        let (
            source,
            sort_field,
            sort_dir,
            filter,
            browse,
            exclude_ai,
            project_ai,
            queue_ids,
            virtual_queue,
        ) = {
            let state = self.imp().state.borrow();
            (
                state.source.clone(),
                state.sort_field.clone(),
                state.sort_dir.clone(),
                state.filter.clone(),
                state.browse.clone(),
                state.exclude_ai,
                state.project_ai,
                state.queue_ids.clone(),
                state.virtual_queue.clone(),
            )
        };

        let (query_offset, queue_ids) = if source == ViewSource::Queue {
            if let Some(queue) = virtual_queue {
                (
                    0,
                    queue.ids_window(window_start as usize, WINDOW_SIZE as usize),
                )
            } else {
                (i64::from(window_start), queue_ids)
            }
        } else {
            (i64::from(window_start), queue_ids)
        };

        let rows = {
            let mut conn = conn.borrow_mut();
            queries::query_track_window_browsed_ai(
                &mut conn,
                &source,
                &sort_field,
                &sort_dir,
                &filter,
                &browse,
                query_offset,
                i64::from(WINDOW_SIZE),
                &queue_ids,
                exclude_ai,
                project_ai,
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

    /// Patches the cached `Track`'s rating at `position` IN PLACE, emitting no
    /// model signal. A star-rating click updates the visible widget first, so
    /// only the model's cached clone is stale. Emitting a fake one-row
    /// remove+insert would make GtkColumnView replace the row widget under the
    /// pointer and snap the viewport back to the top. Patching the cached value
    /// directly keeps a later scroll-away/back correct without any signal. If
    /// the covering window is not cached there is nothing to patch: the next
    /// `track_at` re-reads the already-updated row from SQL.
    pub fn set_cached_rating(&self, position: u32, rating: i32) {
        let window_start = (position / WINDOW_SIZE) * WINDOW_SIZE;
        let offset_in_window = (position - window_start) as usize;
        let mut state = self.imp().state.borrow_mut();
        if let Some(track) = state
            .cache
            .get_mut(&window_start)
            .and_then(|window| window.get_mut(offset_in_window))
        {
            track.rating = rating;
        }
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
#[path = "track_list_model_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "track_list_model_scalability_tests.rs"]
mod scalability_tests;
