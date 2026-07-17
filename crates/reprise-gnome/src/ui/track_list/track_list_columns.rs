//! `GtkColumnView` view widgets for `ui::track_list`: the seven text columns
//! (`append_column`), the interactive rating column (`append_rating_column`
//! with its `on_rating_changed` write-back), the shared empty-state
//! placeholder (`build_status_page`), and the empty-state decision and
//! application that drives that placeholder (`EmptyState`, `empty_state_for`,
//! `apply_empty_state`). Split out of `track_list.rs` as a focused sibling;
//! every function `TrackList::new`/`reload` needs is `pub(in crate::ui)` while the
//! `Shared` state and reload orchestration stay in `track_list.rs`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;

use crate::ui::cover_loader::CoverLoader;
use crate::ui::eq_bars::{self, EqVariant};
use crate::ui::list_density;
use crate::ui::rating::RatingWidget;
use crate::ui::strings;
use crate::ui::track_cover::TrackCover;
use crate::ui::track_list::{
    reload, show_toast, Shared, STACK_PAGE_EMPTY, STACK_PAGE_IMPORT_ERRORS, STACK_PAGE_LIST,
};
use crate::ui::track_list_context_menu;
use crate::ui::track_list_dnd;
use crate::ui::track_list_row_interaction;
use reprise_core::cover::ThumbnailSize;
use reprise_core::library::stats;
use reprise_core::models::Track;
use reprise_core::view_source::ViewSource;

/// Marker class carried by every cell of the currently-playing row — drives
/// the accent row background. See `track_list_row_interaction.rs`'s CSS.
const NOW_PLAYING_CLASS: &str = "now-playing";
/// Extra class on the leading (cover) cell only, carrying the 2 px left-edge
/// accent indicator so it sits at the row's left edge without a per-row hunt.
const NOW_PLAYING_LEADING_CLASS: &str = "now-playing-leading";
/// Class on the title label of the playing row: bold + theme accent.
const NOW_PLAYING_TITLE_CLASS: &str = "now-playing-title";

/// Adds or removes `class` on `widget` to match `present` (idempotent, so a
/// recycled cell rebound to a different row always ends in the right state).
fn toggle_class(widget: &impl gtk4::prelude::IsA<gtk4::Widget>, class: &str, present: bool) {
    if present {
        widget.add_css_class(class);
    } else {
        widget.remove_css_class(class);
    }
}

/// Toggles the `.now-playing` marker on `cell` by comparing `track_id`
/// against `shared.playing_track_id`, returning whether this row is the
/// playing one. Called from every column's `connect_bind` — cells recycle, so
/// it must set *or* clear on each bind — which is also why a row scrolled into
/// view while it is the playing track is marked with no extra bookkeeping.
/// `leading` additionally carries the cover column's left-edge indicator.
pub(in crate::ui) fn apply_now_playing(
    cell: &impl gtk4::prelude::IsA<gtk4::Widget>,
    track_id: i64,
    shared: &Shared,
    leading: bool,
) -> bool {
    let playing = shared.playing_track_id.get() == Some(track_id);
    toggle_class(cell, NOW_PLAYING_CLASS, playing);
    if leading {
        toggle_class(cell, NOW_PLAYING_LEADING_CLASS, playing);
    }
    playing
}

/// Icon shown on the empty-library placeholder (nothing has been scanned
/// in yet).
const ICON_EMPTY_LIBRARY: &str = "folder-music-symbolic";
/// Icon shown when a search filter matched zero rows — distinct from the
/// empty-library icon so the two states also read differently at a glance.
const ICON_NO_RESULTS: &str = "system-search-symbolic";
/// Icon shown for the neutral "nothing here" state (`Missing`/`ImportErrors`
/// sources with no rows and no active filter) — distinct from both of the
/// above: this isn't "no music has been scanned in" nor "your search
/// matched nothing", just "this particular view has no members right now".
const ICON_NOTHING_HERE: &str = "dialog-information-symbolic";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RatingRefresh {
    Row,
    Query,
}

