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

use super::now_playing_marker;
use super::rating_cell_refresh;
use crate::ui::cover_loader::CoverLoader;
use crate::ui::list_density;
use crate::ui::playing_marker;
use crate::ui::rating::RatingWidget;
use crate::ui::strings;
use crate::ui::track_cover::TrackCover;
use crate::ui::track_list::{reload, show_toast, Shared};
use crate::ui::track_list_context_menu;
use crate::ui::track_list_dnd;
use crate::ui::track_list_row_interaction;
use reprise_core::cover::ThumbnailSize;
use reprise_core::library::stats;
use reprise_core::models::{MissingReason, Track};

/// Marker class carried by every cell of the currently-playing row — drives
/// the accent row background. See `track_list_row_interaction.rs`'s CSS.
const NOW_PLAYING_CLASS: &str = "now-playing";
/// Extra class on the leading (cover) cell only, carrying the 2 px left-edge
/// accent indicator so it sits at the row's left edge without a per-row hunt.
const NOW_PLAYING_LEADING_CLASS: &str = "now-playing-leading";
/// Class on the title label of the playing row: bold + theme accent.
const NOW_PLAYING_TITLE_CLASS: &str = "now-playing-title";
/// Class on a missing track's title label; set and cleared on every bind.
const MISSING_TRACK_TITLE_CLASS: &str = "missing-track-title";

/// Adds or removes `class` on `widget` to match `present` (idempotent, so a
/// recycled cell rebound to a different row always ends in the right state).
fn toggle_class(widget: &impl gtk4::prelude::IsA<gtk4::Widget>, class: &str, present: bool) {
    if present {
        widget.add_css_class(class);
    } else {
        widget.remove_css_class(class);
    }
}

/// Human explanation shared by the missing-row tooltip and the explicit
/// activation feedback. Presence is decided by `missing_since`; a missing
/// reason absent from an old/migrated row degrades honestly to `Unknown`.
pub(in crate::ui) fn missing_track_explanation(
    missing_since: Option<i64>,
    reason: Option<MissingReason>,
) -> Option<String> {
    let missing_since = missing_since?;
    match reason.unwrap_or(MissingReason::Unknown) {
        MissingReason::Unmounted => Some(strings::issue_text(strings::MISSING_ROW_UNAVAILABLE)),
        MissingReason::Deleted | MissingReason::Unknown => {
            let date = reprise_core::format::format_unix_timestamp(missing_since);
            Some(strings::missing_row_file_since(&date))
        }
    }
}

