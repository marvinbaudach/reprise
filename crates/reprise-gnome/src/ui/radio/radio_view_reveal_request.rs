//! Explicit player-link reveal entry point for the radio source view.

use super::*;
use crate::ui::radio::radio_filter_bar::filter_without_hiding;
use crate::ui::radio::radio_reveal::{
    connected_station, station_position, station_reveal_outcome, StationRevealOutcome,
};

impl RadioView {
    pub(in crate::ui) fn request_reveal_connected(&self) {
        let station_id = {
            let live = self.shared.live.borrow();
            connected_station(&live)
        };
        let Some(station_id) = station_id else {
            show_station_not_listed(&self.shared);
            return;
        };
        let rows = self.shared.rows.borrow().clone();
        if station_reveal_outcome(&rows, station_id) == StationRevealOutcome::NotListed {
            show_station_not_listed(&self.shared);
            return;
        }
        let station = rows
            .iter()
            .find(|row| row.id == station_id)
            .expect("station_reveal_outcome accepted a station that is present");
        let filter = self.shared.filter_bar.filter();
        let visible = filter_rows(&rows, &filter);
        if station_position(&visible, station_id).is_none() {
            let adjusted = filter_without_hiding(station, &filter);
            if adjusted != filter {
                // `apply_filter` synchronously invokes the one existing
                // `render_rows` callback; do not replace the model twice.
                self.shared.filter_bar.apply_filter(adjusted);
            }
        }
        self.shared.reveal.reveal(LoadedItemChange::RequestedByUser);
    }
}

fn show_station_not_listed(shared: &Shared) {
    if let Some(overlay) = shared.toast_overlay.upgrade() {
        overlay.add_toast(adw::Toast::new(&strings::text(
            strings::STATION_NOT_IN_FAVORITES,
        )));
    }
}
