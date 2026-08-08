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
    if let Some(badge) = build_build_kind_badge() {
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

/// What the badge should read, given what is actually different about this
/// build. `None` means "nothing to say" — a plain release build.
fn build_kind_label(is_debug_build: bool, scroll_diagnostic: bool) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    if is_debug_build {
        parts.push("DEBUG");
    }
    if scroll_diagnostic {
        parts.push("SCROLL-LOG");
    }
    if parts.is_empty() {
        return None;
    }
    Some(parts.join(" \u{b7} "))
}

/// A badge naming this build, for anything that is not the shipped release.
///
/// A session regularly has several binaries in play at once — the installed
/// app, a debug build from a worktree, one with a diagnostic switched on —
/// and telling them apart by the window alone was guesswork. The badge sits
/// in the header bar rather than in the window title because the title is
/// rewritten on every navigation (`Music`, an album name, …) while this
/// survives.
///
/// `None` for a release build with no diagnostics active: the shipped app
/// carries no badge at all.
fn build_build_kind_badge() -> Option<gtk4::Widget> {
    let text = build_kind_label(
        cfg!(debug_assertions),
        std::env::var_os("REPRISE_DEBUG_SCROLL").is_some(),
    )?;
    let label = gtk4::Label::new(Some(&text));
    label.add_css_class("reprise-build-badge");
    label.set_tooltip_text(Some(
        "This is not the installed release build. Diagnostics may be active.",
    ));
    Some(label.upcast())
}

#[cfg(test)]
mod tests {
    use super::build_kind_label;

    /// The shipped app must stay unmarked — a badge on the release build
    /// would be noise in every screenshot and bug report.
    #[test]
    fn a_release_build_without_diagnostics_carries_no_badge() {
        assert_eq!(build_kind_label(false, false), None);
    }

    #[test]
    fn the_badge_names_what_is_actually_different() {
        assert_eq!(build_kind_label(true, false).as_deref(), Some("DEBUG"));
        assert_eq!(
            build_kind_label(false, true).as_deref(),
            Some("SCROLL-LOG"),
            "a release build can still have a diagnostic switched on"
        );
        assert_eq!(
            build_kind_label(true, true).as_deref(),
            Some("DEBUG · SCROLL-LOG")
        );
    }
}
