//! Full-width library chrome from design mockup 7a.

use libadwaita as adw;
use libadwaita::prelude::*;

use super::strings;

const SEARCH_WIDTH: i32 = 300;
pub(in crate::ui) const SEARCH_ACTIVE_CLASS: &str = "reprise-search-active";
const LIBRARY_TITLE_SOURCE: &str = "source";
const LIBRARY_TITLE_SWITCHER: &str = "library-switcher";

pub(in crate::ui) struct LibraryChrome {
    pub(in crate::ui) root: adw::ToolbarView,
}

pub(in crate::ui) struct LibraryMaintenanceActions {
    pub(in crate::ui) scan: gtk4::Button,
}

pub(in crate::ui) struct LibraryTitle {
    pub(in crate::ui) root: gtk4::Stack,
    #[cfg(test)]
    pub(in crate::ui) switcher: gtk4::StackSwitcher,
}

impl LibraryTitle {
    pub(in crate::ui) fn set_library_navigation_visible(&self, visible: bool) {
        let name = if visible {
            LIBRARY_TITLE_SWITCHER
        } else {
            LIBRARY_TITLE_SOURCE
        };
        self.root.set_visible_child_name(name);
    }
}

pub(in crate::ui) fn build(
    header: &adw::HeaderBar,
    content: &impl IsA<gtk4::Widget>,
) -> LibraryChrome {
    let root = adw::ToolbarView::new();
    root.set_top_bar_style(adw::ToolbarStyle::Flat);
    root.add_top_bar(header);
    root.set_content(Some(content));
    LibraryChrome { root }
}

pub(in crate::ui) fn search_accent_active(text: &str) -> bool {
    !text.trim().is_empty()
}

