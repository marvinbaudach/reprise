//! The sortable, searchable track list: a `GtkColumnView` backed by a
//! `gio::ListStore` that is refilled from `queries::query_track_window` on
//! every sort/search change — SQL stays the single source of truth for
//! ordering and filtering, GTK never re-sorts the model itself.
//!
//! ## Row data: `glib::BoxedAnyObject`, not a GObject subclass
//!
//! `models::Track` is a plain Rust struct. Wrapping it for `gio::ListStore`
//! only requires *something* that is `IsA<glib::Object>`; a full
//! `glib::Object` subclass with GObject properties would be needed if the
//! bound widgets had to react to property-level `notify::` signals (e.g. for
//! in-place editing) or if the object needed to cross an FFI/property-binding
//! boundary. Neither applies here — the factory callbacks just read a
//! `Track` once per bind — so `glib::BoxedAnyObject::new(track)` is the
//! simplest correct approach and there is no separate `track_object.rs`
//! module (a bespoke wrapper type would add boilerplate without behavior).
//!
//! ## Sorting: per-column `CustomSorter` as a click signal only
//!
//! `GtkColumnView` headers only become clickable/toggle-sortable once a
//! column has a non-null `sorter`. This module gives every column a
//! `gtk::CustomSorter` whose compare function always returns `Equal` — it
//! never actually reorders the `gio::ListStore` — purely so GTK renders the
//! sort indicator and emits sort-order changes on click. The real ordering
//! is decided by SQL: clicking a header changes `ColumnView`'s aggregate
//! `ColumnViewSorter` (`primary-sort-column`/`primary-sort-order`), which
//! this module observes, maps back to a whitelisted `queries` sort field via
//! `ColumnViewColumn::id()`, and uses to re-run `query_track_window`.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use rusqlite::Connection;

use crate::format::format_duration;
use crate::models::Track;
use crate::queries;
use crate::ui::strings;

/// Stage-1 window size (per the brief): a single fixed page of rows loaded
/// on every reload, always starting at offset 0. Full gapless virtualization
/// (loading additional pages as the user scrolls) is stage 2 and not
/// implemented here.
const WINDOW_LIMIT: i64 = 200;
const WINDOW_OFFSET: i64 = 0;

const STACK_PAGE_EMPTY: &str = "empty";
const STACK_PAGE_LIST: &str = "list";

/// Ratings are stored as a plain `i32` with no CHECK constraint; clamp to a
/// sane star-count range so corrupt/out-of-range data can never make the
/// label render an absurd number of stars.
const RATING_MIN: i32 = 0;
const RATING_MAX: i32 = 5;

/// Glyph used to render a star rating (not a translatable phrase — a visual
/// symbol, like the icon names used elsewhere — so it lives here rather than
/// in `strings.rs`).
const RATING_STAR: &str = "★";

