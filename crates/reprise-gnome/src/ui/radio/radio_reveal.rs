//! Revealing the connected station in the radio table.
//!
//! `SRC-13`'s "how" for radio. The table is a flat `ColumnView` with uniform
//! row heights, so the shared `scroll_center` math applies unchanged.
//!
//! Radio has no transport of its own, but the connected station still changes
//! from outside this view — an MPRIS command, a restored session, the queue.
//! Those are `ChangedElsewhere`, and leaving them unwired would have made the
//! shared policy a promise this surface does not keep.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use gtk4::prelude::*;
use reprise_core::radio::StationRow;

use super::radio_filter_bar::{filter_rows, filter_without_hiding, RadioFilter};
use super::radio_model::{RadioModel, RadioObject};
use super::radio_presentation::RadioLiveState;
use crate::ui::source_reveal::{self, LoadedItemChange, RevealPolicy};

/// Frames to wait for the table to allocate before giving up. `map` fires
/// before the `ColumnView` has any scroll geometry, so a single synchronous
/// attempt silently reveals nothing — the same trap `podcasts_reveal` and
/// `current_track_selection` already guard against.
const MAX_LAYOUT_FRAMES: u32 = 60;

/// The station the table should be showing as connected, if any. A presented
/// but disconnected station is not a reveal target (`RAD-1`).
pub(super) fn connected_station(live: &RadioLiveState) -> Option<i64> {
    live.connected.then_some(live.station_id).flatten()
}

/// Position of `station_id` in the rows the table is currently showing, or
/// `None` when the active filter hides it.
pub(super) fn station_position(rows: &[StationRow], station_id: i64) -> Option<u32> {
    rows.iter()
        .position(|row| row.id == station_id)
        .and_then(|position| u32::try_from(position).ok())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StationRevealOutcome {
    Reveal,
    NotListed,
}

pub(super) fn station_reveal_outcome(rows: &[StationRow], station_id: i64) -> StationRevealOutcome {
    if station_position(rows, station_id).is_some() {
        StationRevealOutcome::Reveal
    } else {
        StationRevealOutcome::NotListed
    }
}

/// What a player-link jump to the connected station comes down to
/// (`PLAY-12`/`SRC-13`), decided from state alone so the view only has to
/// carry it out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ConnectedRevealPlan {
    /// Nothing is connected, or the station is no longer in the favorites —
    /// say so instead of scrolling to nothing.
    NotListed,
    /// Reveal the station. `relax_filter` carries the filter to apply first
    /// when the active one hides it, and is `None` when it does not.
    Reveal { relax_filter: Option<RadioFilter> },
}

pub(super) fn connected_reveal_plan(
    live: &RadioLiveState,
    rows: &[StationRow],
    filter: &RadioFilter,
) -> ConnectedRevealPlan {
    let Some(station_id) = connected_station(live) else {
        return ConnectedRevealPlan::NotListed;
    };
    let Some(station) = rows.iter().find(|row| row.id == station_id) else {
        return ConnectedRevealPlan::NotListed;
    };
    let visible = filter_rows(rows, filter);
    if station_position(&visible, station_id).is_some() {
        return ConnectedRevealPlan::Reveal { relax_filter: None };
    }
    let relaxed = filter_without_hiding(station, filter);
    ConnectedRevealPlan::Reveal {
        relax_filter: (relaxed != *filter).then_some(relaxed),
    }
}

fn visible_rows(model: &RadioModel) -> Vec<StationRow> {
    (0..model.store().n_items())
        .filter_map(|position| {
            model
                .store()
                .item(position)
                .and_downcast::<RadioObject>()
                .map(|object| object.row())
        })
        .collect()
}

/// Everything the reveal needs, kept alive by the view so a change arriving
/// from outside can reach the same policy the view-entry path uses.
pub(super) struct RadioReveal {
    model: Rc<RadioModel>,
    live: Rc<RefCell<RadioLiveState>>,
    column_view: gtk4::ColumnView,
    last_scroll_activity: Rc<Cell<Option<Instant>>>,
}

