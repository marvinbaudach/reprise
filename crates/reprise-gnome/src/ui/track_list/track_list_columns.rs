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
pub(in crate::ui) use super::rating_column::append_rating_column;
use crate::ui::cover_loader::CoverLoader;
use crate::ui::list_density;
use crate::ui::playing_marker;
use crate::ui::strings;
use crate::ui::track_cover::TrackCover;
use crate::ui::track_list::Shared;
use crate::ui::track_list_context_menu;
use crate::ui::track_list_dnd;
use crate::ui::track_list_row_interaction;
use reprise_core::cover::ThumbnailSize;
use reprise_core::models::{MissingReason, Track};
use reprise_core::queries::QueueItemMetadata;

/// Marker class carried by every cell of the currently-playing row — drives
/// the accent row background. See `track_list_row_interaction.rs`'s CSS.
pub(super) const NOW_PLAYING_CLASS: &str = "now-playing";
/// Extra class on the leading (cover) cell only, carrying the 2 px left-edge
/// accent indicator so it sits at the row's left edge without a per-row hunt.
const NOW_PLAYING_LEADING_CLASS: &str = "now-playing-leading";
/// Class on the title label of the playing row: bold + theme accent.
pub(super) const NOW_PLAYING_TITLE_CLASS: &str = "now-playing-title";
/// Class on a missing track's title label; set and cleared on every bind.
const MISSING_TRACK_TITLE_CLASS: &str = "missing-track-title";

/// Adds or removes `class` on `widget` to match `present` (idempotent, so a
/// recycled cell rebound to a different row always ends in the right state).
pub(super) fn toggle_class(
    widget: &impl gtk4::prelude::IsA<gtk4::Widget>,
    class: &str,
    present: bool,
) {
    if present {
        widget.add_css_class(class);
    } else {
        widget.remove_css_class(class);
    }
}

/// Keeps the title factory on the one shared NAV-10a marker constructor even
/// though that factory lives in a size-cap sibling.
pub(super) fn build_playing_marker() -> gtk4::Box {
    playing_marker::build()
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

pub(super) fn apply_missing_title(label: &gtk4::Label, track: &Track) {
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

pub(super) fn clear_missing_title(label: &gtk4::Label) {
    toggle_class(label, MISSING_TRACK_TITLE_CLASS, false);
    label.set_attributes(None);
    label.set_tooltip_text(None);
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

pub(super) fn apply_now_playing_item(
    cell: &impl gtk4::prelude::IsA<gtk4::Widget>,
    item: &QueueItemMetadata,
    shared: &Shared,
    leading: bool,
) -> bool {
    let playing = super::queue_item_presentation::is_now_playing(
        item,
        shared.playing_track_id.get(),
        shared.playing_episode.get(),
    );
    toggle_class(cell, NOW_PLAYING_CLASS, playing);
    if leading {
        toggle_class(cell, NOW_PLAYING_LEADING_CLASS, playing);
    }
    playing
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RatingRefresh {
    Row,
    Query,
}

pub(super) fn rating_refresh_for_sort(sort_field: &str) -> RatingRefresh {
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
            tracing::warn!("track list column bind: item is not typed queue metadata");
            return;
        };
        let metadata = boxed.borrow::<QueueItemMetadata>();
        let raw = super::queue_item_presentation::track(&metadata).map_or_else(
            || super::queue_item_presentation::cell_text(&metadata, sort_id),
            &render,
        );
        let markup = if super::match_highlight::is_searchable_column(sort_id) {
            super::match_highlight::highlight_from_filter(&raw, &shared_for_bind.filter, || {
                super::match_highlight::accent_foreground(&label)
            })
        } else {
            None
        };
        match markup {
            Some(markup) => label.set_markup(&markup),
            None => label.set_text(&raw),
        }
        apply_now_playing_item(&label, &metadata, &shared_for_bind, false);
        let track_id = super::queue_item_presentation::rating_track_id(&metadata);
        now_playing_marker::register_cell(&shared_for_bind, item, {
            let label = label.clone();
            move |shared| {
                let playing = track_id
                    .is_some_and(|track_id| shared.playing_track_id.get() == Some(track_id));
                toggle_class(&label, NOW_PLAYING_CLASS, playing);
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
/// bumped on every bind before the load starts; `CoverLoader::
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
                tracing::warn!("cover column bind: item is not typed queue metadata");
                return;
            };
            let metadata = boxed.borrow::<QueueItemMetadata>();
            let accessible_label = super::queue_item_presentation::track(&metadata).map_or_else(
                || super::queue_item_presentation::title(&metadata).to_owned(),
                |track| {
                    crate::ui::strings::formatted(
                        crate::ui::strings::GO_TO_ALBUM_NAMED,
                        &[("album", &track.album)],
                    )
                },
            );
            cover.update_property(&[gtk4::accessible::Property::Label(&accessible_label)]);
            apply_now_playing_item(&cover, &metadata, &shared, true);
            let track_id = super::queue_item_presentation::rating_track_id(&metadata);
            now_playing_marker::register_cell(&shared, item, {
                let cover = cover.clone();
                move |shared| {
                    let playing = track_id
                        .is_some_and(|track_id| shared.playing_track_id.get() == Some(track_id));
                    toggle_class(&cover, NOW_PLAYING_CLASS, playing);
                    toggle_class(&cover, NOW_PLAYING_LEADING_CLASS, playing);
                }
            });

            let key = item.as_ptr() as usize;
            let generation = generations
                .borrow_mut()
                .entry(key)
                .or_insert_with(|| Rc::new(Cell::new(0u64)))
                .clone();
            // Bump before starting the load: this is the token the async
            // result must still match when it lands (see the doc comment
            // above) — a stale-in-flight load for whatever track this cell
            // previously showed is dropped by `load_into`'s own check.
            let token = generation.get().wrapping_add(1);
            generation.set(token);
            if let Some(track) = super::queue_item_presentation::track(&metadata) {
                loader.load_into_track_cover(
                    &cover,
                    &track.path,
                    ThumbnailSize::List,
                    token,
                    &generation,
                );
            } else if let Some(icon) = super::queue_item_presentation::source_icon(&metadata) {
                cover.set_icon_name(icon);
            }
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

/// INST-10: the AI badge shows for an AI-manipulated track and nothing else. A
/// pure decision so the rule is testable without realising a ColumnView cell.
pub(super) fn ai_badge_visible(is_ai: bool) -> bool {
    is_ai
}

#[cfg(test)]
#[path = "track_list_columns_tests.rs"]
mod tests;
