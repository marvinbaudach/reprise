//! The sortable, searchable track list: a `GtkColumnView` backed by
//! `track_list_model::TrackListModel`, a lazy `gio::ListModel` that fetches
//! `WINDOW_SIZE`-row SQL windows from `queries::query_track_window` on demand
//! — SQL stays the single source of truth for ordering and filtering, GTK
//! never re-sorts the model itself, and the whole result set is never held
//! in memory at once.
//!
//! ## Row data: `glib::BoxedAnyObject`, not a GObject subclass
//!
//! `models::Track` is a plain Rust struct. Returning it from
//! `gio::ListModel::item()` only requires *something* that is
//! `IsA<glib::Object>`; a full `glib::Object` subclass with GObject
//! properties would be needed if the bound widgets had to react to
//! property-level `notify::` signals (e.g. for in-place editing) or if the
//! object needed to cross an FFI/property-binding boundary. Neither applies
//! here — the factory callbacks just read a `Track` once per bind — so
//! `glib::BoxedAnyObject::new(track)` is the simplest correct approach and
//! there is no separate `track_object.rs` module (a bespoke wrapper type
//! would add boilerplate without behavior). `TrackListModel` (see
//! `track_list_model.rs`) is the one place that constructs these boxes.
//!
//! ## Sorting: per-column `CustomSorter` as a click signal only
//!
//! `GtkColumnView` headers only become clickable/toggle-sortable once a
//! column has a non-null `sorter`. This module gives every column a
//! `gtk::CustomSorter` whose compare function always returns `Equal` — it
//! never actually reorders `TrackListModel` — purely so GTK renders the sort
//! indicator and emits sort-order changes on click. The real ordering is
//! decided by SQL: clicking a header changes `ColumnView`'s aggregate
//! `ColumnViewSorter` (`primary-sort-column`/`primary-sort-order`), which
//! this module observes, maps back to a whitelisted `queries` sort field via
//! `ColumnViewColumn::id()`, and uses to re-run the model's query via
//! `TrackListModel::set_query`.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use rusqlite::Connection;

use crate::format::format_duration;
use crate::library::stats;
use crate::models::Track;
use crate::ui::rating::RatingWidget;
use crate::ui::strings;
use crate::ui::track_list_model::TrackListModel;

const STACK_PAGE_EMPTY: &str = "empty";
const STACK_PAGE_LIST: &str = "list";

/// Dev/verification hook (permanent, like `REPRISE_SCAN_DIR` and
/// `REPRISE_SMOKE_QUIT`): when set, the first row is activated
/// programmatically — through the exact same `on_activate` path a
/// double-click takes — once the initial load has run and the main loop is
/// idle. Combined with `REPRISE_SCAN_DIR` (populate), `REPRISE_AUDIO_SINK=
/// fakesink` (no audio device) and `REPRISE_SMOKE_QUIT` (exit), this enables
/// the full headless play-a-track E2E:
///
/// `REPRISE_SCAN_DIR=… REPRISE_SMOKE_ACTIVATE=1 REPRISE_AUDIO_SINK=fakesink
///  REPRISE_SMOKE_QUIT=1 xvfb-run -a cargo run`
const SMOKE_ACTIVATE_ENV_VAR: &str = "REPRISE_SMOKE_ACTIVATE";

/// Icon shown on the empty-library placeholder (nothing has been scanned
/// in yet).
const ICON_EMPTY_LIBRARY: &str = "folder-music-symbolic";
/// Icon shown when a search filter matched zero rows — distinct from the
/// empty-library icon so the two states also read differently at a glance.
const ICON_NO_RESULTS: &str = "system-search-symbolic";

#[derive(Debug, Clone, PartialEq, Eq)]
struct SortState {
    field: String,
    dir: String,
}

/// Which page of the track-list `Stack` should be visible, and (for the two
/// empty variants) which copy the shared `StatusPage` should carry. A plain
/// enum decided by a pure function (`empty_state_for`) so the selection
/// logic is unit-testable without a running GTK application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EmptyState {
    /// The library itself has no tracks yet (no filter active either).
    EmptyLibrary,
    /// The library has tracks, but the active search filter matched none.
    NoResults,
    /// At least one row to show — the populated list page.
    List,
}