impl RadioReveal {
    /// `SRC-13`: centers the connected station unless the policy says the
    /// viewport belongs to the user right now. Never touches focus, selection
    /// or the active filter.
    pub(super) fn reveal(self: &Rc<Self>, change: LoadedItemChange) {
        let user_scrolling = source_reveal::is_user_scrolling(self.last_scroll_activity.get());
        if source_reveal::reveal_policy(change, user_scrolling) == RevealPolicy::MarkerOnly {
            return;
        }
        let Some(station_id) = connected_station(&self.live.borrow()) else {
            return;
        };
        let frames = Cell::new(0_u32);
        let weak = Rc::downgrade(self);
        self.column_view.add_tick_callback(move |column_view, _| {
            let Some(reveal) = weak.upgrade() else {
                return gtk4::glib::ControlFlow::Break;
            };
            if reveal.center_now(station_id, column_view) {
                return gtk4::glib::ControlFlow::Break;
            }
            let seen = frames.replace(frames.get() + 1);
            if seen >= MAX_LAYOUT_FRAMES {
                tracing::debug!(station_id, "radio reveal gave up waiting for layout");
                return gtk4::glib::ControlFlow::Break;
            }
            gtk4::glib::ControlFlow::Continue
        });
    }

    /// `SRC-13`: the one place that decides whether an external snapshot is
    /// worth a reveal. It hangs off the connected station's identity, so a
    /// reconnect, an inline error or any other snapshot for the same station
    /// leaves the viewport alone.
    ///
    /// `change` says where the snapshot came from. A row activated in this
    /// very table was visible by definition, so it resolves to `MarkerOnly` —
    /// double-clicking a station must not scroll the table under the pointer.
    pub(super) fn on_external_change(
        self: &Rc<Self>,
        previously_connected: Option<i64>,
        change: LoadedItemChange,
    ) {
        if connected_station(&self.live.borrow()) != previously_connected {
            self.reveal(change);
        }
    }

    /// Drops the recorded scroll activity, so the next reveal is not held off
    /// by [`source_reveal::USER_SCROLL_GRACE`]. Tests scroll the table
    /// themselves to set up a viewport, which would otherwise make every
    /// reveal they then trigger a no-op for the next 1.5 seconds.
    #[cfg(test)]
    pub(super) fn forget_scroll_activity(&self) {
        self.last_scroll_activity.set(None);
    }

    /// Returns whether the centering could be applied. `false` means the
    /// geometry is not usable yet — or the station is not in the visible rows,
    /// in which case there is nothing to wait for either, but the bounded
    /// retry costs at most `MAX_LAYOUT_FRAMES` no-op frames.
    fn center_now(&self, station_id: i64, column_view: &gtk4::ColumnView) -> bool {
        let rows = visible_rows(&self.model);
        let Some(position) = station_position(&rows, station_id) else {
            return true;
        };
        let Ok(n_rows) = u32::try_from(rows.len()) else {
            return true;
        };
        let Some((adjustment, value)) =
            crate::ui::scroll_center::centered_scroll_target(column_view, n_rows, position)
        else {
            return false;
        };
        adjustment.set_value(value);
        true
    }
}

