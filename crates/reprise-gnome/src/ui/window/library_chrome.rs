//! Full-width library chrome from design mockup 7a.

use libadwaita as adw;
use libadwaita::prelude::*;

use super::strings;

const SEARCH_WIDTH: i32 = 300;
const LIBRARY_TITLE_SOURCE: &str = "source";
const LIBRARY_TITLE_SWITCHER: &str = "library-switcher";

pub(super) struct LibraryChrome {
    pub(super) root: adw::ToolbarView,
}

pub(super) struct LibraryMaintenanceActions {
    pub(super) scan: gtk4::Button,
}

pub(super) struct LibraryTitle {
    pub(super) root: gtk4::Stack,
    #[cfg(test)]
    pub(super) switcher: adw::ViewSwitcher,
}

impl LibraryTitle {
    pub(super) fn set_library_navigation_visible(&self, visible: bool) {
        let name = if visible {
            LIBRARY_TITLE_SWITCHER
        } else {
            LIBRARY_TITLE_SOURCE
        };
        self.root.set_visible_child_name(name);
    }
}

pub(super) fn build(header: &adw::HeaderBar, content: &impl IsA<gtk4::Widget>) -> LibraryChrome {
    let root = adw::ToolbarView::new();
    root.set_top_bar_style(adw::ToolbarStyle::Flat);
    root.add_top_bar(header);
    root.set_content(Some(content));
    LibraryChrome { root }
}

pub(super) fn style_header(header: &adw::HeaderBar, search: &gtk4::SearchEntry) {
    header.set_centering_policy(adw::CenteringPolicy::Strict);
    search.set_width_request(SEARCH_WIDTH);
    search.set_hexpand(false);
}

pub(super) fn action_button(icon_name: &str, label: &str) -> gtk4::Button {
    let button = gtk4::Button::builder()
        .icon_name(icon_name)
        .tooltip_text(label)
        .build();
    button.update_property(&[gtk4::accessible::Property::Label(label)]);
    button
}

pub(super) fn build_maintenance_actions() -> LibraryMaintenanceActions {
    let scan = action_button("folder-open-symbolic", &strings::text(strings::SCAN_FOLDER));
    LibraryMaintenanceActions { scan }
}

pub(super) fn build_library_title(
    source_title: &adw::WindowTitle,
    views: &adw::ViewStack,
) -> LibraryTitle {
    let switcher = adw::ViewSwitcher::builder()
        .policy(adw::ViewSwitcherPolicy::Wide)
        .stack(views)
        .build();
    switcher.add_css_class("reprise-surface");
    let root = gtk4::Stack::new();
    root.add_named(source_title, Some(LIBRARY_TITLE_SOURCE));
    root.add_named(&switcher, Some(LIBRARY_TITLE_SWITCHER));
    root.set_visible_child_name(LIBRARY_TITLE_SWITCHER);
    LibraryTitle {
        root,
        #[cfg(test)]
        switcher,
    }
}

#[cfg(test)]
mod tests {
    use libadwaita::prelude::*;

    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn header_spans_the_navigation_with_strict_centering() {
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

        assert_eq!(header.centering_policy(), adw::CenteringPolicy::Strict);
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
        let views = adw::ViewStack::new();
        views.add_titled(&gtk4::Label::new(Some("Tracks")), Some("tracks"), "Tracks");

        let library_title = build_library_title(&title, &views);

        assert_eq!(library_title.switcher.stack(), Some(views.clone()));
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
    fn header_stays_above_a_top_player_bar() {
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
        let top_bar = shell
            .widget()
            .first_child()
            .unwrap()
            .downcast::<gtk4::Box>()
            .unwrap();
        assert_eq!(top_bar.first_child(), Some(player.upcast()));
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