/// Pure decision of which empty state (or the populated list) applies for a
/// given result-row count and whether a search filter is currently active.
/// Kept side-effect free and separate from `reload`/`apply_empty_state` so
/// it can be unit tested directly instead of only through a live GTK stack.
fn empty_state_for(row_count: usize, has_filter: bool) -> EmptyState {
    match (row_count, has_filter) {
        (0, false) => EmptyState::EmptyLibrary,
        (0, true) => EmptyState::NoResults,
        _ => EmptyState::List,
    }
}

/// Default sort: artist ascending, matching the secondary-order convention
/// already baked into `queries::SORT_WHITELIST` for the "artist" field.
impl Default for SortState {
    fn default() -> Self {
        Self {
            field: "artist".to_string(),
            dir: "asc".to_string(),
        }
    }
}

/// Callback invoked with an activated row's `Track` (double-click/Enter on a
/// row, or the `REPRISE_SMOKE_ACTIVATE` hook). Provided by `window::build`,
/// which routes it to the player — the track list itself stays free of any
/// playback knowledge.
pub type OnActivate = Box<dyn Fn(&Track)>;

struct Shared {
    model: TrackListModel,
    /// The same UI-owned connection `TrackList::new` was given, kept here
    /// too (alongside the clone `TrackListModel` holds internally) so the
    /// rating column's click handler can write through `library::stats`
    /// without reaching into the model's private state.
    conn: Rc<RefCell<Connection>>,
    stack: gtk4::Stack,
    /// The single empty-state placeholder widget. Its title/description/icon
    /// are mutated in place by `apply_empty_state` rather than swapping in a
    /// third stack page — see that function's doc comment.
    empty_page: adw::StatusPage,
    sort: RefCell<SortState>,
    filter: RefCell<String>,
    /// Shared by `wire_activate` (user activation) and the smoke-activate
    /// hook so both take the identical code path.
    on_activate: OnActivate,
    /// Invoked at the end of every `reload()` — initial load, search-filter
    /// changes, sort-header clicks, and the explicit `TrackList::reload()`
    /// call `window.rs` makes after a scan completes. `window.rs` uses this
    /// single hook to keep `status_bar::StatusBar` in sync rather than
    /// scattering refresh calls across every place that can trigger a
    /// reload.
    on_reload: Box<dyn Fn()>,
}

/// Handle to the built track list widget. Owns the shared, reference-counted
/// state that the sort-header and search-debounce callbacks close over.
pub struct TrackList {
    shared: Rc<Shared>,
}

