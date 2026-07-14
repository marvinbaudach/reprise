//! Full-width library chrome from design mockup 7a.

use libadwaita as adw;
use libadwaita::prelude::*;

const SEARCH_WIDTH: i32 = 300;

pub(super) struct LibraryChrome {
    pub(super) root: adw::ToolbarView,
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
