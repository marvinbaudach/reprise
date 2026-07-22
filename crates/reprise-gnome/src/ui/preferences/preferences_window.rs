use gtk4::prelude::*;
use libadwaita as adw;

use super::device_sync_strings;
use super::strings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum PageId {
    Playback,
    Appearance,
    Layout,
    Library,
    Synchronization,
    Plugins,
    Experimental,
}

pub(in crate::ui) const PAGE_ORDER: [PageId; 7] = [
    PageId::Playback,
    PageId::Appearance,
    PageId::Layout,
    PageId::Library,
    PageId::Synchronization,
    PageId::Plugins,
    PageId::Experimental,
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
            Self::Experimental => "experimental",
        }
    }

    pub(in crate::ui) fn title(self) -> String {
        let message = match self {
            Self::Playback => strings::PREFERENCES_PLAYBACK,
            Self::Appearance => strings::PREFERENCES_APPEARANCE,
            Self::Layout => strings::PREFERENCES_LAYOUT,
            Self::Library => strings::PREFERENCES_LIBRARY,
            Self::Synchronization => device_sync_strings::SYNCHRONIZATION,
            Self::Plugins => strings::PREFERENCES_PLUGINS,
            Self::Experimental => strings::EXPERIMENTAL_PAGE_TITLE,
        };
        strings::text(message)
    }

    pub(in crate::ui) fn icon_name(self) -> &'static str {
        match self {
            Self::Playback => "audio-speakers-symbolic",
            Self::Appearance => "applications-graphics-symbolic",
            Self::Layout => "view-grid-symbolic",
            Self::Library => "folder-music-symbolic",
            Self::Synchronization => "phone-symbolic",
            Self::Plugins => "application-x-addon-symbolic",
            Self::Experimental => "applications-science-symbolic",
        }
    }
}

pub(in crate::ui) struct PreferencesShell {
    pub(in crate::ui) dialog: adw::Dialog,
    pub(in crate::ui) navigation: adw::NavigationView,
    pub(in crate::ui) stack: adw::ViewStack,
    pub(in crate::ui) sidebar: gtk4::ListBox,
}

/// One sidebar entry (icon + title) for `id`; its list index equals the
/// page's `PAGE_ORDER` position, which is how selection maps back to a page.
fn sidebar_row(id: PageId) -> gtk4::ListBoxRow {
    let icon = gtk4::Image::from_icon_name(id.icon_name());
    let label = gtk4::Label::new(Some(&id.title()));
    label.set_xalign(0.0);
    let row_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    row_box.set_margin_top(8);
    row_box.set_margin_bottom(8);
    row_box.set_margin_start(6);
    row_box.set_margin_end(6);
    row_box.append(&icon);
    row_box.append(&label);
    let row = gtk4::ListBoxRow::new();
    // a11y-semantics: role=list-item name=page-title state=selected action=focus/navigate
    row.set_focusable(true);
    row.set_child(Some(&row_box));
    row
}

fn appearance_index() -> i32 {
    PAGE_ORDER
        .iter()
        .position(|id| *id == PageId::Appearance)
        .unwrap_or(0) as i32
}

/// Returns the sidebar row index for the page whose stack name matches
/// `name`, or `None` if no page matches.
pub(in crate::ui) fn page_index_by_name(name: &str) -> Option<i32> {
    PAGE_ORDER
        .iter()
        .position(|id| id.name() == name)
        .map(|i| i as i32)
}

pub(in crate::ui) fn selected_sidebar_focus_target(sidebar: &gtk4::ListBox) -> gtk4::Widget {
    let Some(row) = sidebar.selected_row() else {
        return sidebar.clone().upcast();
    };
    row.upcast()
}