impl TrackList {
    /// Builds the track list and performs the initial load (unfiltered,
    /// default sort). `conn` is the shared UI-owned database connection;
    /// `on_activate` receives the `Track` of every activated row; `on_reload`
    /// is called after every reload (see the `Shared::on_reload` doc
    /// comment).
    pub fn new(
        conn: Rc<RefCell<Connection>>,
        on_activate: OnActivate,
        on_reload: impl Fn() + 'static,
    ) -> Self {
        let model = TrackListModel::new(conn.clone());
        let selection = gtk4::NoSelection::new(Some(model.clone()));

        let column_view = gtk4::ColumnView::builder()
            .model(&selection)
            .show_row_separators(true)
            .show_column_separators(true)
            .build();

        append_column(
            &column_view,
            "title",
            strings::COLUMN_TITLE,
            0.0,
            false,
            |t| t.title.clone(),
        );
        let artist_column = append_column(
            &column_view,
            "artist",
            strings::COLUMN_ARTIST,
            0.0,
            false,
            |t| t.artist.clone(),
        );
        append_column(
            &column_view,
            "album",
            strings::COLUMN_ALBUM,
            0.0,
            false,
            |t| t.album.clone(),
        );
        append_column(
            &column_view,
            "year",
            strings::COLUMN_YEAR,
            0.0,
            false,
            |t| t.year.map(|y| y.to_string()).unwrap_or_default(),
        );
        append_column(
            &column_view,
            "duration_ms",
            strings::COLUMN_LENGTH,
            1.0,
            true,
            |t| format_duration(t.duration_ms),
        );
        let scrolled = gtk4::ScrolledWindow::builder()
            .child(&column_view)
            .vexpand(true)
            .hexpand(true)
            .build();

        let empty_page = build_status_page();

        let stack = gtk4::Stack::new();
        stack.add_named(&empty_page, Some(STACK_PAGE_EMPTY));
        stack.add_named(&scrolled, Some(STACK_PAGE_LIST));
        stack.set_visible_child_name(STACK_PAGE_EMPTY);

        let shared = Rc::new(Shared {
            model,
            conn,
            stack,
            empty_page,
            sort: RefCell::new(SortState::default()),
            filter: RefCell::new(String::new()),
            on_activate,
            on_reload: Box::new(on_reload),
        });

        // Built after `shared` exists (unlike the other columns above): its
        // click handler needs `shared.conn`/`shared.model` to persist a
        // rating write and refresh the model's cached row — see
        // `append_rating_column`'s doc comment. Appended last, so it still
        // lands as the rightmost column, matching the visual order the
        // other five columns were just added in.
        append_rating_column(&column_view, &shared);

        wire_sort_clicks(&column_view, &shared);

        // Sets the initial sort indicator (artist ascending) on the column
        // header. `SortState::default()` is already `artist`/`asc`, so the
        // `primary-sort-column`/`primary-sort-order` notify signals this
        // triggers land in `on_sorter_changed`, compute the same
        // (field, dir) pair already stored in `shared.sort`, and the dedup
        // guard there (`if *shared.sort.borrow() == new_sort { return; }`)
        // short-circuits before it would call `reload` — so this call fires
        // zero SQL queries. The one and only initial load below still runs
        // exactly once.
        column_view.sort_by_column(Some(&artist_column), gtk4::SortType::Ascending);

        wire_activate(&column_view, &shared);

        reload(&shared);
        arm_smoke_activate(&shared);

        Self { shared }
    }

    /// The root widget to embed as the window body (a `gtk::Stack` that
    /// switches between the empty placeholder and the populated list).
    pub fn widget(&self) -> &gtk4::Stack {
        &self.shared.stack
    }

    /// Sets the live-search filter and reloads. Called from `window.rs`
    /// after its own debounce timer fires.
    pub fn set_filter(&self, text: &str) {
        *self.shared.filter.borrow_mut() = text.to_string();
        reload(&self.shared);
    }

    /// Re-runs the current sort/filter query and refreshes the list without
    /// changing either — used by `window.rs` after a scan completes, so
    /// newly added tracks show up without disturbing an active search.
    pub fn reload(&self) {
        reload(&self.shared);
    }
}

/// Builds the shared empty-state placeholder, initially carrying the
/// empty-library copy (the state `TrackList::new`'s first `reload()` will
/// normally confirm, since there's no library yet on first launch).
/// `apply_empty_state` swaps its title/description/icon in place for the
/// no-results case rather than building a second widget.
fn build_status_page() -> adw::StatusPage {
    adw::StatusPage::builder()
        .icon_name(ICON_EMPTY_LIBRARY)
        .title(strings::EMPTY_LIBRARY_TITLE)
        .description(strings::EMPTY_LIBRARY_DESCRIPTION)
        .vexpand(true)
        .build()
}

