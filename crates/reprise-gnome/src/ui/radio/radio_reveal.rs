//! Revealing the connected station in the radio table.
//!
//! `SRC-13`'s "how" for radio. The table is a flat `ColumnView` with uniform
//! row heights, so the shared `scroll_center` math applies unchanged.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use gtk4::prelude::*;
use reprise_core::radio::StationRow;

use super::radio_model::{RadioModel, RadioObject};
use super::radio_presentation::RadioLiveState;
use crate::ui::source_reveal::{self, LoadedItemChange, RevealPolicy};

/// Position of `station_id` in the rows the table is currently showing, or
/// `None` when the active filter hides it.
pub(super) fn station_position(rows: &[StationRow], station_id: i64) -> Option<u32> {
    rows.iter()
        .position(|row| row.id == station_id)
        .and_then(|position| u32::try_from(position).ok())
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

fn reveal_connected_station(
    model: &RadioModel,
    live: &RadioLiveState,
    column_view: &gtk4::ColumnView,
    last_scroll_activity: Option<Instant>,
    change: LoadedItemChange,
) {
    let user_scrolling = source_reveal::is_user_scrolling(last_scroll_activity);
    if source_reveal::reveal_policy(change, user_scrolling) == RevealPolicy::MarkerOnly {
        return;
    }
    let Some(station_id) = live.connected.then_some(live.station_id).flatten() else {
        return;
    };
    let rows = visible_rows(model);
    let Some(position) = station_position(&rows, station_id) else {
        return;
    };
    let Ok(n_rows) = u32::try_from(rows.len()) else {
        return;
    };
    let Some((adjustment, value)) =
        crate::ui::scroll_center::centered_scroll_target(column_view, n_rows, position)
    else {
        return;
    };
    adjustment.set_value(value);
}

pub(super) fn install(
    root: &gtk4::Widget,
    scrolled: &gtk4::ScrolledWindow,
    column_view: &gtk4::ColumnView,
    model: Rc<RadioModel>,
    live: Rc<RefCell<RadioLiveState>>,
) {
    let last_scroll_activity = Rc::new(Cell::new(None::<Instant>));
    let last_activity = last_scroll_activity.clone();
    scrolled.vadjustment().connect_value_changed(move |_| {
        last_activity.set(Some(Instant::now()));
    });

    let column_view = column_view.clone();
    root.connect_map(move |_| {
        let live = live.borrow().clone();
        reveal_connected_station(
            &model,
            &live,
            &column_view,
            last_scroll_activity.get(),
            LoadedItemChange::ViewEntered,
        );
    });
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

    #[test]
    fn src_13_a_station_hidden_by_the_filter_has_nothing_to_reveal() {
        let rows = rows(&[5, 9]);

        assert_eq!(station_position(&rows, 42), None);
        assert_eq!(station_position(&[], 5), None);
    }
}
