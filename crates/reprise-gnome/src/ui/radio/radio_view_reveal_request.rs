//! Explicit player-link reveal entry point for the radio source view.

use super::*;
use crate::ui::radio::radio_reveal::{connected_reveal_plan, ConnectedRevealPlan};

impl RadioView {
    pub(in crate::ui) fn request_reveal_connected(&self) {
        // Clone every input before applying a filter: `apply_filter` invokes
        // the one existing `render_rows` callback synchronously, which borrows
        // the view's collections again.
        let plan = {
            let live = self.shared.live.borrow();
            let rows = self.shared.rows.borrow();
            connected_reveal_plan(&live, &rows, &self.shared.filter_bar.filter())
        };
        match plan {
            ConnectedRevealPlan::NotListed => show_station_not_listed(&self.shared),
            ConnectedRevealPlan::Reveal { relax_filter } => {
                if let Some(filter) = relax_filter {
                    self.shared.filter_bar.apply_filter(filter);
                }
                self.shared.reveal.reveal(LoadedItemChange::RequestedByUser);
            }
        }
    }
}

fn show_station_not_listed(shared: &Shared) {
    if let Some(overlay) = shared.toast_overlay.upgrade() {
        overlay.add_toast(adw::Toast::new(&strings::text(
            strings::STATION_NOT_IN_FAVORITES,
        )));
    }
}
