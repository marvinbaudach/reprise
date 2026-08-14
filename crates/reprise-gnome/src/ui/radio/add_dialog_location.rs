use gtk4::prelude::*;

use super::radio_chips::NearYouAction;
use crate::ui::source_empty_state::{SourceEmptyState, SourceEmptyStateCopy};
use crate::ui::strings;

const RESULTS_PAGE: &str = "results";
const MISSING_LOCATION_PAGE: &str = "missing-location";
const MISSING_COUNTRY_PAGE: &str = "missing-country";

pub(super) struct LocationResults {
    stack: gtk4::Stack,
    missing_location: SourceEmptyState,
    missing_country: SourceEmptyState,
}

impl LocationResults {
    pub(super) fn new(results: &gtk4::ListBox) -> Self {
        let scroller = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .propagate_natural_height(true)
            .vexpand(true)
            .child(results)
            .build();
        results.set_margin_end(6);
        let missing_location = SourceEmptyState::new(&SourceEmptyStateCopy {
            icon_name: "find-location-symbolic",
            title: strings::text(strings::RADIO_NEAR_YOU_NO_LOCATION_TITLE),
            body: strings::text(strings::RADIO_NEAR_YOU_NO_LOCATION_DESCRIPTION),
            button_label: strings::text(strings::RADIO_OPEN_LOCATION_PREFERENCES),
            button_icon_name: "go-next-symbolic",
            secondary_line: None,
        });
        let missing_country = SourceEmptyState::new(&SourceEmptyStateCopy {
            icon_name: "find-location-symbolic",
            title: strings::text(strings::RADIO_NEAR_YOU_NO_COUNTRY_TITLE),
            body: strings::text(strings::RADIO_NEAR_YOU_NO_COUNTRY_DESCRIPTION),
            button_label: strings::text(strings::RADIO_OPEN_LOCATION_PREFERENCES),
            button_icon_name: "go-next-symbolic",
            secondary_line: None,
        });
        let stack = gtk4::Stack::builder()
            .transition_type(gtk4::StackTransitionType::Crossfade)
            .vexpand(true)
            .build();
        stack.add_named(&scroller, Some(RESULTS_PAGE));
        stack.add_named(missing_location.widget(), Some(MISSING_LOCATION_PAGE));
        stack.add_named(missing_country.widget(), Some(MISSING_COUNTRY_PAGE));
        stack.set_visible_child_name(RESULTS_PAGE);
        Self {
            stack,
            missing_location,
            missing_country,
        }
    }

    pub(super) fn widget(&self) -> &gtk4::Widget {
        self.stack.upcast_ref()
    }

    pub(super) fn connect_open_preferences(&self, callback: impl Fn() + Clone + 'static) {
        self.missing_location.connect_add(callback.clone());
        self.missing_country.connect_add(callback);
    }

    pub(super) fn show_results(&self) {
        self.stack.set_visible_child_name(RESULTS_PAGE);
    }

    pub(super) fn show_empty(&self, action: &NearYouAction) {
        let page = match action {
            NearYouAction::MissingLocation => MISSING_LOCATION_PAGE,
            NearYouAction::MissingCountry => MISSING_COUNTRY_PAGE,
            NearYouAction::Search(_) => return,
        };
        self.stack.set_visible_child_name(page);
    }

    #[cfg(test)]
    pub(super) fn visible_page_name(&self) -> Option<String> {
        self.stack.visible_child_name().map(|name| name.to_string())
    }

    #[cfg(test)]
    pub(super) fn open_preferences_button(&self, action: &NearYouAction) -> &gtk4::Button {
        match action {
            NearYouAction::MissingLocation => self.missing_location.button(),
            NearYouAction::MissingCountry => self.missing_country.button(),
            NearYouAction::Search(_) => panic!("a usable location has no empty-state button"),
        }
    }
}