/// Builds one `ColumnViewColumn` bound to a `SignalListItemFactory` that
/// renders a single `gtk::Label` per cell. `sort_id` is a whitelisted
/// `queries` sort field name, stashed on the column via `set_id` so header
/// clicks can be mapped back to it. `right_align` additionally marks the
/// label with the "numeric" style class (tabular figures, GNOME convention
/// for right-aligned numeric columns such as file sizes/durations). Returns
/// the built column so `TrackList::new` can set the initial sort indicator
/// on the artist column.
fn append_column(
    column_view: &gtk4::ColumnView,
    sort_id: &'static str,
    title: &str,
    xalign: f32,
    right_align: bool,
    render: impl Fn(&Track) -> String + 'static,
) -> gtk4::ColumnViewColumn {
    let factory = gtk4::SignalListItemFactory::new();

    factory.connect_setup(move |_, obj| {
        let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
            tracing::warn!("track list column setup: object is not a ListItem");
            return;
        };
        let label = gtk4::Label::new(None);
        label.set_xalign(xalign);
        label.set_halign(if right_align {
            gtk4::Align::End
        } else {
            gtk4::Align::Start
        });
        if right_align {
            label.add_css_class("numeric");
        }
        item.set_child(Some(&label));
    });

    factory.connect_bind(move |_, obj| {
        let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
            tracing::warn!("track list column bind: object is not a ListItem");
            return;
        };
        let Some(label) = item.child().and_then(|w| w.downcast::<gtk4::Label>().ok()) else {
            tracing::warn!("track list column bind: list item child is not a Label");
            return;
        };
        let Some(boxed) = item
            .item()
            .and_then(|o| o.downcast::<glib::BoxedAnyObject>().ok())
        else {
            tracing::warn!("track list column bind: item is not a BoxedAnyObject<Track>");
            return;
        };
        let track = boxed.borrow::<Track>();
        label.set_text(&render(&track));
    });

    let column = gtk4::ColumnViewColumn::builder()
        .title(title)
        .factory(&factory)
        .resizable(true)
        .build();
    column.set_id(Some(sort_id));

    // Dummy sorter: makes the header clickable/toggleable without ever
    // reordering the model itself (SQL is the sort source of truth — see
    // module doc comment).
    let never_sorts = gtk4::CustomSorter::new(|_, _| gtk4::Ordering::Equal);
    column.set_sorter(Some(&never_sorts));

    column_view.append_column(&column);
    column
}

/// Builds the interactive `Rating` column: each cell is a `RatingWidget`
/// (`ui::rating`) instead of a `gtk::Label` — the one column whose factory
/// writes back to the database on user interaction rather than only
/// rendering a `Track` field. Requires a fully-built `shared` (its
/// `conn`/`model` are used by the click handler), which is why
/// `TrackList::new` calls this after constructing `Shared`, unlike the
/// other five columns built by `append_column` beforehand.
fn append_rating_column(
    column_view: &gtk4::ColumnView,
    shared: &Rc<Shared>,
) -> gtk4::ColumnViewColumn {
    let factory = gtk4::SignalListItemFactory::new();

    factory.connect_setup(move |_, obj| {
        let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
            tracing::warn!("rating column setup: object is not a ListItem");
            return;
        };
        let rating_widget = RatingWidget::new();
        item.set_child(Some(&rating_widget));
    });

    {
        let shared = shared.clone();
        factory.connect_bind(move |_, obj| {
            let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
                tracing::warn!("rating column bind: object is not a ListItem");
                return;
            };
            let Some(rating_widget) = item.child().and_then(|w| w.downcast::<RatingWidget>().ok())
            else {
                tracing::warn!("rating column bind: list item child is not a RatingWidget");
                return;
            };
            let Some(boxed) = item
                .item()
                .and_then(|o| o.downcast::<glib::BoxedAnyObject>().ok())
            else {
                tracing::warn!("rating column bind: item is not a BoxedAnyObject<Track>");
                return;
            };
            let track = boxed.borrow::<Track>();
            // Programmatic display update only — `RatingWidget::set_rating`
            // never invokes the `on_changed` callback, so this can never
            // recurse into `on_rating_changed` below (see the module doc
            // comment on `ui::rating`).
            rating_widget.set_rating(track.rating);

            let track_id = track.id;
            let position = item.position();
            let shared = shared.clone();
            rating_widget.set_on_changed(move |new_rating| {
                on_rating_changed(&shared, track_id, position, new_rating);
            });
        });
    }

    // Recycling guard: on unbind (the row is about to be rebound to a
    // different `Track`, or the widget dropped off-screen entirely), clear
    // the callback rather than leaving it pointed at the just-vacated
    // `(track_id, position)` pair. `connect_bind` always installs a fresh
    // one before the widget can be interacted with again, so this mainly
    // closes off a race that's already vanishingly unlikely — but a no-op
    // closure costs nothing and removes the possibility outright.
    factory.connect_unbind(move |_, obj| {
        let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(rating_widget) = item.child().and_then(|w| w.downcast::<RatingWidget>().ok())
        else {
            return;
        };
        rating_widget.set_on_changed(|_| {});
    });

    let column = gtk4::ColumnViewColumn::builder()
        .title(strings::RATING)
        .factory(&factory)
        .resizable(true)
        .build();
    column.set_id(Some("rating"));

    // Dummy sorter: makes the header clickable/toggleable without ever
    // reordering the model itself (SQL is the sort source of truth — see
    // module doc comment).
    let never_sorts = gtk4::CustomSorter::new(|_, _| gtk4::Ordering::Equal);
    column.set_sorter(Some(&never_sorts));

    column_view.append_column(&column);
    column
}