pub(in crate::ui) fn build(
    pages: [(PageId, adw::PreferencesPage); 7],
    foreground_top_bar: Option<&gtk4::Widget>,
) -> PreferencesShell {
    let stack = adw::ViewStack::new();
    stack.set_vexpand(true);
    for (id, page) in pages {
        page.add_css_class("reprise-preferences-page");
        stack.add_titled_with_icon(&page, Some(id.name()), &id.title(), id.icon_name());
    }

    // Vertical page navigation (redesign): a sidebar list drives the stack,
    // replacing the former top ViewSwitcher. A row's list index equals its
    // `PAGE_ORDER` position, so selection maps straight back to a page.
    let sidebar_list = gtk4::ListBox::new();
    sidebar_list.add_css_class("navigation-sidebar");
    sidebar_list.set_selection_mode(gtk4::SelectionMode::Single);
    for id in PAGE_ORDER {
        sidebar_list.append(&sidebar_row(id));
    }

    let content_title = adw::WindowTitle::new(&PageId::Appearance.title(), "");
    sidebar_list.connect_row_selected({
        let stack = stack.clone();
        let content_title = content_title.clone();
        move |_, row| {
            let Some(row) = row else { return };
            let Some(id) = PAGE_ORDER.get(row.index() as usize).copied() else {
                return;
            };
            stack.set_visible_child_name(id.name());
            content_title.set_title(&id.title());
        }
    });

    let sidebar_header = adw::HeaderBar::new();
    sidebar_header.set_title_widget(Some(&adw::WindowTitle::new(
        &strings::text(strings::PREFERENCES),
        "",
    )));
    let sidebar_scroll = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&sidebar_list)
        .build();
    let sidebar_toolbar = adw::ToolbarView::new();
    sidebar_toolbar.add_top_bar(&sidebar_header);
    sidebar_toolbar.set_content(Some(&sidebar_scroll));
    let sidebar_page =
        adw::NavigationPage::new(&sidebar_toolbar, &strings::text(strings::PREFERENCES));

    let content_header = adw::HeaderBar::new();
    content_header.set_title_widget(Some(&content_title));
    let content_toolbar = adw::ToolbarView::new();
    content_toolbar.add_top_bar(&content_header);
    if let Some(foreground_top_bar) = foreground_top_bar {
        content_toolbar.add_top_bar(foreground_top_bar);
    }
    content_toolbar.set_content(Some(&stack));
    let content_page = adw::NavigationPage::new(&content_toolbar, &PageId::Appearance.title());

    let split = adw::NavigationSplitView::builder()
        .sidebar(&sidebar_page)
        .content(&content_page)
        .build();

    // Start on Appearance (the established default), highlighting its row —
    // which also drives the stack and content title through the handler.
    stack.set_visible_child_name("appearance");
    sidebar_list.select_row(sidebar_list.row_at_index(appearance_index()).as_ref());

    let root_page =
        adw::NavigationPage::with_tag(&split, &strings::text(strings::PREFERENCES), "preferences");
    let navigation = adw::NavigationView::new();
    navigation.add(&root_page);

    let dialog = adw::Dialog::builder()
        .child(&navigation)
        .title(strings::text(strings::PREFERENCES))
        .content_width(760)
        .content_height(680)
        .build();

    PreferencesShell {
        dialog,
        navigation,
        stack,
        sidebar: sidebar_list,
    }
}

pub(in crate::ui) fn css() -> String {
    ".reprise-preferences-page > scrolledwindow > viewport > clamp > box { \
     margin: 12px; \
     border-spacing: 18px; \
     }"
    .to_string()
}

#[cfg(test)]
mod tests {
    use gtk4::gio;
    use gtk4::prelude::*;
    use libadwaita as adw;
    use libadwaita::prelude::*;

    use super::*;

    #[test]
    fn page_spacing_css_applies_one_compact_inset_to_every_settings_page() {
        let css = css();
        assert!(css.contains(".reprise-preferences-page"));
        assert!(css.contains("margin: 12px"));
    }