pub(super) fn install(
    root: &gtk4::Widget,
    scrolled: &gtk4::ScrolledWindow,
    column_view: &gtk4::ColumnView,
    model: Rc<RadioModel>,
    live: Rc<RefCell<RadioLiveState>>,
) -> Rc<RadioReveal> {
    let last_scroll_activity = Rc::new(Cell::new(None::<Instant>));
    let last_activity = last_scroll_activity.clone();
    scrolled.vadjustment().connect_value_changed(move |_| {
        last_activity.set(Some(Instant::now()));
    });

    let reveal = Rc::new(RadioReveal {
        model,
        live,
        column_view: column_view.clone(),
        last_scroll_activity,
    });

    let weak = Rc::downgrade(&reveal);
    root.connect_map(move |_| {
        if let Some(reveal) = weak.upgrade() {
            reveal.reveal(LoadedItemChange::ViewEntered);
        }
    });
    reveal
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(ids: &[i64]) -> Vec<StationRow> {
        ids.iter()
            .map(|id| StationRow {
                id: *id,
                uuid: None,
                name: format!("Station {id}"),
                stream_url: format!("https://example.test/{id}"),
                homepage: None,
                favicon_url: None,
                genre: None,
                codec: None,
                bitrate_kbps: None,
                country_code: None,
                votes: None,
                added_at: 1,
                removed_at: None,
            })
            .collect()
    }

    #[test]
    fn src_13_the_connected_station_is_located_by_its_visible_position() {
        let rows = rows(&[5, 9, 3]);

        assert_eq!(station_position(&rows, 5), Some(0));
        assert_eq!(station_position(&rows, 3), Some(2));
    }

    /// `RAD-1`: a presented but disconnected station is not a reveal target,
    /// so a snapshot that merely names a station must not move the viewport.
    #[test]
    fn src_13_only_a_connected_station_is_a_reveal_target() {
        let connected = RadioLiveState {
            station_id: Some(7),
            connected: true,
            ..RadioLiveState::default()
        };
        let presented = RadioLiveState {
            station_id: Some(7),
            connected: false,
            ..RadioLiveState::default()
        };

        assert_eq!(connected_station(&connected), Some(7));
        assert_eq!(connected_station(&presented), None);
        assert_eq!(connected_station(&RadioLiveState::default()), None);
    }

    #[test]
    fn src_13_a_station_hidden_by_the_filter_has_nothing_to_reveal() {
        let rows = rows(&[5, 9]);

        assert_eq!(station_position(&rows, 42), None);
        assert_eq!(station_position(&[], 5), None);
    }

    #[test]
    fn src_13_a_station_that_is_gone_is_reported_instead_of_ignored() {
        let rows = rows(&[5, 9]);

        assert_eq!(
            station_reveal_outcome(&rows, 9),
            StationRevealOutcome::Reveal
        );
        assert_eq!(
            station_reveal_outcome(&rows, 42),
            StationRevealOutcome::NotListed
        );
    }

    fn connected(station_id: i64) -> RadioLiveState {
        RadioLiveState {
            station_id: Some(station_id),
            connected: true,
            ..RadioLiveState::default()
        }
    }

    /// `PLAY-12`: the whole player-link jump for radio in one place — the
    /// station is revealed, the active filter is relaxed exactly as far as it
    /// has to be, and a station that left the favorites is reported instead of
    /// being scrolled to.
    #[test]
    fn src_13_jumping_to_the_connected_station_relaxes_only_what_hides_it() {
        let mut rows = rows(&[5, 9]);
        rows[0].genre = Some("Jazz".into());
        rows[0].country_code = Some("DE".into());
        rows[1].genre = Some("Rock".into());
        let unfiltered = RadioFilter::default();
        let hiding = RadioFilter {
            genre: Some("Rock".into()),
            country: Some("DE".into()),
        };

        // Visible already: nothing to relax.
        assert_eq!(
            connected_reveal_plan(&connected(5), &rows, &unfiltered),
            ConnectedRevealPlan::Reveal { relax_filter: None }
        );

        // Hidden by the genre chip only — the country chip matches and stays.
        assert_eq!(
            connected_reveal_plan(&connected(5), &rows, &hiding),
            ConnectedRevealPlan::Reveal {
                relax_filter: Some(RadioFilter {
                    genre: None,
                    country: Some("DE".into()),
                }),
            }
        );

        // No longer among the favorites, and nothing connected at all: both
        // are "not listed", not a silent no-op.
        assert_eq!(
            connected_reveal_plan(&connected(42), &rows, &unfiltered),
            ConnectedRevealPlan::NotListed
        );
        assert_eq!(
            connected_reveal_plan(&RadioLiveState::default(), &rows, &unfiltered),
            ConnectedRevealPlan::NotListed
        );
    }
}