pub(in crate::ui) fn style_header(header: &adw::HeaderBar, search: &gtk4::SearchEntry) {
    // Loose (not Strict) centering: at comfortable widths the view switcher is
    // still centered, but Strict reserves 2×max(start, end) around the centre —
    // the 300px search entry alone forces a ~1404px header minimum, which cuts
    // off the header's right controls (and squeezes the content past the info
    // panel) on a maximised HiDPI screen. Loose keeps everything visible and
    // only lets the switcher drift off-centre when space is genuinely tight.
    header.set_centering_policy(adw::CenteringPolicy::Loose);
    search.set_width_request(SEARCH_WIDTH);
    search.set_hexpand(false);
    search.connect_search_changed(|entry| {
        if search_accent_active(&entry.text()) {
            entry.add_css_class(SEARCH_ACTIVE_CLASS);
        } else {
            entry.remove_css_class(SEARCH_ACTIVE_CLASS);
        }
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

pub(in crate::ui) fn build_library_title(
    header: &adw::HeaderBar,
    source_title: &adw::WindowTitle,
    views: &gtk4::Stack,
) -> LibraryTitle {
    header.set_title_widget(gtk4::Widget::NONE);
    let switcher = gtk4::StackSwitcher::builder().stack(views).build();
    switcher.add_css_class("reprise-view-switcher");
    let root = gtk4::Stack::new();
    root.add_named(source_title, Some(LIBRARY_TITLE_SOURCE));
    root.add_named(&switcher, Some(LIBRARY_TITLE_SWITCHER));
    root.set_visible_child_name(LIBRARY_TITLE_SWITCHER);
    header.set_title_widget(Some(&root));
    LibraryTitle {
        root,
        #[cfg(test)]
        switcher,
    }
}

/// The Tracks/Albums/Artists `GtkStackSwitcher` styled as a rounded pill
/// group (design mockup 14a): a subtle white-tint container with a soft
/// radius, and segment buttons that shed the default `.linked` hard edges —
/// the active segment tinted + bold, inactive quiet, hover a hair brighter.
/// `@window_fg_color` (near-white on the dark theme) keeps it theme-aware.
/// Installed app-wide by [`super::style`].
pub(in crate::ui) fn css() -> String {
    ".reprise-view-switcher { \
       background-color: alpha(@window_fg_color, 0.06); \
       border: none; border-radius: 8px; padding: 2px; box-shadow: none; }\n\
     .reprise-view-switcher > button { \
       border: none; border-radius: 6px; box-shadow: none; outline: none; \
       min-height: 0; margin: 0; padding: 2px 14px; \
       background-color: transparent; background-image: none; \
       color: alpha(@window_fg_color, 0.60); font-weight: 400; }\n\
     .reprise-view-switcher > button:hover:not(:checked) { \
       background-color: alpha(@window_fg_color, 0.08); }\n\
     .reprise-view-switcher > button:checked { \
       background-color: alpha(@window_fg_color, 0.14); \
       color: @window_fg_color; font-weight: 700; }"
        .to_string()
}

#[cfg(test)]
mod tests {
    use libadwaita::prelude::*;

    use super::*;

    // UX FIL-4: the field is marked as soon as it carries real text — also
    // unfocused; whitespace-only never claims state (mirrors is_restricted).
    #[test]
    fn fil_4_search_accent_tracks_trimmed_text() {
        assert!(search_accent_active("falling"));
        assert!(!search_accent_active(""));
        assert!(!search_accent_active("   "));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn header_spans_the_navigation_with_loose_centering() {
        if gtk4::init().is_err() {
            return;
        }
        let header = adw::HeaderBar::new();
        let title = adw::WindowTitle::new("Music", "");
        let search = gtk4::SearchEntry::new();
        header.set_title_widget(Some(&title));
        style_header(&header, &search);
        let navigation = test_navigation();
        let chrome = build(&header, &navigation);

        // Loose, not Strict: Strict reserves 2×max(start,end) and forces a
        // ~1404px header minimum that cuts the right controls on a maximised
        // HiDPI screen (QA #3/#4).
        assert_eq!(header.centering_policy(), adw::CenteringPolicy::Loose);
        assert_eq!(search.width_request(), 300);
        assert_eq!(chrome.root.top_bar_style(), adw::ToolbarStyle::Flat);
        assert_eq!(
            chrome.root.content().as_ref(),
            Some(navigation.upcast_ref())
        );
        assert!(header.is_ancestor(&chrome.root));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn header_actions_are_compact_icons_with_accessible_tooltips() {
        if gtk4::init().is_err() {
            return;
        }
        let button = action_button("folder-open-symbolic", "Scan folder…");

        assert_eq!(button.icon_name().as_deref(), Some("folder-open-symbolic"));
        assert_eq!(button.tooltip_text().as_deref(), Some("Scan folder…"));
        assert!(button.label().is_none());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn tip_1a_library_chrome_buttons_follow_tooltip_discipline() {
        if gtk4::init().is_err() {
            return;
        }
        let actions = build_maintenance_actions();

        let violations =
            crate::ui::tooltip_discipline::tooltip_violations(actions.scan.upcast_ref());
        assert!(violations.is_empty(), "{violations:?}");
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn maintenance_actions_keep_the_scan_trigger_out_of_the_header() {
        if gtk4::init().is_err() {
            return;
        }
        let header = adw::HeaderBar::new();

        let actions = build_maintenance_actions();

        assert!(!actions.scan.is_ancestor(&header));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn library_view_title_switches_between_source_title_and_view_switcher() {
        if gtk4::init().is_err() {
            return;
        }
        let title = adw::WindowTitle::new("Music", "");
        let views = gtk4::Stack::new();
        views.add_titled(&gtk4::Label::new(Some("Tracks")), Some("tracks"), "Tracks");

        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&title));
        let library_title = build_library_title(&header, &title, &views);

        assert_eq!(library_title.switcher.stack(), Some(views.clone()));
        assert_eq!(title.parent(), Some(library_title.root.clone().upcast()));
        assert!(library_title.root.is_ancestor(&header));
        assert_eq!(
            library_title.root.visible_child_name().as_deref(),
            Some(LIBRARY_TITLE_SWITCHER)
        );
        library_title.set_library_navigation_visible(false);
        assert_eq!(
            library_title.root.visible_child_name().as_deref(),
            Some(LIBRARY_TITLE_SOURCE)
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn header_stays_above_a_player_bar_shell() {
        if gtk4::init().is_err() {
            return;
        }
        let header = adw::HeaderBar::new();
        let navigation = test_navigation();
        let player = gtk4::ActionBar::new();
        let shell = super::super::library_player_bar::LibraryPlayerBarShell::new(
            &navigation,
            Some(player.upcast_ref()),
            reprise_core::library::settings::PlayerBarPosition::Top,
        );

        let chrome = build(&header, shell.widget());

        assert!(header.is_ancestor(&chrome.root));
        assert_eq!(
            chrome.root.content().as_ref(),
            Some(shell.widget().upcast_ref())
        );
        // Bar and navigation are structural siblings: with the Top position
        // the bar precedes the navigation instead of floating above it.
        assert_eq!(
            shell.widget().last_child().as_ref(),
            Some(navigation.upcast_ref::<gtk4::Widget>())
        );
        assert!(player.is_ancestor(shell.widget()));
    }

    fn test_navigation() -> adw::NavigationSplitView {
        let sidebar = adw::NavigationPage::builder()
            .title("Sidebar")
            .child(&gtk4::Label::new(Some("Sidebar")))
            .build();
        let content = adw::NavigationPage::builder()
            .title("Library")
            .child(&gtk4::Label::new(Some("Library")))
            .build();
        adw::NavigationSplitView::builder()
            .sidebar(&sidebar)
            .content(&content)
            .build()
    }
}