fn rating_refresh_for_sort(sort_field: &str) -> RatingRefresh {
    if sort_field == "rating" {
        RatingRefresh::Query
    } else {
        RatingRefresh::Row
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum CellAlignment {
    Text,
    Numeric,
}

impl CellAlignment {
    fn xalign(self) -> f32 {
        match self {
            Self::Text => 0.0,
            Self::Numeric => 0.5,
        }
    }

    fn uses_tabular_figures(self) -> bool {
        matches!(self, Self::Numeric)
    }
}

#[cfg(test)]
#[path = "track_list_columns_alignment_tests.rs"]
mod cell_alignment_tests;

/// Which page of the track-list `Stack` should be visible, and (for the
/// empty variants) which copy the shared `StatusPage` should carry. A plain
/// enum decided by a pure function (`empty_state_for`) so the selection
/// logic is unit-testable without a running GTK application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum EmptyState {
    /// The library itself has no tracks yet (no filter active either).
    EmptyLibrary,
    /// A neutral "nothing here" state (Stage 3 Task 3): the `Missing`/
    /// `ImportErrors` sources have no rows and no filter is active — unlike
    /// `EmptyLibrary`, this isn't about the library having no music at all,
    /// just this particular view having no members right now.
    NothingHere,
    /// The current source has rows, but the active search filter matched
    /// none.
    NoResults,
    /// QUE-4: the Queue source with nothing playing and nothing pending.
    /// Deliberately its own copy ("Nothing queued — play something"), one
    /// next step per FB-5, instead of the EmptyLibrary scan prompt.
    EmptyQueue,
    /// At least one row to show — the populated list page.
    List,
}

/// Pure decision of which empty state (or the populated list) applies for a
/// given result-row count, whether a search filter is currently active, and
/// which `ViewSource` is showing. Kept side-effect free and separate from
/// `reload`/`apply_empty_state` so it can be unit tested directly instead of
/// only through a live GTK stack. `source` only matters for the
/// zero-rows/no-filter case: `Missing`/`ImportErrors` get the neutral
/// `NothingHere` copy there instead of `EmptyLibrary`'s "no music yet"
/// (which would be a confusing thing to say about, e.g., a "no files are
/// currently missing" state — that's good news, not an invitation to scan a
/// folder).
pub(in crate::ui) fn empty_state_for(
    row_count: usize,
    has_filter: bool,
    source: &ViewSource,
) -> EmptyState {
    match (row_count, has_filter) {
        (0, true) => EmptyState::NoResults,
        (0, false) => match source {
            ViewSource::Missing
            | ViewSource::ImportErrors
            | ViewSource::Album { .. }
            | ViewSource::Artist(_) => EmptyState::NothingHere,
            ViewSource::Queue => EmptyState::EmptyQueue,
            _ => EmptyState::EmptyLibrary,
        },
        _ => EmptyState::List,
    }
}

/// Builds the shared empty-state placeholder, initially carrying the
/// empty-library copy (the state `TrackList::new`'s first `reload()` will
/// normally confirm, since there's no library yet on first launch).
/// `apply_empty_state` swaps its title/description/icon in place for the
/// no-results case rather than building a second widget.
pub(in crate::ui) fn build_status_page() -> adw::StatusPage {
    adw::StatusPage::builder()
        .icon_name(ICON_EMPTY_LIBRARY)
        .title(strings::text(strings::EMPTY_LIBRARY_TITLE))
        .description(strings::text(strings::EMPTY_LIBRARY_DESCRIPTION))
        .vexpand(true)
        .build()
}

/// Builds one `ColumnViewColumn` bound to a `SignalListItemFactory` that
/// renders a single `gtk::Label` per cell. `sort_id` is a whitelisted
/// `queries` sort field name, stashed on the column via `set_id` so header
/// clicks can be mapped back to it. Numeric alignment centers the value and
/// marks the label with the "numeric" style class for tabular figures. Returns
/// the built column so `TrackList::new` can set the initial sort indicator
/// on the artist column. `shared`/`column_view` are threaded through to
/// `wire_context_menu_gesture` (Stage 3 Task 5) so a secondary click on this
/// column's cells opens the row context menu — see that function's doc
/// comment.
pub(in crate::ui) fn append_column(
    column_view: &gtk4::ColumnView,
    shared: &Rc<Shared>,
    sort_id: &'static str,
    title: &str,
    alignment: CellAlignment,
    render: impl Fn(&Track) -> String + 'static,
) -> gtk4::ColumnViewColumn {
    let factory = gtk4::SignalListItemFactory::new();

    let shared_for_bind = shared.clone();
    let shared = shared.clone();
    let column_view_for_setup = column_view.clone();
    factory.connect_setup(move |_, obj| {
        let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
            tracing::warn!("track list column setup: object is not a ListItem");
            return;
        };
        let label = gtk4::Label::new(None);
        track_list_row_interaction::expand_to_cell(&label);
        label.set_xalign(alignment.xalign());
        if alignment.uses_tabular_figures() {
            label.add_css_class("numeric");
        }
        track_list_context_menu::wire_context_menu_gesture(
            &label,
            item,
            &shared,
            &column_view_for_setup,
        );
        // Stage 3 Task 6: drag-source (fill a playlist / reorder) and
        // drop-target (reorder) — see `ui::track_list_dnd`'s doc comment.
        track_list_dnd::wire_row_dnd(&label, item, &shared);
        item.set_child(Some(&label));
        list_density::inherit(&column_view_for_setup, &label);
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
        apply_now_playing(&label, track.id, &shared_for_bind, false);
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

/// Builds the Title column. Unlike the seven generic `append_column` text
/// columns, its cell is a `Box[eq-bars, label]` so the now-playing row shows
/// an animated equaliser before the title (hidden on every other row) and
/// renders the title bold + accent (`.now-playing-title`). The equaliser is
/// built once per cell (recycled with the row) and only shown when the bound
/// track is the playing one; its pause is driven by the `.playback-paused`
/// class on the `ColumnView` (see `TrackList::set_playback_paused`), never
/// per cell. Same sorter/context-menu/drag wiring as `append_column`.
pub(in crate::ui) fn append_title_column(
    column_view: &gtk4::ColumnView,
    shared: &Rc<Shared>,
) -> gtk4::ColumnViewColumn {
    let factory = gtk4::SignalListItemFactory::new();

    let shared_for_bind = shared.clone();
    let shared = shared.clone();
    let column_view_for_setup = column_view.clone();
    factory.connect_setup(move |_, obj| {
        let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
            tracing::warn!("title column setup: object is not a ListItem");
            return;
        };
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        track_list_row_interaction::expand_to_cell(&row);
        let eq = eq_bars::build(EqVariant::Animated);
        eq.set_visible(false);
        let label = gtk4::Label::new(None);
        label.set_xalign(0.0);
        label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        label.set_hexpand(true);
        row.append(&eq);
        row.append(&label);
        track_list_context_menu::wire_context_menu_gesture(
            &row,
            item,
            &shared,
            &column_view_for_setup,
        );
        track_list_dnd::wire_row_dnd(&row, item, &shared);
        item.set_child(Some(&row));
        list_density::inherit(&column_view_for_setup, &row);
    });

    factory.connect_bind(move |_, obj| {
        let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
            tracing::warn!("title column bind: object is not a ListItem");
            return;
        };
        let Some(row) = item.child().and_then(|w| w.downcast::<gtk4::Box>().ok()) else {
            tracing::warn!("title column bind: list item child is not a Box");
            return;
        };
        let Some(eq) = row.first_child() else {
            tracing::warn!("title column bind: title cell has no equaliser child");
            return;
        };
        let Some(label) = eq
            .next_sibling()
            .and_then(|w| w.downcast::<gtk4::Label>().ok())
        else {
            tracing::warn!("title column bind: title cell has no label child");
            return;
        };
        let Some(boxed) = item
            .item()
            .and_then(|o| o.downcast::<glib::BoxedAnyObject>().ok())
        else {
            tracing::warn!("title column bind: item is not a BoxedAnyObject<Track>");
            return;
        };
        let track = boxed.borrow::<Track>();
        label.set_text(&track.title);
        // One comparison drives all three now-playing affordances: the cell
        // background (via `.now-playing` on the row box), the equaliser's
        // visibility, and the bold-accent title.
        let playing = apply_now_playing(&row, track.id, &shared_for_bind, false);
        eq.set_visible(playing);
        toggle_class(&label, NOW_PLAYING_TITLE_CLASS, playing);
    });

    let column = gtk4::ColumnViewColumn::builder()
        .title(strings::text(strings::COLUMN_TITLE))
        .factory(&factory)
        .resizable(true)
        .build();
    column.set_id(Some("title"));

    // Dummy sorter: makes the header clickable/toggleable without ever
    // reordering the model itself (SQL is the sort source of truth).
    let never_sorts = gtk4::CustomSorter::new(|_, _| gtk4::Ordering::Equal);
    column.set_sorter(Some(&never_sorts));

    column_view.append_column(&column);
    column
}

/// Builds the leading cover-art column (Task 6): each cell is a
/// density-aware `TrackCover`, lazily fed off-thread by the shared
/// `CoverLoader` (Task 4)
/// instead of rendering a `Track` field synchronously like `append_column`'s
/// `gtk::Label` cells. `TrackList::new` calls this FIRST, before any other
/// `append_column`/`append_rating_column`, so the cover lands as the
/// leftmost column.
///
/// ## Per-cell generation guard
///
/// `GtkColumnView` recycles cell widgets as rows scroll in and out of view:
/// the same `TrackCover` gets rebound (`connect_bind`) to a succession of
/// different `Track`s over its lifetime. Cover loading is async
/// (`CoverLoader::load_into` decodes/thumbnails off the main loop), so a
/// slow load started for track A can complete after the cell has already
/// been recycled to show track B — without a guard, that late result would
/// paint A's cover into B's row. Each cell gets its own generation counter,
/// bumped on every bind before the load is kicked off; `CoverLoader::
/// load_into` compares the token it was given against the counter's *current*
/// value when the async result lands and drops it if they no longer match
/// (see that function's doc comment).
///
/// The counter remains binding state rather than presentation state, so it
/// is kept in a side table keyed by the cell's `ListItem` pointer identity
/// (`glib::object::ObjectExt::as_ptr`), inserted on `connect_setup` and
/// removed on `connect_teardown`. This also avoids an unsafe GObject
/// `set_data`/`data` qdata pair.
pub(in crate::ui) fn append_cover_column(
    column_view: &gtk4::ColumnView,
    shared: &Rc<Shared>,
    loader: &Rc<CoverLoader>,
) -> gtk4::ColumnViewColumn {
    let generations: Rc<RefCell<HashMap<usize, Rc<Cell<u64>>>>> =
        Rc::new(RefCell::new(HashMap::new()));

    let factory = gtk4::SignalListItemFactory::new();

    {
        let generations = generations.clone();
        let shared = shared.clone();
        let column_view = column_view.clone();
        factory.connect_setup(move |_, obj| {
            let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
                tracing::warn!("cover column setup: object is not a ListItem");
                return;
            };
            let cover = TrackCover::new();
            track_list_row_interaction::expand_to_cell(&cover);
            cover.set_placeholder();
            track_list_context_menu::wire_context_menu_gesture(&cover, item, &shared, &column_view);
            track_list_dnd::wire_row_dnd(&cover, item, &shared);
            generations
                .borrow_mut()
                .insert(item.as_ptr() as usize, Rc::new(Cell::new(0u64)));
            item.set_child(Some(&cover));
            list_density::inherit(&column_view, &cover);
        });
    }

    {
        let generations = generations.clone();
        let loader = loader.clone();
        let shared = shared.clone();
        factory.connect_bind(move |_, obj| {
            let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
                tracing::warn!("cover column bind: object is not a ListItem");
                return;
            };
            let Some(cover) = item.child().and_then(|w| w.downcast::<TrackCover>().ok()) else {
                tracing::warn!("cover column bind: list item child is not a TrackCover");
                return;
            };
            let Some(boxed) = item
                .item()
                .and_then(|o| o.downcast::<glib::BoxedAnyObject>().ok())
            else {
                tracing::warn!("cover column bind: item is not a BoxedAnyObject<Track>");
                return;
            };
            let track = boxed.borrow::<Track>();
            apply_now_playing(&cover, track.id, &shared, true);

            let key = item.as_ptr() as usize;
            let generation = generations
                .borrow_mut()
                .entry(key)
                .or_insert_with(|| Rc::new(Cell::new(0u64)))
                .clone();
            // Bump before kicking off the load: this is the token the async
            // result must still match when it lands (see the doc comment
            // above) — a stale-in-flight load for whatever track this cell
            // previously showed is dropped by `load_into`'s own check.
            let token = generation.get().wrapping_add(1);
            generation.set(token);
            loader.load_into_track_cover(
                &cover,
                &track.path,
                ThumbnailSize::List,
                token,
                &generation,
            );
        });
    }

    {
        let generations = generations.clone();
        factory.connect_teardown(move |_, obj| {
            let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
                return;
            };
            generations.borrow_mut().remove(&(item.as_ptr() as usize));
        });
    }

    let column = gtk4::ColumnViewColumn::builder()
        .title("")
        .factory(&factory)
        .resizable(false)
        .build();
    column.set_fixed_width(40);

    column_view.append_column(&column);
    column
}

