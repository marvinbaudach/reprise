//! Sort state and the SQL-backed sort logic for `ui::track_list`: the
//! `SortState` (field + direction) the model queries with, the header-click
//! observer (`wire_sort_clicks`/`on_sorter_changed`) that maps a
//! `ColumnViewSorter` change back to a whitelisted `queries` field, and the
//! pure source-switch sort matrix (`default_sort_for_source`/
//! `resolve_sort_on_switch`). Split out of `track_list.rs`; the pure matrix
//! functions carry their own unit tests here.

use std::rc::Rc;

use gtk4::prelude::*;

use crate::ui::column_layout::ColumnId;
use crate::ui::track_list::{reload, Shared};
use reprise_core::view_source::ViewSource;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::ui) struct SortState {
    pub(in crate::ui) field: String,
    pub(in crate::ui) dir: String,
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

pub(in crate::ui) fn restored_sort(field: &str, dir: &str) -> SortState {
    if ColumnId::from_sort_field(field).is_none() {
        return SortState {
            field: "title".into(),
            dir: "asc".into(),
        };
    }
    SortState {
        field: field.into(),
        dir: if dir == "desc" { "desc" } else { "asc" }.into(),
    }
}

/// Observes the `ColumnView`'s aggregate sorter for header clicks and maps
/// them back to a whitelisted sort field + direction, then reloads.
pub(in crate::ui) fn wire_sort_clicks(column_view: &gtk4::ColumnView, shared: &Rc<Shared>) {
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
    if shared.restoring_view.get() {
        return;
    }
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

/// Mirrors `queries.rs`'s `"playlist_order"` `SORT_WHITELIST` sentinel (see
/// that module's `Playlist(id)` doc section) — the one sort field this
/// module ever sets on a source switch rather than a column-header click.
pub(in crate::ui) const PLAYLIST_ORDER_SORT_FIELD: &str = "playlist_order";

/// Pure decision of what `shared.sort` should become when the track list
/// switches to `source`, *before* the switch's reload runs — factored out of
/// `set_source_and_reload` so it can be unit tested without building a live
/// `TrackList` (constructing one requires an initialized GTK display, unlike
/// this plain function).
///
/// `Some(sort)` names the exact default a newly-selected source forces
/// regardless of whatever sort was previously active: today only `Playlist`
/// does this, defaulting to `pt.position` order via the `"playlist_order"`
/// sentinel (see `queries.rs`'s module doc) rather than the general
/// `SortState::default()` (artist/asc) every other source starts from.
///
/// `None` means the new source has no forced default of its own — the
/// caller (`set_source_and_reload`) then only needs to make sure a
/// *previous* source's forced default doesn't linger: the `"playlist_order"`
/// sentinel only resolves to valid SQL inside a query that joins
/// `playlist_tracks` (i.e. only for `ViewSource::Playlist`), so leaving a
/// playlist for any other source must reset it back to `SortState::
/// default()` rather than silently keep sorting by a column expression the
/// new source's query doesn't select.
fn default_sort_for_source(source: &ViewSource) -> Option<SortState> {
    match source {
        ViewSource::Playlist(_) => Some(SortState {
            field: PLAYLIST_ORDER_SORT_FIELD.to_string(),
            dir: "asc".to_string(),
        }),
        ViewSource::Library
        | ViewSource::Smart(_)
        | ViewSource::Queue
        | ViewSource::Missing
        | ViewSource::Album { .. }
        | ViewSource::Artist(_)
        | ViewSource::Device { .. } => None,
        ViewSource::ImportErrors
        | ViewSource::MyStats
        | ViewSource::Releases
        | ViewSource::Concerts
        | ViewSource::Conversions => None,
    }
}

/// Pure decision of what the active sort should become when the track list
/// switches from a state currently sorted by `current` to `target` —
/// factored out of `set_source_and_reload` (like `default_sort_for_source`,
/// which it builds on) so the *whole* switch matrix is unit-testable,
/// including the previously-untested leaving-a-playlist reset arm:
///
/// - `target` forces a default of its own (today: `Playlist` →
///   `"playlist_order"`) → that default wins, whatever was active.
/// - `target` forces nothing, but `current` is the `"playlist_order"`
///   sentinel → reset to `SortState::default()`: the sentinel only
///   resolves to valid SQL inside a query that joins `playlist_tracks`,
///   so it must never leak into any other source's query.
/// - otherwise → `current` is kept as-is; a column-header click's sort
///   deliberately survives source switches (matching pre-Stage-3 behavior
///   for Library/Missing/… hops).
pub(in crate::ui) fn resolve_sort_on_switch(current: &SortState, target: &ViewSource) -> SortState {
    match default_sort_for_source(target) {
        Some(sort) => sort,
        None if current.field == PLAYLIST_ORDER_SORT_FIELD => SortState::default(),
        None => current.clone(),
    }
}

#[cfg(test)]
mod default_sort_for_source_tests {
    use super::*;

    /// CRITICAL fix (review round 1): a `Playlist` source must always
    /// resolve to the `"playlist_order"` sentinel/asc, regardless of the id
    /// it carries — this is what `set_source_and_reload` now applies to
    /// `shared.sort` *before* reloading, which is the missing wiring that
    /// let switching to a playlist silently reload with whatever sort
    /// (usually artist/asc) was already active instead of `pt.position`.
    #[test]
    fn playlist_always_defaults_to_playlist_order_ascending() {
        for id in [1, 2, 42] {
            assert_eq!(
                default_sort_for_source(&ViewSource::Playlist(id)),
                Some(SortState {
                    field: "playlist_order".to_string(),
                    dir: "asc".to_string(),
                })
            );
        }
    }

    /// Every non-Playlist source has no forced default of its own — the
    /// caller (`set_source_and_reload`, via `resolve_sort_on_switch`) is
    /// responsible for resetting away from a lingering playlist sentinel in
    /// this case, not this function.
    #[test]
    fn non_playlist_sources_have_no_forced_default() {
        assert_eq!(default_sort_for_source(&ViewSource::Library), None);
        assert_eq!(default_sort_for_source(&ViewSource::Smart(1)), None);
        assert_eq!(default_sort_for_source(&ViewSource::Queue), None);
        assert_eq!(default_sort_for_source(&ViewSource::Missing), None);
        assert_eq!(default_sort_for_source(&ViewSource::ImportErrors), None);
        assert_eq!(
            default_sort_for_source(&ViewSource::Album {
                album: "Blue".into(),
                album_artist: "Joni Mitchell".into(),
            }),
            None
        );
        assert_eq!(
            default_sort_for_source(&ViewSource::Artist("Björk".into())),
            None
        );
    }
}

/// The full source-switch sort matrix for `resolve_sort_on_switch` — the
/// exact logic `set_source_and_reload` applies to `shared.sort` before
/// every reload, including the previously-untested leaving-a-playlist
/// reset arm (Stage 3 re-review follow-up).
#[cfg(test)]
mod resolve_sort_on_switch_tests {
    use super::*;

    fn playlist_order_sort() -> SortState {
        SortState {
            field: PLAYLIST_ORDER_SORT_FIELD.to_string(),
            dir: "asc".to_string(),
        }
    }

    fn header_click_sort() -> SortState {
        SortState {
            field: "title".to_string(),
            dir: "desc".to_string(),
        }
    }

    #[test]
    fn library_to_playlist_forces_playlist_order() {
        assert_eq!(
            resolve_sort_on_switch(&SortState::default(), &ViewSource::Playlist(1)),
            playlist_order_sort()
        );
    }

    #[test]
    fn playlist_to_playlist_keeps_forcing_playlist_order() {
        // Switching between two playlists: the sentinel is re-applied (and
        // any header-click override from the first playlist is dropped —
        // the second playlist starts in its own default order).
        assert_eq!(
            resolve_sort_on_switch(&playlist_order_sort(), &ViewSource::Playlist(2)),
            playlist_order_sort()
        );
        assert_eq!(
            resolve_sort_on_switch(&header_click_sort(), &ViewSource::Playlist(2)),
            playlist_order_sort()
        );
    }

    /// The reset arm this module exists to pin down: leaving a playlist
    /// while the `"playlist_order"` sentinel is active must fall back to
    /// the general default — the sentinel only resolves to valid SQL in a
    /// query that joins `playlist_tracks`.
    #[test]
    fn playlist_to_library_resets_sentinel_to_default() {
        assert_eq!(
            resolve_sort_on_switch(&playlist_order_sort(), &ViewSource::Library),
            SortState::default()
        );
    }

    #[test]
    fn playlist_sentinel_resets_for_every_non_playlist_target() {
        for target in [
            ViewSource::Library,
            ViewSource::Smart(1),
            ViewSource::Queue,
            ViewSource::Missing,
            ViewSource::ImportErrors,
            ViewSource::Album {
                album: "Blue".into(),
                album_artist: "Joni Mitchell".into(),
            },
            ViewSource::Artist("Björk".into()),
        ] {
            assert_eq!(
                resolve_sort_on_switch(&playlist_order_sort(), &target),
                SortState::default(),
                "sentinel must not leak into {target:?}"
            );
        }
    }

    #[test]
    fn library_to_missing_keeps_current_sort() {
        assert_eq!(
            resolve_sort_on_switch(&SortState::default(), &ViewSource::Missing),
            SortState::default()
        );
    }

    /// A column-header click inside a playlist overrides the sentinel; on
    /// leaving the playlist that (non-sentinel) sort survives — only the
    /// sentinel itself is reset, matching pre-Stage-3 behavior where a
    /// header-click sort persisted across Library/Missing/… hops.
    #[test]
    fn header_click_override_survives_leaving_a_playlist() {
        assert_eq!(
            resolve_sort_on_switch(&header_click_sort(), &ViewSource::Library),
            header_click_sort()
        );
        assert_eq!(
            resolve_sort_on_switch(&header_click_sort(), &ViewSource::Queue),
            header_click_sort()
        );
    }
}

#[cfg(test)]
mod restored_sort_tests {
    use super::*;

    #[test]
    fn unknown_restored_sort_falls_back_to_title_ascending() {
        assert_eq!(
            restored_sort("drop table", "sideways"),
            SortState {
                field: "title".into(),
                dir: "asc".into(),
            }
        );
    }
}
