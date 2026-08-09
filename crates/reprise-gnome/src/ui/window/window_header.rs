use gtk4::prelude::*;
use libadwaita as adw;

use super::strings;

pub(super) struct WindowHeader {
    pub window_title: adw::WindowTitle,
    pub search_entry: gtk4::SearchEntry,
    pub sidebar_toggle: gtk4::ToggleButton,
    pub header: adw::HeaderBar,
    pub scan_button: gtk4::Button,
}

pub(super) fn build() -> WindowHeader {
    // Headerbar title follows the currently selected `ViewSource` (Stage 3
    // Task 4); `Library` (`ViewSource::default()`) is both `TrackList`'s and
    // `Sidebar`'s own default initial source, so this is set directly here
    // rather than through a round trip via `Sidebar::set_on_select` (not
    // wired until after `TrackList` exists — see that method's doc comment).
    let window_title = adw::WindowTitle::new(&strings::text(strings::SIDEBAR_MUSIC), "");

    let search_entry = gtk4::SearchEntry::builder()
        .placeholder_text(strings::text(strings::SEARCH_PLACEHOLDER))
        .accessible_role(gtk4::AccessibleRole::SearchBox)
        .build();
    // SEARCH-9: `GtkSearchEntry` throttles `search-changed` by its own
    // `search-delay` (150 ms by default). Reprise debounces the query itself
    // in `view_session::wire_search`, so leaving GTK's delay on stacked two
    // waits and put half the latency out of reach of the code that owns it.
    search_entry.set_search_delay(0);
    search_entry.update_property(&[gtk4::accessible::Property::Label(&strings::text(
        strings::SEARCH_PLACEHOLDER,
    ))]);

    // Starts hidden until `wire_sidebar_toggle` has applied both the persisted
    // Sidebar preference and the current split-view state.
    let sidebar_toggle = gtk4::ToggleButton::builder()
        .icon_name("sidebar-show-symbolic")
        .tooltip_text(strings::text(strings::SIDEBAR_TOGGLE))
        .css_classes(["flat", "reprise-panel-toggle"])
        .visible(false)
        .build();

    let header = adw::HeaderBar::new();
    if let Some(badge) = super::window_build_badge::build() {
        header.pack_start(&badge);
    }
    header.pack_start(&sidebar_toggle);
    header.pack_start(&super::library_chrome::build_navigation_back_button());
    header.set_title_widget(Some(&window_title));
    let scan_button = super::library_chrome::build_maintenance_actions().scan;

    WindowHeader {
        window_title,
        search_entry,
        sidebar_toggle,
        header,
        scan_button,
    }
}