    #[test]
    fn set_5_preferences_short_pages_expand_from_the_top() {
        let source = include_str!("preferences_window.rs");
        let stack_expansion = ["stack.set_", "vexpand(true);"].concat();

        assert_eq!(
            source.matches(&stack_expansion).count(),
            1,
            "the page stack must fill the toolbar content height so short pages stay top-aligned"
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn page_spacing_css_parses_without_gtk_errors() {
        gtk4::init().unwrap();
        let errors = crate::ui::style::css_parse_errors(&css());
        assert!(
            errors.is_empty(),
            "GTK reported CSS parsing errors: {errors:?}"
        );
    }

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
                PageId::Experimental,
            ]
        );
    }

    #[test]
    fn acc_3_preferences_focuses_the_selected_navigation_row_not_its_container() {
        let source = include_str!("preferences.rs");
        let shell_source = include_str!("preferences_window.rs");
        let explicit_focusable = ["row.set_", "focusable(true);"].concat();
        let semantic_focusable = [
            "// a11y-semantics: role=list-item name=page-title state=selected ",
            "action=focus/navigate\n    row.set_",
            "focusable(true);",
        ]
        .concat();

        assert!(source.contains("selected_sidebar_focus_target(&shell.sidebar)"));
        assert!(!source.contains("bind_closable_dialog(&shell.dialog, &shell.sidebar)"));
        assert_eq!(shell_source.matches(&explicit_focusable).count(), 1);
        assert!(shell_source.contains(&semantic_focusable));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn preferences_are_a_dialog_with_a_page_sidebar() {
        gtk4::init().unwrap();
        let app = adw::Application::builder()
            .application_id("org.reprise.Reprise.PreferencesWindowTest")
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gio::Cancellable>).unwrap();
        let pages = PAGE_ORDER.map(|id| {
            let page = adw::PreferencesPage::builder()
                .title(id.title())
                .icon_name(id.icon_name())
                .build();
            (id, page)
        });

        let shell = build(pages, None);

        assert_eq!(shell.dialog.content_width(), 760);
        assert_eq!(shell.dialog.content_height(), 680);
        assert!(shell
            .sidebar
            .row_at_index(PAGE_ORDER.len() as i32 - 1)
            .is_some());
        assert!(shell
            .sidebar
            .row_at_index(PAGE_ORDER.len() as i32)
            .is_none());
        assert_eq!(shell.stack.pages().n_items(), PAGE_ORDER.len() as u32);
        assert_eq!(
            selected_sidebar_focus_target(&shell.sidebar),
            shell
                .sidebar
                .selected_row()
                .unwrap()
                .upcast::<gtk4::Widget>()
        );
        for id in PAGE_ORDER {
            assert!(shell
                .stack
                .child_by_name(id.name())
                .is_some_and(|page| page.has_css_class("reprise-preferences-page")));
        }
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn preferences_push_detail_pages_inside_the_dialog() {
        gtk4::init().unwrap();
        let app = adw::Application::builder()
            .application_id("org.reprise.Reprise.PreferencesNavigationTest")
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gio::Cancellable>).unwrap();
        let pages = PAGE_ORDER.map(|id| {
            let page = adw::PreferencesPage::builder()
                .title(id.title())
                .icon_name(id.icon_name())
                .build();
            (id, page)
        });
        let shell = build(pages, None);
        let detail =
            adw::NavigationPage::new(&gtk4::Box::new(gtk4::Orientation::Vertical, 0), "Columns");

        shell.navigation.push(&detail);

        assert_eq!(shell.navigation.visible_page().as_ref(), Some(&detail));
        assert_eq!(
            shell.dialog.child().as_ref(),
            Some(shell.navigation.upcast_ref())
        );
        assert!(shell.navigation.pop());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn foreground_progress_is_parented_inside_the_preferences_dialog() {
        gtk4::init().unwrap();
        let app = adw::Application::builder()
            .application_id("org.reprise.Reprise.PreferencesProgressTest")
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gio::Cancellable>).unwrap();
        let pages = PAGE_ORDER.map(|id| {
            let page = adw::PreferencesPage::builder()
                .title(id.title())
                .icon_name(id.icon_name())
                .build();
            (id, page)
        });
        let progress = gtk4::Revealer::new();

        let shell = build(pages, Some(progress.upcast_ref()));

        assert!(progress.parent().is_some());
        assert_eq!(
            shell.dialog.child().as_ref(),
            Some(shell.navigation.upcast_ref())
        );
        assert!(progress.is_ancestor(&shell.navigation));
    }
}