#[derive(Debug, Clone)]
struct SortState {
    field: String,
    dir: String,
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

struct Shared {
    conn: Rc<RefCell<Connection>>,
    store: gio::ListStore,
    stack: gtk4::Stack,
    sort: RefCell<SortState>,
    filter: RefCell<String>,
}

/// Handle to the built track list widget. Owns the shared, reference-counted
/// state that the sort-header and search-debounce callbacks close over.
pub struct TrackList {
    shared: Rc<Shared>,
}

impl TrackList {
    /// Builds the track list and performs the initial load (unfiltered,
    /// default sort). `conn` is the shared UI-owned database connection.
    pub fn new(conn: Rc<RefCell<Connection>>) -> Self {
        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        let selection = gtk4::NoSelection::new(Some(store.clone()));

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
        append_column(
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
        append_column(
            &column_view,
            "rating",
            strings::COLUMN_RATING,
            0.0,
            false,
            |t| RATING_STAR.repeat(t.rating.clamp(RATING_MIN, RATING_MAX) as usize),
        );

        let scrolled = gtk4::ScrolledWindow::builder()
            .child(&column_view)
            .vexpand(true)
            .hexpand(true)
            .build();

        let empty_page = adw_status_page();

        let stack = gtk4::Stack::new();
        stack.add_named(&empty_page, Some(STACK_PAGE_EMPTY));
        stack.add_named(&scrolled, Some(STACK_PAGE_LIST));
        stack.set_visible_child_name(STACK_PAGE_EMPTY);

        let shared = Rc::new(Shared {
            conn,
            store,
            stack,
            sort: RefCell::new(SortState::default()),
            filter: RefCell::new(String::new()),
        });

        wire_sort_clicks(&column_view, &shared);
        wire_activate(&column_view, &shared);

        reload(&shared);

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
}

/// Builds the empty-library placeholder shown while the current window (the
/// unfiltered library, or a search with no matches) has zero rows.
fn adw_status_page() -> gtk4::Widget {
    use libadwaita as adw;
    adw::StatusPage::builder()
        .icon_name("folder-music-symbolic")
        .title(strings::EMPTY_LIBRARY_TITLE)
        .description(strings::EMPTY_LIBRARY_DESCRIPTION)
        .vexpand(true)
        .build()
        .upcast()
}

/// Builds one `ColumnViewColumn` bound to a `SignalListItemFactory` that
/// renders a single `gtk::Label` per cell. `sort_id` is a whitelisted
/// `queries` sort field name, stashed on the column via `set_id` so header
/// clicks can be mapped back to it. `right_align` additionally marks the
/// label with the "numeric" style class (tabular figures, GNOME convention
/// for right-aligned numeric columns such as file sizes/durations).
fn append_column(
    column_view: &gtk4::ColumnView,
    sort_id: &'static str,
    title: &str,
    xalign: f32,
    right_align: bool,
    render: impl Fn(&Track) -> String + 'static,
) {
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
    // reordering the ListStore itself (SQL is the sort source of truth —
    // see module doc comment).
    let never_sorts = gtk4::CustomSorter::new(|_, _| gtk4::Ordering::Equal);
    column.set_sorter(Some(&never_sorts));

    column_view.append_column(&column);
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
    *shared.sort.borrow_mut() = SortState {
        field: id.to_string(),
        dir: dir.to_string(),
    };
    reload(shared);
}

/// Row activation (double-click or Enter on a focused row). The player lands
/// in Task 9 — for now this is just an observable log line.
fn wire_activate(column_view: &gtk4::ColumnView, shared: &Rc<Shared>) {
    let shared = shared.clone();
    column_view.connect_activate(move |view, position| {
        let Some(model) = view.model() else {
            return;
        };
        let Some(item) = model.item(position) else {
            tracing::warn!(position, "track list activate: no item at position");
            return;
        };
        let Some(boxed) = item.downcast_ref::<glib::BoxedAnyObject>() else {
            tracing::warn!("track list activate: item is not a BoxedAnyObject<Track>");
            return;
        };
        let track = boxed.borrow::<Track>();
        tracing::info!(path = %track.path, "activate track");
        // Touching `shared` keeps its clone alive for the closure's lifetime
        // and documents that future tasks (player wiring) will use it.
        let _ = &shared;
    });
}

/// Re-runs the windowed query against the current sort/filter state and
/// replaces the `ListStore` contents. Shows the empty placeholder whenever
/// the result set (library or filtered view) has zero rows.
fn reload(shared: &Rc<Shared>) {
    let sort = shared.sort.borrow().clone();
    let filter = shared.filter.borrow().clone();

    let tracks = {
        let mut conn = shared.conn.borrow_mut();
        queries::query_track_window(
            &mut conn,
            &sort.field,
            &sort.dir,
            &filter,
            WINDOW_OFFSET,
            WINDOW_LIMIT,
        )
    };

    let tracks = match tracks {
        Ok(tracks) => tracks,
        Err(error) => {
            tracing::error!(%error, field = %sort.field, dir = %sort.dir, "failed to load track window");
            return;
        }
    };

    shared.store.remove_all();
    for track in &tracks {
        shared
            .store
            .append(&glib::BoxedAnyObject::new(track.clone()));
    }

    let count = tracks.len();
    shared.stack.set_visible_child_name(if count == 0 {
        STACK_PAGE_EMPTY
    } else {
        STACK_PAGE_LIST
    });

    tracing::info!(count, field = %sort.field, dir = %sort.dir, filter = %filter, "loaded {count} tracks");
}
