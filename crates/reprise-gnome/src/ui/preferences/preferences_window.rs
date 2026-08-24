use gtk4::prelude::*;
use libadwaita as adw;

use super::strings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum PageId {
    Playback,
    Appearance,
    Layout,
    Library,
    Location,
    Plugins,
}

/// The dialog's authored size. The content height is the 680 px the pages were
/// laid out against plus the tallest height the background-activity bar takes
/// at rest (`SET-18`) — measured on 2026-08-24, 1600x900, Adwaita defaults:
/// 46 px with the gate on and nothing running, 72 px with the gate off, where
/// the bar also carries the line that says why it is empty.
///
/// Without the addition the bar would take its place *out of* the pages: the
/// Layout page's last two switch rows fell below the fold and stopped being
/// clickable at all, which the pointer harness caught. A permanent bottom bar
/// costs permanent height, so the dialog pays for it rather than the pages.
/// While jobs actually run the bar is taller still and the page does give up
/// those rows — that is transient, and it is the state the reader is looking
/// at the bar in anyway.
const PREFERENCES_CONTENT_WIDTH: i32 = 760;
/// What the pinned sidebar takes out of that width. `pin_sidebar_width` freezes
/// whatever `AdwNavigationSplitView` allotted at map time, which at this dialog
/// width is 195 px (measured 2026-08-24, Adwaita defaults). Everything to the
/// right of it — the pages and the background-activity footer — divides up the
/// rest, so this is the figure their width budgets are drawn against — which
/// is where it is read, so the guard carries it rather than the layout code.
#[cfg(test)]
const SIDEBAR_WIDTH_BUDGET_PX: i32 = 195;
const BACKGROUND_BAR_RESTING_HEIGHT: i32 = 72;
const PREFERENCES_CONTENT_HEIGHT: i32 = 680 + BACKGROUND_BAR_RESTING_HEIGHT;

pub(in crate::ui) const PAGE_ORDER: [PageId; 6] = [
    PageId::Playback,
    PageId::Appearance,
    PageId::Layout,
    PageId::Library,
    PageId::Location,
    PageId::Plugins,
];

impl PageId {
    pub(in crate::ui) fn name(self) -> &'static str {
        match self {
            Self::Playback => "playback",
            Self::Appearance => "appearance",
            Self::Layout => "layout",
            Self::Library => "library",
            Self::Location => "location",
            Self::Plugins => "plugins",
        }
    }

    pub(in crate::ui) fn title(self) -> String {
        let message = match self {
            Self::Playback => strings::PREFERENCES_PLAYBACK,
            Self::Appearance => strings::PREFERENCES_APPEARANCE,
            Self::Layout => strings::PREFERENCES_LAYOUT,
            Self::Library => strings::PREFERENCES_LIBRARY,
            Self::Location => strings::PREFERENCES_LOCATION,
            Self::Plugins => strings::PREFERENCES_PLUGINS,
        };
        strings::text(message)
    }

    pub(in crate::ui) fn icon_name(self) -> &'static str {
        match self {
            Self::Playback => "audio-speakers-symbolic",
            Self::Appearance => "applications-graphics-symbolic",
            Self::Layout => "view-grid-symbolic",
            Self::Library => "folder-music-symbolic",
            Self::Location => "find-location-symbolic",
            Self::Plugins => "application-x-addon-symbolic",
        }
    }
}

