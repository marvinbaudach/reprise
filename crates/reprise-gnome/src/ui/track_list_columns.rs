//! `GtkColumnView` view widgets for `ui::track_list`: the seven text columns
//! (`append_column`), the interactive rating column (`append_rating_column`
//! with its `on_rating_changed` write-back), the shared empty-state
//! placeholder (`build_status_page`), and the empty-state decision and
//! application that drives that placeholder (`EmptyState`, `empty_state_for`,
//! `apply_empty_state`). Split out of `track_list.rs` as a focused sibling;
//! every function `TrackList::new`/`reload` needs is `pub(super)` while the
//! `Shared` state and reload orchestration stay in `track_list.rs`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;

use crate::ui::cover_loader::CoverLoader;
use crate::ui::rating::RatingWidget;
use crate::ui::strings;
use crate::ui::track_list::{
    reload, show_toast, Shared, STACK_PAGE_EMPTY, STACK_PAGE_IMPORT_ERRORS, STACK_PAGE_LIST,
};
use crate::ui::track_list_context_menu;
use crate::ui::track_list_dnd;
use reprise_core::cover::ThumbnailSize;
use reprise_core::library::stats;
use reprise_core::models::Track;
use reprise_core::view_source::ViewSource;

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

/// Which page of the track-list `Stack` should be visible, and (for the
/// empty variants) which copy the shared `StatusPage` should carry. A plain
/// enum decided by a pure function (`empty_state_for`) so the selection
/// logic is unit-testable without a running GTK application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EmptyState {
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
pub(super) fn empty_state_for(
    row_count: usize,
    has_filter: bool,
    source: &ViewSource,
) -> EmptyState {
    match (row_count, has_filter) {
        (0, true) => EmptyState::NoResults,
        (0, false) => match source {
            ViewSource::Missing | ViewSource::ImportErrors => EmptyState::NothingHere,
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
pub(super) fn build_status_page() -> adw::StatusPage {
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
/// clicks can be mapped back to it. `right_align` additionally marks the
/// label with the "numeric" style class (tabular figures, GNOME convention
/// for right-aligned numeric columns such as file sizes/durations). Returns
/// the built column so `TrackList::new` can set the initial sort indicator
/// on the artist column. `shared`/`column_view` are threaded through to
/// `wire_context_menu_gesture` (Stage 3 Task 5) so a secondary click on this
/// column's cells opens the row context menu — see that function's doc
/// comment.
pub(super) fn append_column(
    column_view: &gtk4::ColumnView,
    shared: &Rc<Shared>,
    sort_id: &'static str,
    title: &str,
    xalign: f32,
    right_align: bool,
    render: impl Fn(&Track) -> String + 'static,
) -> gtk4::ColumnViewColumn {
    let factory = gtk4::SignalListItemFactory::new();

    let shared = shared.clone();
    let column_view_for_setup = column_view.clone();
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

/// Builds the leading cover-art column (Task 6): each cell is a
/// `gtk4::Image`, lazily fed off-thread by the shared `CoverLoader` (Task 4)
/// instead of rendering a `Track` field synchronously like `append_column`'s
/// `gtk::Label` cells. `TrackList::new` calls this FIRST, before any other
/// `append_column`/`append_rating_column`, so the cover lands as the
/// leftmost column.
///
/// ## Per-cell generation guard
///
/// `GtkColumnView` recycles cell widgets as rows scroll in and out of view:
/// the same `gtk4::Image` gets rebound (`connect_bind`) to a succession of
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
/// The counter can't live on the `gtk4::Image` cell itself — this cell has
/// no interactive state worth a bespoke `gtk::Box` subclass, unlike
/// `RatingWidget` below — so it's kept in a side table keyed by the cell's
/// `ListItem` pointer identity (`glib::object::ObjectExt::as_ptr`), inserted
/// on `connect_setup` and removed on `connect_teardown`. This is the safe
/// alternative to an unsafe GObject `set_data`/`data` qdata pair; see
/// `rating.rs`'s doc comment (`## Why a gtk::Box subclass, not a plain Rust
/// struct`) for why this codebase avoids that pattern elsewhere.
pub(super) fn append_cover_column(
    column_view: &gtk4::ColumnView,
    shared: &Rc<Shared>,
    loader: &Rc<CoverLoader>,
) -> gtk4::ColumnViewColumn {
    // Reserved for parity with the other column-builders' signature (and any
    // future need, e.g. reacting to a library rescan) — cover cells need no
    // per-row DB access today, unlike `append_rating_column`'s write-back.
    let _ = shared;

    let generations: Rc<RefCell<HashMap<usize, Rc<Cell<u64>>>>> =
        Rc::new(RefCell::new(HashMap::new()));

    let factory = gtk4::SignalListItemFactory::new();

    {
        let generations = generations.clone();
        factory.connect_setup(move |_, obj| {
            let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
                tracing::warn!("cover column setup: object is not a ListItem");
                return;
            };
            let image = gtk4::Image::new();
            image.set_pixel_size(32); // 48px cached texture in a 32pt cell — crisp, compact row
            CoverLoader::set_placeholder(&image);
            generations
                .borrow_mut()
                .insert(item.as_ptr() as usize, Rc::new(Cell::new(0u64)));
            item.set_child(Some(&image));
        });
    }

    {
        let generations = generations.clone();
        let loader = loader.clone();
        factory.connect_bind(move |_, obj| {
            let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
                tracing::warn!("cover column bind: object is not a ListItem");
                return;
            };
            let Some(image) = item.child().and_then(|w| w.downcast::<gtk4::Image>().ok()) else {
                tracing::warn!("cover column bind: list item child is not an Image");
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
            loader.load_into(&image, &track.path, ThumbnailSize::List, token, &generation);
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
pub(super) fn append_rating_column(
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
pub(super) fn apply_empty_state(shared: &Rc<Shared>, state: EmptyState) {
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
    fn nothing_here_for_missing_and_import_errors_with_no_rows_and_no_filter() {
        assert_eq!(
            empty_state_for(0, false, &ViewSource::Missing),
            EmptyState::NothingHere
        );
        assert_eq!(
            empty_state_for(0, false, &ViewSource::ImportErrors),
            EmptyState::NothingHere
        );
    }

    /// Non-Library, non-Missing/ImportErrors sources (Playlist, Smart,
    /// Queue) still get `EmptyLibrary`'s copy for now — a dedicated "this
    /// playlist has no tracks yet" message is left to a later stage (Task 4
    /// builds the sidebar that would make such a distinction meaningful).
    #[test]
    fn playlist_smart_and_queue_fall_back_to_empty_library_copy() {
        assert_eq!(
            empty_state_for(0, false, &ViewSource::Playlist(1)),
            EmptyState::EmptyLibrary
        );
        assert_eq!(
            empty_state_for(0, false, &ViewSource::Smart(1)),
            EmptyState::EmptyLibrary
        );
        assert_eq!(
            empty_state_for(0, false, &ViewSource::Queue),
            EmptyState::EmptyLibrary
        );
    }
}