fn apply_missing_title(label: &gtk4::Label, track: &Track) {
    let missing = track.is_missing();
    toggle_class(label, MISSING_TRACK_TITLE_CLASS, missing);
    let attributes = missing.then(|| {
        let attributes = gtk4::pango::AttrList::new();
        attributes.insert(gtk4::pango::AttrInt::new_strikethrough(true));
        attributes
    });
    label.set_attributes(attributes.as_ref());
    let explanation = missing_track_explanation(track.missing_since, track.missing_reason);
    label.set_tooltip_text(explanation.as_deref());
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
    let shared_for_unbind = shared.clone();
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
        let raw = render(&track);
        let markup = if super::match_highlight::is_searchable_column(sort_id) {
            let needle = shared_for_bind.filter.borrow().clone();
            super::match_highlight::highlight_markup(
                &raw,
                &needle,
                super::match_highlight::accent_foreground(&label).as_deref(),
            )
        } else {
            None
        };
        match markup {
            Some(markup) => label.set_markup(&markup),
            None => label.set_text(&raw),
        }
        apply_now_playing(&label, track.id, &shared_for_bind, false);
        now_playing_marker::register_cell(&shared_for_bind, item, {
            let label = label.clone();
            let track_id = track.id;
            move |shared| {
                apply_now_playing(&label, track_id, shared, false);
            }
        });
    });

    // GtkColumnView pools unbound cells; drop this cell's marker entry on
    // unbind so the now-playing registry (and the widgets its re-appliers
    // capture) stays bounded to visible cells instead of leaking one screenful
    // per re-sort. See `now_playing_marker::unregister_cell`.
    factory.connect_unbind(move |_, obj| {
        if let Some(item) = obj.downcast_ref::<gtk4::ListItem>() {
            now_playing_marker::unregister_cell(&shared_for_unbind, item);
        }
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
    let shared_for_unbind_title = shared.clone();
    let shared = shared.clone();
    let column_view_for_setup = column_view.clone();
    factory.connect_setup(move |_, obj| {
        let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
            tracing::warn!("title column setup: object is not a ListItem");
            return;
        };
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        track_list_row_interaction::expand_to_cell(&row);
        let eq = playing_marker::build();
        eq.set_visible(false);
        let label = gtk4::Label::new(None);
        label.set_xalign(0.0);
        label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        label.set_hexpand(true);
        row.append(&eq);
        row.append(&label);
        // INST-10: a compact "AI" badge after the title, hidden until a bound
        // row is AI-manipulated. `stats-badge` is the shared accent-pill style.
        let ai_badge = gtk4::Label::new(Some(&strings::text(strings::AI_BADGE_LABEL)));
        ai_badge.add_css_class("stats-badge");
        ai_badge.set_tooltip_text(Some(&strings::text(strings::AI_BADGE_TOOLTIP)));
        ai_badge.set_visible(false);
        row.append(&ai_badge);
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
        if track.is_missing() {
            // Missing rows are de-emphasised (grey + strikethrough per the
            // missing rules); the plain title carries no search highlight.
            label.set_text(&track.title);
            apply_missing_title(&label, &track);
        } else {
            // Clear any leftover missing styling from a recycled row FIRST
            // (class off, attributes cleared, tooltip cleared), then let the
            // search-match highlight own the label's final markup.
            apply_missing_title(&label, &track);
            let needle = shared_for_bind.filter.borrow().clone();
            match super::match_highlight::highlight_markup(
                &track.title,
                &needle,
                super::match_highlight::accent_foreground(&label).as_deref(),
            ) {
                Some(markup) => label.set_markup(&markup),
                None => label.set_text(&track.title),
            }
        }
        // One comparison drives all three now-playing affordances: the cell
        // background (via `.now-playing` on the row box), the equaliser's
        // visibility, and the bold-accent title.
        let playing = apply_now_playing(&row, track.id, &shared_for_bind, false);
        eq.set_visible(playing);
        toggle_class(&label, NOW_PLAYING_TITLE_CLASS, playing);
        // INST-10: the AI badge (the label's trailing sibling) follows the row's
        // provenance flag, gated on the live experimental switch (INST-11).
        if let Some(ai_badge) = label.next_sibling() {
            let experimental_on =
                crate::ui::experimental::experimental_enabled(&shared_for_bind.conn);
            ai_badge.set_visible(ai_badge_visible(experimental_on, track.is_ai));
        }
        now_playing_marker::register_cell(&shared_for_bind, item, {
            let row = row.clone();
            let eq = eq.clone();
            let label = label.clone();
            let track_id = track.id;
            move |shared| {
                let playing = apply_now_playing(&row, track_id, shared, false);
                eq.set_visible(playing);
                toggle_class(&label, NOW_PLAYING_TITLE_CLASS, playing);
            }
        });
    });

    // Drop this cell's marker entry on unbind so the registry stays bounded to
    // visible cells (see `now_playing_marker::unregister_cell`).
    factory.connect_unbind(move |_, obj| {
        if let Some(item) = obj.downcast_ref::<gtk4::ListItem>() {
            now_playing_marker::unregister_cell(&shared_for_unbind_title, item);
        }
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
            let accessible_label = crate::ui::strings::formatted(
                crate::ui::strings::GO_TO_ALBUM_NAMED,
                &[("album", &track.album)],
            );
            cover.update_property(&[gtk4::accessible::Property::Label(&accessible_label)]);
            apply_now_playing(&cover, track.id, &shared, true);
            now_playing_marker::register_cell(&shared, item, {
                let cover = cover.clone();
                let track_id = track.id;
                move |shared| {
                    apply_now_playing(&cover, track_id, shared, true);
                }
            });

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
        // Drop this cell's marker entry on unbind so the registry stays bounded
        // to visible cells (see `now_playing_marker::unregister_cell`).
        let shared = shared.clone();
        factory.connect_unbind(move |_, obj| {
            if let Some(item) = obj.downcast_ref::<gtk4::ListItem>() {
                now_playing_marker::unregister_cell(&shared, item);
            }
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
            rating_cell_refresh::register_cell(&shared, item, track.id, {
                let rating_widget = rating_widget.clone();
                move |rating| rating_widget.set_rating(rating)
            });
            apply_now_playing(&rating_widget, track.id, &shared, false);
            now_playing_marker::register_cell(&shared, item, {
                let rating_widget = rating_widget.clone();
                let track_id = track.id;
                move |shared| {
                    apply_now_playing(&rating_widget, track_id, shared, false);
                }
            });

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
    //
    // Also drop this cell's now-playing marker entry so the registry stays
    // bounded to visible cells (see `now_playing_marker::unregister_cell`).
    let shared_for_unbind_rating = shared.clone();
    factory.connect_unbind(move |_, obj| {
        let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        now_playing_marker::unregister_cell(&shared_for_unbind_rating, item);
        rating_cell_refresh::unregister_cell(&shared_for_unbind_rating, item);
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

/// Persists a rating change via `library::stats::set_rating` and patches the
/// model's cached copy of the affected row on success. A write failure is logged and,
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
        let conn = &shared.conn;
        stats::set_rating(conn, track_id, new_rating)
    };
    match result {
        Ok(()) => {
            let refresh = rating_refresh_for_sort(&shared.sort.borrow().field);
            match refresh {
                // The star widget already shows the new rating (set on click,
                // above); only the model's cached clone is stale. Patch it in
                // place: a one-row `items_changed` would replace the row widget
                // under the pointer and snap the viewport to the top.
                RatingRefresh::Row => shared.model.set_cached_rating(position, new_rating),
                RatingRefresh::Query => reload(shared),
            }
        }
        Err(error) => {
            tracing::error!(%error, track_id, new_rating, "failed to persist rating change");
            show_toast(shared, &strings::rating_save_failed_toast(title));
        }
    }
}

/// INST-10 + INST-11: the AI badge shows only for an AI-manipulated track and
/// only while the experimental switch is on (the badge is instrumental UI). A
/// pure decision so the rule is testable without realising a ColumnView cell.
fn ai_badge_visible(experimental_on: bool, is_ai: bool) -> bool {
    experimental_on && is_ai
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

#[cfg(test)]
mod missing_track_tests {
    use reprise_core::models::MissingReason;

    use super::*;

    #[test]
    fn missing_track_explanation_distinguishes_unavailable_drive_from_missing_file() {
        assert_eq!(
            missing_track_explanation(Some(1_000_000_000), Some(MissingReason::Unmounted)),
            Some("On unavailable drive — returns when mounted".into())
        );
        for reason in [MissingReason::Deleted, MissingReason::Unknown] {
            assert_eq!(
                missing_track_explanation(Some(1_000_000_000), Some(reason)),
                Some("File missing since 2001-09-09 01:46".into())
            );
        }
        assert_eq!(missing_track_explanation(None, None), None);
    }

    #[test]
    fn missing_title_css_uses_half_opacity() {
        let css = crate::ui::track_list_row_interaction::css();
        assert!(css.contains(".missing-track-title"));
        assert!(css.contains("opacity: 0.5"));
    }
}

#[cfg(test)]
mod ai_badge_tests {
    use super::ai_badge_visible;

    // UX INST-10: the AI badge renders for AI-manipulated tracks, and (INST-11)
    // only while the experimental switch is on — never on a plain track, never
    // when the switch is off.
    #[test]
    fn inst_10_ai_badge_shows_only_for_ai_tracks_with_experimental_on() {
        assert!(
            ai_badge_visible(true, true),
            "an AI track with experimental on shows the badge"
        );
        assert!(
            !ai_badge_visible(true, false),
            "a plain track shows no badge"
        );
        assert!(
            !ai_badge_visible(false, true),
            "experimental off hides the badge (INST-11 master gate)"
        );
        assert!(!ai_badge_visible(false, false));
    }
}
