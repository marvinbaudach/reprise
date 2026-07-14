use gtk4::prelude::*;
use libadwaita as adw;

use super::device_sync_strings;
use super::strings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PageId {
    Playback,
    Appearance,
    Layout,
    Library,
    Synchronization,
    Plugins,
}

pub(super) const PAGE_ORDER: [PageId; 6] = [
    PageId::Playback,
    PageId::Appearance,
    PageId::Layout,
    PageId::Library,
    PageId::Synchronization,
    PageId::Plugins,
];

impl PageId {
    fn name(self) -> &'static str {
        match self {
            Self::Playback => "playback",
            Self::Appearance => "appearance",
            Self::Layout => "layout",
            Self::Library => "library",
            Self::Synchronization => "synchronization",
            Self::Plugins => "plugins",
        }
    }

    pub(super) fn title(self) -> String {
        let message = match self {
            Self::Playback => strings::PREFERENCES_PLAYBACK,
            Self::Appearance => strings::PREFERENCES_APPEARANCE,
            Self::Layout => strings::PREFERENCES_LAYOUT,
            Self::Library => strings::PREFERENCES_LIBRARY,
            Self::Synchronization => device_sync_strings::SYNCHRONIZATION,
            Self::Plugins => strings::PREFERENCES_PLUGINS,
        };
        strings::text(message)
    }

    pub(super) fn icon_name(self) -> &'static str {
        match self {
            Self::Playback => "audio-speakers-symbolic",
            Self::Appearance => "applications-graphics-symbolic",
            Self::Layout => "view-grid-symbolic",
            Self::Library => "folder-music-symbolic",
            Self::Synchronization => "phone-symbolic",
            Self::Plugins => "application-x-addon-symbolic",
        }
    }
}

pub(super) struct PreferencesShell {
    pub(super) window: adw::Window,
    pub(super) navigation: adw::NavigationView,
    pub(super) stack: adw::ViewStack,
    #[cfg(test)]
    pub(super) switcher: adw::ViewSwitcher,
    #[cfg(test)]
    pub(super) header: adw::HeaderBar,
}

pub(super) fn build(
    parent: &adw::ApplicationWindow,
    pages: [(PageId, adw::PreferencesPage); 6],
) -> PreferencesShell {
    let stack = adw::ViewStack::new();
    for (id, page) in pages {
        stack.add_titled_with_icon(&page, Some(id.name()), &id.title(), id.icon_name());
    }
    // Appearance was the established default page. Keeping it visible also
    // lets Playback's ComboRows finish parenting before users open that tab.
    stack.set_visible_child_name("appearance");
    let switcher = adw::ViewSwitcher::builder()
        .policy(adw::ViewSwitcherPolicy::Wide)
        .stack(&stack)
        .build();
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&switcher));
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&stack));
    let root_page = adw::NavigationPage::with_tag(
        &toolbar,
        &strings::text(strings::PREFERENCES),
        "preferences",
    );
    let navigation = adw::NavigationView::new();
    navigation.add(&root_page);
    let window = adw::Window::builder()
        .application(
            &parent
                .application()
                .expect("main window has an application"),
        )
        .title(strings::text(strings::PREFERENCES))
        .transient_for(parent)
        .modal(false)
        .destroy_with_parent(true)
        .default_width(760)
        .default_height(680)
        .content(&navigation)
        .build();
    let focus_target = switcher.clone();
    window.connect_map(move |window| {
        gtk4::prelude::GtkWindowExt::set_focus(window, Some(&focus_target));
    });
    window.set_size_request(560, 480);
    PreferencesShell {
        window,
        navigation,
        stack,
        #[cfg(test)]
        switcher,
        #[cfg(test)]
        header,
    }
}

#[cfg(test)]
mod tests {
    use gtk4::gio;
    use gtk4::prelude::*;
    use libadwaita as adw;
    use libadwaita::prelude::*;

    use super::*;

    #[test]
    fn page_tabs_follow_the_design_order_with_synchronization() {
        assert_eq!(
            PAGE_ORDER,
            [
                PageId::Playback,
                PageId::Appearance,
                PageId::Layout,
                PageId::Library,
                PageId::Synchronization,
                PageId::Plugins,
            ]
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn preferences_are_a_movable_window_with_top_tabs() {
        gtk4::init().unwrap();
        let app = adw::Application::builder()
            .application_id("org.reprise.Reprise.PreferencesWindowTest")
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gio::Cancellable>).unwrap();
        let parent = adw::ApplicationWindow::builder().application(&app).build();
        let pages = PAGE_ORDER.map(|id| {
            let page = adw::PreferencesPage::builder()
                .title(id.title())
                .icon_name(id.icon_name())
                .build();
            (id, page)
        });

        let shell = build(&parent, pages);

        assert!(!shell.window.is_modal());
        assert_eq!(
            shell.window.transient_for().as_ref(),
            Some(parent.upcast_ref())
        );
        assert_eq!(shell.switcher.stack().as_ref(), Some(&shell.stack));
        assert!(shell.switcher.is_ancestor(&shell.header));
        assert_eq!(shell.stack.pages().n_items(), PAGE_ORDER.len() as u32);
        shell.window.close();
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn preferences_push_detail_pages_inside_the_existing_window() {
        gtk4::init().unwrap();
        let app = adw::Application::builder()
            .application_id("org.reprise.Reprise.PreferencesNavigationTest")
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gio::Cancellable>).unwrap();
        let parent = adw::ApplicationWindow::builder().application(&app).build();
        let pages = PAGE_ORDER.map(|id| {
            let page = adw::PreferencesPage::builder()
                .title(id.title())
                .icon_name(id.icon_name())
                .build();
            (id, page)
        });
        let shell = build(&parent, pages);
        let detail =
            adw::NavigationPage::new(&gtk4::Box::new(gtk4::Orientation::Vertical, 0), "Columns");

        shell.navigation.push(&detail);

        assert_eq!(shell.navigation.visible_page().as_ref(), Some(&detail));
        assert_eq!(
            shell.window.content().as_ref(),
            Some(shell.navigation.upcast_ref())
        );
        assert!(shell.navigation.pop());
        shell.window.close();
    }
}
