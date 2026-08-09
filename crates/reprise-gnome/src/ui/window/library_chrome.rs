//! Full-width library chrome from design mockup 7a.

use libadwaita as adw;
use libadwaita::prelude::*;

pub(in crate::ui) use super::library_chrome_css::css;
use super::search_popover::SearchPopover;
use super::strings;

pub(in crate::ui) struct LibraryChrome {
    pub(in crate::ui) root: adw::ToolbarView,
    pub(in crate::ui) search: SearchPopover,
    /// SEARCH-8a needs the lens outside tests too: a section without a list
    /// makes it insensitive and re-labels it.
    pub(in crate::ui) search_toggle: gtk4::ToggleButton,
}

pub(in crate::ui) struct LibraryMaintenanceActions {
    pub(in crate::ui) scan: gtk4::Button,
}

pub(in crate::ui) fn build(
    header: &adw::HeaderBar,
    content: &impl IsA<gtk4::Widget>,
    search_entry: &gtk4::SearchEntry,
    _key_capture_widget: &impl IsA<gtk4::Widget>,
) -> LibraryChrome {
    header.add_css_class("reprise-library-header");
    let search_toggle = gtk4::ToggleButton::builder()
        .icon_name("system-search-symbolic")
        .tooltip_text(strings::shortcut_tooltip(
            strings::SEARCH_PLACEHOLDER,
            strings::SHORTCUT_SEARCH,
        ))
        .css_classes(["flat", "reprise-panel-toggle"])
        .build();
    search_toggle.update_property(&[gtk4::accessible::Property::Label(&strings::text(
        strings::SEARCH_PLACEHOLDER,
    ))]);
    header.pack_end(&search_toggle);

    let search = SearchPopover::new(&search_toggle, search_entry);
    wire_search_toggle(&search_toggle, &search, search_entry);

    let root = adw::ToolbarView::new();
    root.set_top_bar_style(adw::ToolbarStyle::Flat);
    root.add_top_bar(header);
    root.set_content(Some(content));
    LibraryChrome {
        root,
        search,
        search_toggle,
    }
}

pub(in crate::ui) fn wire_content_stack(root: &adw::ToolbarView, stack: &gtk4::Stack) {
    sync_content_chrome(root, stack.visible_child_name().as_deref());
    let root = root.clone();
    stack.connect_visible_child_name_notify(move |stack| {
        sync_content_chrome(&root, stack.visible_child_name().as_deref());
    });
}

fn sync_content_chrome(root: &adw::ToolbarView, visible_child: Option<&str>) {
    root.set_reveal_top_bars(visible_child != Some("library-doctor"));
}

pub(in crate::ui) fn build_navigation_back_button() -> gtk4::Button {
    let label = strings::text(strings::NAVIGATE_BACK);
    let button = gtk4::Button::builder()
        .icon_name("go-previous-symbolic")
        .tooltip_text(&label)
        .action_name("win.nav-back")
        .css_classes(["flat", "reprise-panel-toggle"])
        .build();
    button.update_property(&[gtk4::accessible::Property::Label(&label)]);
    button
}

pub(in crate::ui) fn search_toggle_active(search_mode: bool, query: &str) -> bool {
    search_mode || !query.trim().is_empty()
}

fn wire_search_toggle(
    toggle: &gtk4::ToggleButton,
    search: &SearchPopover,
    search_entry: &gtk4::SearchEntry,
) {
    let search_for_click = search.clone();
    toggle.connect_clicked(move |toggle| {
        if crate::ui::shortcuts::next_search_mode(search_for_click.is_open()) {
            search_for_click.open();
        } else {
            search_for_click.close();
        }
        toggle.set_active(search_toggle_active(
            search_for_click.is_open(),
            &search_for_click.entry().text(),
        ));
    });

    let toggle_weak = toggle.downgrade();
    let entry = search_entry.downgrade();
    search.connect_open_changed(move |open| {
        let (Some(toggle), Some(entry)) = (toggle_weak.upgrade(), entry.upgrade()) else {
            return;
        };
        toggle.set_active(search_toggle_active(open, &entry.text()));
    });

    let toggle_weak = toggle.downgrade();
    // Weak, not a clone. This closure is stored in the entry's own handler
    // list, and a strong `SearchPopover` holds that same entry — a strong
    // capture here closes the loop onto itself, so `finalize` never runs and
    // the entry, the popover and its caption outlive the window. Measured, not
    // assumed. Same rule as the one spelled out on `SectionSearch`.
    let search_for_change = search.downgrade();
    // `connect_changed`, not `connect_search_changed`: the lens only reflects
    // "a query exists" (SEARCH-3) and must follow every keystroke. Since
    // SEARCH-9 the entry's own `search-delay` is 0 and the two signals fire
    // together, but the app's debounce still sits behind `search_changed` in
    // `view_session`, and the lens must not wait for it.
    search_entry.connect_changed(move |entry| {
        let Some(toggle) = toggle_weak.upgrade() else {
            return;
        };
        let query = entry.text();
        toggle.set_active(search_toggle_active(search_for_change.is_open(), &query));
    });
}

pub(in crate::ui) fn action_button(icon_name: &str, label: &str) -> gtk4::Button {
    let button = gtk4::Button::builder()
        .icon_name(icon_name)
        .tooltip_text(label)
        .build();
    button.update_property(&[gtk4::accessible::Property::Label(label)]);
    button
}

pub(in crate::ui) fn build_maintenance_actions() -> LibraryMaintenanceActions {
    let scan = action_button("folder-open-symbolic", &strings::text(strings::SCAN_FOLDER));
    LibraryMaintenanceActions { scan }
}

#[cfg(test)]
#[path = "library_chrome_tests.rs"]
mod tests;

#[cfg(test)]
mod style_guard {
    /// UX STYLE-1: every chrome surface that should read as its own plane
    /// declares its background explicitly.
    #[test]
    fn style_1_chrome_surfaces_declare_background_and_edge() {
        let css = super::css();

        for class in [".reprise-library-header", ".reprise-search-popover"] {
            let block = css
                .split(class)
                .nth(1)
                .unwrap_or_else(|| panic!("{class} has no rule in the chrome CSS"));
            let block = block.split('}').next().unwrap_or_default();
            assert!(
                block.contains("background-color:"),
                "{class} inherits its background"
            );
            // The header divides itself from content with a bottom hairline;
            // the floating popover has a full border instead.
            if class == ".reprise-library-header" {
                assert!(block.contains("border-bottom:"));
            } else {
                assert!(block.contains("border:"));
            }
        }
    }
}