/// Persists a rating change via `library::stats::set_rating` and, on
/// success, invalidates the model's cached copy of the affected row (see
/// `TrackListModel::invalidate_window_at`) so a subsequent read — e.g.
/// scrolling away and back — sees the freshly written value rather than the
/// stale in-memory clone. A write failure is logged and otherwise swallowed
/// (fault tolerance: a rating write must never crash or wedge the UI); the
/// displayed rating already reflects the click (`RatingWidget::set_rating`
/// ran before this is called), so the user sees no visible inconsistency
/// unless they scroll away and back before the next successful write.
fn on_rating_changed(shared: &Rc<Shared>, track_id: i64, position: u32, new_rating: i32) {
    tracing::debug!(track_id, position, new_rating, "rating changed");
    let result = {
        let conn = shared.conn.borrow();
        stats::set_rating(&conn, track_id, new_rating)
    };
    match result {
        Ok(()) => shared.model.invalidate_window_at(position),
        Err(error) => {
            tracing::error!(%error, track_id, new_rating, "failed to persist rating change")
        }
    }
}

/// Observes the `ColumnView`'s aggregate sorter for header clicks and maps
/// them back to a whitelisted sort field + direction, then reloads.
fn wire_sort_clicks(column_view: &gtk4::ColumnView, shared: &Rc<Shared>) {
    let Some(sorter) = column_view.sorter() else {
        tracing::warn!("track list: ColumnView has no aggregate sorter; header clicks won't sort");
        return;
    };
    let Some(cv_sorter) = sorter.downcast_ref::<gtk4::ColumnViewSorter>() else {
        tracing::warn!(
            "track list: ColumnView sorter is not a ColumnViewSorter; header clicks won't sort"
        );
        return;
    };

    {
        let shared = shared.clone();
        cv_sorter.connect_primary_sort_column_notify(move |s| on_sorter_changed(&shared, s));
    }
    {
        let shared = shared.clone();
        cv_sorter.connect_primary_sort_order_notify(move |s| on_sorter_changed(&shared, s));
    }
}

fn on_sorter_changed(shared: &Rc<Shared>, sorter: &gtk4::ColumnViewSorter) {
    let Some(column) = sorter.primary_sort_column() else {
        return;
    };
    let Some(id) = column.id() else {
        tracing::warn!("track list: sorted column has no id; ignoring click");
        return;
    };
    let dir = match sorter.primary_sort_order() {
        gtk4::SortType::Descending => "desc",
        _ => "asc",
    };
    let new_sort = SortState {
        field: id.to_string(),
        dir: dir.to_string(),
    };

    // A single header click can fire both `primary-sort-column-notify` and
    // `primary-sort-order-notify` (e.g. switching to a new column changes
    // both which column is primary and its initial direction). Both land
    // here; only reload once the (field, dir) pair has actually changed so
    // one click can't trigger two identical SQL queries.
    if *shared.sort.borrow() == new_sort {
        return;
    }

    *shared.sort.borrow_mut() = new_sort;
    reload(shared);
}

/// Row activation (double-click or Enter on a focused row): resolve the
/// row's `Track` via `TrackListModel::track_at` and hand it to the
/// `on_activate` callback (which `window::build` routes to the player).
fn wire_activate(column_view: &gtk4::ColumnView, shared: &Rc<Shared>) {
    let shared = shared.clone();
    column_view.connect_activate(move |_view, position| {
        let Some(track) = shared.model.track_at(position) else {
            tracing::warn!(position, "track list activate: no item at position");
            return;
        };
        tracing::info!(path = %track.path, "activate track");
        (shared.on_activate)(&track);
    });
}