pub(in crate::ui) struct PreferencesShell {
    pub(in crate::ui) dialog: adw::Dialog,
    pub(in crate::ui) navigation: adw::NavigationView,
    pub(in crate::ui) stack: adw::ViewStack,
    pub(in crate::ui) sidebar: gtk4::ListBox,
    pub(in crate::ui) search: std::rc::Rc<super::preferences_search::SettingsSearch>,
    #[cfg(test)]
    pub(in crate::ui) root_overlay: gtk4::Overlay,
    #[cfg(test)]
    pub(in crate::ui) content_header: adw::HeaderBar,
    #[cfg(test)]
    pub(in crate::ui) content_title: adw::WindowTitle,
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
    row.set_widget_name(id.name());
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

/// SET-8: builds a page on first sight, not on open.
///
/// The dialog used to build every page and hand them here, and both halves
/// of that cost scale with the page count: the pages themselves (measured 128 ms
/// median, Plugins alone 66–110 of it) and adding each one to the `ViewStack`,
/// which realises it (another 130 ms). Together that was two thirds of the
/// 314 ms it took the dialog to appear — mostly on pages nobody had asked
/// to see.
///
/// Each stack child is therefore an empty `adw::Bin`, filled the moment its page
/// becomes visible. **Synchronously**, via `visible-child` notification: the
/// sidebar's row-selected handler, the `initial_page` navigation and the smoke
/// hooks all reach a page by setting the visible child, and callers that follow
/// such a jump — `present_plugins` highlighting rows it just navigated to —
/// must find the page already there. An idle-deferred build would hand them an
/// empty one.
pub(in crate::ui) fn build(
    page_factory: std::rc::Rc<dyn Fn(PageId) -> adw::PreferencesPage>,
    background_bar: Option<&gtk4::Widget>,
) -> PreferencesShell {
    let stack = adw::ViewStack::new();
    stack.set_vexpand(true);
    for id in PAGE_ORDER {
        let holder = adw::Bin::new();
        holder.set_vexpand(true);
        stack.add_titled_with_icon(&holder, Some(id.name()), &id.title(), id.icon_name());
    }

    let materialize_page = {
        let stack = stack.clone();
        std::rc::Rc::new(move |id: PageId| {
            let Some(holder) = stack
                .child_by_name(id.name())
                .and_then(|child| child.downcast::<adw::Bin>().ok())
            else {
                return;
            };
            // Qualified: `child`/`set_child` exist on several traits in scope
            // through the two preludes, and an unqualified call resolves to the
            // wrong one.
            if adw::prelude::BinExt::child(&holder).is_some() {
                return;
            }
            let page = page_factory(id);
            page.add_css_class("reprise-preferences-page");
            adw::prelude::BinExt::set_child(&holder, Some(&page));
        })
    };

    stack.connect_visible_child_notify({
        let materialize_page = materialize_page.clone();
        move |stack| {
            let Some(name) = stack.visible_child_name() else {
                return;
            };
            if let Some(id) = PAGE_ORDER.iter().find(|id| id.name() == name.as_str()) {
                materialize_page(*id);
            }
        }
    });

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
            let name = row.widget_name();
            let Some(id) = PAGE_ORDER.iter().find(|id| id.name() == name.as_str()) else {
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
    let search = super::preferences_search::SettingsSearch::install(
        &sidebar_list,
        &stack,
        &content_title,
        &content_header,
        &content_toolbar,
        materialize_page.clone(),
    );
    // The head belongs to the title and the search, and nothing is laid over
    // it. Background work goes to a bottom bar instead: a fixed place that does
    // not scroll and does not move (`SET-18`).
    if let Some(background_bar) = background_bar {
        content_toolbar.add_bottom_bar(background_bar);
    }
    let content_overlay = gtk4::Overlay::new();
    content_overlay.set_child(Some(&content_toolbar));
    let content_page = adw::NavigationPage::new(&content_overlay, &PageId::Appearance.title());

    let split = adw::NavigationSplitView::builder()
        .sidebar(&sidebar_page)
        .content(&content_page)
        .build();
    search.pin_sidebar_width(&split);

    // Start on Appearance (the established default), highlighting its row —
    // which also drives the stack and content title through the handler.
    //
    // SET-8: this is also what materializes the opening page. The
    // `visible-child` notification below fires from here, so no page needs
    // building before this point — and building `PAGE_ORDER`'s first entry
    // eagerly would build Playback, which is not the page anyone is about to
    // see. SET-13 deliberately keeps that fast opening path: a populated
    // profile measured 314 ms when all pages were eager (against 22 ms here),
    // so settings search materializes and indexes the other pages only on the
    // first non-empty query.
    stack.set_visible_child_name("appearance");
    sidebar_list.select_row(sidebar_list.row_at_index(appearance_index()).as_ref());
    // Belt and braces: if the stack ever opens on a page whose notification
    // did not fire (an identical name assignment emits nothing), the visible
    // page would stay an empty holder. Ask for it once, explicitly.
    if let Some(name) = stack.visible_child_name() {
        if let Some(id) = PAGE_ORDER.iter().find(|id| id.name() == name.as_str()) {
            materialize_page(*id);
        }
    }

    let root_page =
        adw::NavigationPage::with_tag(&split, &strings::text(strings::PREFERENCES), "preferences");
    let navigation = adw::NavigationView::new();
    navigation.add(&root_page);

    let root_overlay = gtk4::Overlay::new();
    root_overlay.set_child(Some(&navigation));

    let dialog = adw::Dialog::builder()
        .child(&root_overlay)
        .title(strings::text(strings::PREFERENCES))
        .content_width(PREFERENCES_CONTENT_WIDTH)
        .content_height(PREFERENCES_CONTENT_HEIGHT)
        .build();
    search.bind_shortcuts(&root_overlay);

    PreferencesShell {
        dialog,
        navigation,
        stack,
        sidebar: sidebar_list,
        search,
        #[cfg(test)]
        root_overlay,
        #[cfg(test)]
        content_header,
        #[cfg(test)]
        content_title,
    }
}

pub(in crate::ui) fn css() -> String {
    format!(
        ".reprise-preferences-page > scrolledwindow > viewport > clamp > box {{ \
     margin: 12px; \
     border-spacing: 18px; \
     }} {}",
        super::preferences_search::css()
    )
}

#[cfg(test)]
#[path = "preferences_chrome_placement_tests.rs"]
mod chrome_placement_tests;

#[cfg(test)]
#[path = "preferences_location_registration_tests.rs"]
mod location_registration_tests;

#[cfg(test)]
mod tests {

    use gtk4::gio;
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
    fn fb_9_foreground_progress_is_not_a_toolbar_top_bar() {
        let source = include_str!("preferences_window.rs");
        let retired_parameter = ["foreground_", "top_bar"].concat();
        let retired_parenting = ["content_toolbar.add_", "top_bar(foreground"].concat();
        assert!(!source.contains(&retired_parameter));
        assert!(!source.contains(&retired_parenting));
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
            .application_id("io.github.marvinbaudach.Reprise.PreferencesWindowTest")
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gio::Cancellable>).unwrap();
        let pages: std::rc::Rc<dyn Fn(PageId) -> adw::PreferencesPage> =
            std::rc::Rc::new(|id: PageId| {
                adw::PreferencesPage::builder()
                    .title(id.title())
                    .icon_name(id.icon_name())
                    .build()
            });

        let shell = build(pages, None);

        assert_eq!(shell.dialog.content_width(), PREFERENCES_CONTENT_WIDTH);
        assert_eq!(shell.dialog.content_height(), PREFERENCES_CONTENT_HEIGHT);
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
            shell.stack.set_visible_child_name(id.name());
            let holder = shell.stack.child_by_name(id.name()).unwrap();
            let page: adw::PreferencesPage = holder.first_child().unwrap().downcast().unwrap();
            assert!(page.has_css_class("reprise-preferences-page"));
        }
        assert_eq!(
            shell.dialog.child().as_ref(),
            Some(shell.root_overlay.upcast_ref())
        );
        assert!(shell.navigation.is_ancestor(&shell.root_overlay));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn preferences_push_detail_pages_inside_the_dialog() {
        gtk4::init().unwrap();
        let app = adw::Application::builder()
            .application_id("io.github.marvinbaudach.Reprise.PreferencesNavigationTest")
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gio::Cancellable>).unwrap();
        let pages: std::rc::Rc<dyn Fn(PageId) -> adw::PreferencesPage> =
            std::rc::Rc::new(|id: PageId| {
                adw::PreferencesPage::builder()
                    .title(id.title())
                    .icon_name(id.icon_name())
                    .build()
            });
        let shell = build(pages, None);
        let detail =
            adw::NavigationPage::new(&gtk4::Box::new(gtk4::Orientation::Vertical, 0), "Columns");

        shell.navigation.push(&detail);

        assert_eq!(shell.navigation.visible_page().as_ref(), Some(&detail));
        assert_eq!(
            shell.dialog.child().as_ref(),
            Some(shell.root_overlay.upcast_ref())
        );
        assert!(shell.navigation.pop());
    }

    /// SET-8: a factory, like the real caller hands `build`. The pages carry a
    /// group and a row so a materialized page has real content to allocate —
    /// the geometry tests below measure where things land, and an empty page
    /// would let them pass without proving anything.
    pub(super) fn test_pages() -> std::rc::Rc<dyn Fn(PageId) -> adw::PreferencesPage> {
        std::rc::Rc::new(|id: PageId| {
            let page = adw::PreferencesPage::builder()
                .title(id.title())
                .icon_name(id.icon_name())
                .build();
            let group = adw::PreferencesGroup::new();
            group.add(&adw::ActionRow::builder().title(id.title()).build());
            page.add(&group);
            page
        })
    }

    pub(super) fn settle_layout() {
        settle_for(std::time::Duration::from_millis(80));
    }

    pub(super) fn settle_for(duration: std::time::Duration) {
        let main_loop = gtk4::glib::MainLoop::new(None, false);
        let quit = main_loop.clone();
        gtk4::glib::timeout_add_local_once(duration, move || quit.quit());
        main_loop.run();
    }
}