/// Builds the interactive `Rating` column: each cell is a `RatingWidget`
/// (`ui::rating`) instead of a `gtk::Label` — the one column whose factory
/// writes back to the database on user interaction rather than only
/// rendering a `Track` field. Requires a fully-built `shared` (its
/// `conn`/`model` are used by the click handler), which is why
/// `TrackList::new` calls this after constructing `Shared`, unlike the
/// other seven columns built by `append_column` beforehand.
pub(in crate::ui) fn append_rating_column(
    column_view: &gtk4::ColumnView,
    shared: &Rc<Shared>,
) -> gtk4::ColumnViewColumn {
    let factory = gtk4::SignalListItemFactory::new();

    {
        let shared = shared.clone();
        let column_view = column_view.clone();
        factory.connect_setup(move |_, obj| {
            let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
                tracing::warn!("rating column setup: object is not a ListItem");
                return;
            };
            let rating_widget = RatingWidget::new();
            track_list_row_interaction::expand_to_cell(&rating_widget);
            // Secondary-click (button 3) context menu (Stage 3 Task 5) —
            // the rating column's own stars only ever respond to primary-
            // button clicks (`gtk::Button`'s default), so this can never
            // steal a rating click. See `wire_context_menu_gesture`'s doc
            // comment.
            track_list_context_menu::wire_context_menu_gesture(
                &rating_widget,
                item,
                &shared,
                &column_view,
            );
            // Stage 3 Task 6: same drag-source/drop-target wiring as the
            // seven text columns — see `ui::track_list_dnd`'s doc comment.
            track_list_dnd::wire_row_dnd(&rating_widget, item, &shared);
            item.set_child(Some(&rating_widget));
            list_density::inherit(&column_view, &rating_widget);
        });
    }

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
            apply_now_playing(&rating_widget, track.id, &shared, false);

            let track_id = track.id;
            let title = track.title.clone();
            let position = item.position();
            let shared = shared.clone();
            rating_widget.set_on_changed(move |new_rating| {
                on_rating_changed(&shared, track_id, &title, position, new_rating);
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
        .title(strings::text(strings::RATING))
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
/// `TrackListModel::invalidate_window_at`). A write failure is logged and,
/// since Stage 3 Task 1 (backlog item a), also surfaced as a toast: the
/// displayed rating already reflects the click (`RatingWidget::set_rating`
/// ran first), so without a toast the user couldn't tell the write didn't
/// persist until scrolling away and back; never crashes or wedges the UI
/// either way (fault tolerance).
fn on_rating_changed(
    shared: &Rc<Shared>,
    track_id: i64,
    title: &str,
    position: u32,
    new_rating: i32,
) {
    tracing::debug!(track_id, position, new_rating, "rating changed");
    let result = {
        let conn = shared.conn.borrow();
        stats::set_rating(&conn, track_id, new_rating)
    };
    match result {
        Ok(()) => {
            let refresh = rating_refresh_for_sort(&shared.sort.borrow().field);
            match refresh {
                RatingRefresh::Row => shared.model.invalidate_window_at(position),
                RatingRefresh::Query => reload(shared),
            }
        }
        Err(error) => {
            tracing::error!(%error, track_id, new_rating, "failed to persist rating change");
            show_toast(shared, &strings::rating_save_failed_toast(title));
        }
    }
}

#[cfg(test)]
mod rating_refresh_tests {
    use super::*;

    #[test]
    fn rating_sort_requires_query_reload_but_other_sorts_need_one_row() {
        assert_eq!(rating_refresh_for_sort("rating"), RatingRefresh::Query);
        assert_eq!(rating_refresh_for_sort("title"), RatingRefresh::Row);
    }
}

/// Applies an `EmptyState` decision to the widget tree. For the two empty
/// variants this mutates the single shared `StatusPage`'s title,
/// description, and icon in place before switching the stack to it, rather
/// than maintaining a third stack page — the empty page's layout role
/// (centered icon + title + description, `vexpand`) never changes, only its
/// copy does, so swapping three properties on one widget is simpler than
/// building and switching between two near-identical `StatusPage`s.
pub(in crate::ui) fn apply_empty_state(shared: &Rc<Shared>, state: EmptyState) {
    match state {
        EmptyState::List => {
            // Stage 3 Task 8: the ImportErrors source's populated page is the
            // dedicated panel, not the shared `ColumnView` page — every other
            // source keeps using `STACK_PAGE_LIST` exactly as before.
            let page = if matches!(*shared.source.borrow(), ViewSource::ImportErrors) {
                STACK_PAGE_IMPORT_ERRORS
            } else {
                STACK_PAGE_LIST
            };
            shared.stack.set_visible_child_name(page);
        }
        EmptyState::EmptyLibrary => {
            shared.empty_page.set_icon_name(Some(ICON_EMPTY_LIBRARY));
            shared
                .empty_page
                .set_title(&strings::text(strings::EMPTY_LIBRARY_TITLE));
            shared
                .empty_page
                .set_description(Some(&strings::text(strings::EMPTY_LIBRARY_DESCRIPTION)));
            shared.stack.set_visible_child_name(STACK_PAGE_EMPTY);
        }
        EmptyState::NoResults => {
            shared.empty_page.set_icon_name(Some(ICON_NO_RESULTS));
            shared
                .empty_page
                .set_title(&strings::text(strings::NO_RESULTS_TITLE));
            shared
                .empty_page
                .set_description(Some(&strings::text(strings::NO_RESULTS_DESCRIPTION)));
            shared.stack.set_visible_child_name(STACK_PAGE_EMPTY);
        }
        EmptyState::EmptyQueue => {
            shared.empty_page.set_icon_name(Some(ICON_NOTHING_HERE));
            shared
                .empty_page
                .set_title(&strings::text(strings::EMPTY_QUEUE_TITLE));
            shared
                .empty_page
                .set_description(Some(&strings::text(strings::EMPTY_QUEUE_DESCRIPTION)));
            shared.stack.set_visible_child_name(STACK_PAGE_EMPTY);
        }
        EmptyState::NothingHere => {
            shared.empty_page.set_icon_name(Some(ICON_NOTHING_HERE));
            shared
                .empty_page
                .set_title(&strings::text(strings::NOTHING_HERE_TITLE));
            shared
                .empty_page
                .set_description(Some(&strings::text(strings::NOTHING_HERE_DESCRIPTION)));
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
    fn empty_library_when_no_rows_and_no_filter_for_library_source() {
        assert_eq!(
            empty_state_for(0, false, &ViewSource::Library),
            EmptyState::EmptyLibrary
        );
    }

    #[test]
    fn no_results_when_no_rows_and_filter_active_regardless_of_source() {
        assert_eq!(
            empty_state_for(0, true, &ViewSource::Library),
            EmptyState::NoResults
        );
        assert_eq!(
            empty_state_for(0, true, &ViewSource::Missing),
            EmptyState::NoResults
        );
        assert_eq!(
            empty_state_for(0, true, &ViewSource::ImportErrors),
            EmptyState::NoResults
        );
    }

    #[test]
    fn list_when_rows_present_regardless_of_filter_or_source() {
        assert_eq!(
            empty_state_for(3, false, &ViewSource::Library),
            EmptyState::List
        );
        assert_eq!(
            empty_state_for(3, true, &ViewSource::Missing),
            EmptyState::List
        );
    }

    /// Stage 3 Task 3: `Missing`/`ImportErrors` get the neutral "nothing
    /// here" copy for the zero-rows/no-filter case, not `EmptyLibrary`'s
    /// "no music yet" (which would read oddly for "no files are currently
    /// missing").
    #[test]
    fn nothing_here_for_transient_or_issue_sources_with_no_rows_and_no_filter() {
        assert_eq!(
            empty_state_for(0, false, &ViewSource::Missing),
            EmptyState::NothingHere
        );
        assert_eq!(
            empty_state_for(0, false, &ViewSource::ImportErrors),
            EmptyState::NothingHere
        );
        assert_eq!(
            empty_state_for(
                0,
                false,
                &ViewSource::Album {
                    album: "Blue".into(),
                    album_artist: "Joni Mitchell".into(),
                },
            ),
            EmptyState::NothingHere
        );
        assert_eq!(
            empty_state_for(0, false, &ViewSource::Artist("Björk".into())),
            EmptyState::NothingHere
        );
    }

    /// Non-Library, non-Missing/ImportErrors sources (Playlist, Smart)
    /// still get `EmptyLibrary`'s copy for now — a dedicated "this playlist
    /// has no tracks yet" message is left to a later stage. The Queue source
    /// has its own QUE-4 copy since the queue+nav plan.
    #[test]
    fn playlist_and_smart_fall_back_to_empty_library_copy() {
        assert_eq!(
            empty_state_for(0, false, &ViewSource::Playlist(1)),
            EmptyState::EmptyLibrary
        );
        assert_eq!(
            empty_state_for(0, false, &ViewSource::Smart(1)),
            EmptyState::EmptyLibrary
        );
    }

    #[test]
    fn empty_queue_gets_its_own_que4_state() {
        assert_eq!(
            empty_state_for(0, false, &ViewSource::Queue),
            EmptyState::EmptyQueue
        );
    }
}