/// Arms the `REPRISE_SMOKE_ACTIVATE` hook (see `SMOKE_ACTIVATE_ENV_VAR`):
/// one idle callback, deferred so it runs once the main loop is up rather
/// than in the middle of window construction, that pushes the first row
/// through the same `on_activate` path as a real double-click.
fn arm_smoke_activate(shared: &Rc<Shared>) {
    if std::env::var(SMOKE_ACTIVATE_ENV_VAR).is_err() {
        return;
    }
    tracing::info!("{SMOKE_ACTIVATE_ENV_VAR} set: arming first-row activation");
    let shared = shared.clone();
    glib::idle_add_local_once(move || {
        let Some(track) = shared.model.track_at(0) else {
            tracing::warn!("{SMOKE_ACTIVATE_ENV_VAR}: track list is empty; nothing to activate");
            return;
        };
        tracing::info!(path = %track.path, "{SMOKE_ACTIVATE_ENV_VAR}: activating first row");
        (shared.on_activate)(&track);
    });
}

/// Re-runs the query against the current sort/filter state via
/// `TrackListModel::set_query`. Switches the stack to whichever page
/// `empty_state_for` selects for the resulting row count and filter state.
fn reload(shared: &Rc<Shared>) {
    let sort = shared.sort.borrow().clone();
    let filter = shared.filter.borrow().clone();
    let has_filter = !filter.trim().is_empty();

    shared.model.set_query(&sort.field, &sort.dir, &filter);

    let count = shared.model.n_items() as usize;
    apply_empty_state(shared, empty_state_for(count, has_filter));

    tracing::info!(count, field = %sort.field, dir = %sort.dir, filter = %filter, "query matched {count} tracks");

    (shared.on_reload)();
}

/// Applies an `EmptyState` decision to the widget tree. For the two empty
/// variants this mutates the single shared `StatusPage`'s title,
/// description, and icon in place before switching the stack to it, rather
/// than maintaining a third stack page — the empty page's layout role
/// (centered icon + title + description, `vexpand`) never changes, only its
/// copy does, so swapping three properties on one widget is simpler than
/// building and switching between two near-identical `StatusPage`s.
fn apply_empty_state(shared: &Rc<Shared>, state: EmptyState) {
    match state {
        EmptyState::List => {
            shared.stack.set_visible_child_name(STACK_PAGE_LIST);
        }
        EmptyState::EmptyLibrary => {
            shared.empty_page.set_icon_name(Some(ICON_EMPTY_LIBRARY));
            shared.empty_page.set_title(strings::EMPTY_LIBRARY_TITLE);
            shared
                .empty_page
                .set_description(Some(strings::EMPTY_LIBRARY_DESCRIPTION));
            shared.stack.set_visible_child_name(STACK_PAGE_EMPTY);
        }
        EmptyState::NoResults => {
            shared.empty_page.set_icon_name(Some(ICON_NO_RESULTS));
            shared.empty_page.set_title(strings::NO_RESULTS_TITLE);
            shared
                .empty_page
                .set_description(Some(strings::NO_RESULTS_DESCRIPTION));
            shared.stack.set_visible_child_name(STACK_PAGE_EMPTY);
        }
    }
    // Debug level (not info): this fires on every reload, including every
    // keystroke-debounced search, so it would be noisy at the default log
    // level — but it's exactly what a headless run needs to assert which
    // empty state (if any) is currently shown.
    tracing::debug!(?state, "track list empty-state page selected");
}

#[cfg(test)]
mod empty_state_tests {
    use super::*;

    #[test]
    fn empty_library_when_no_rows_and_no_filter() {
        assert_eq!(empty_state_for(0, false), EmptyState::EmptyLibrary);
    }

    #[test]
    fn no_results_when_no_rows_and_filter_active() {
        assert_eq!(empty_state_for(0, true), EmptyState::NoResults);
    }

    #[test]
    fn list_when_rows_present_regardless_of_filter() {
        assert_eq!(empty_state_for(3, false), EmptyState::List);
        assert_eq!(empty_state_for(3, true), EmptyState::List);
    }
}
