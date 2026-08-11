use gtk4::prelude::*;
use libadwaita as adw;

use super::strings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum PageId {
    Playback,
    Appearance,
    Layout,
    Library,
    Plugins,
}

pub(in crate::ui) const PAGE_ORDER: [PageId; 5] = [
    PageId::Playback,
    PageId::Appearance,
    PageId::Layout,
    PageId::Library,
    PageId::Plugins,
];

// Horizontal inset for the status chip, used only until the header's
// end-title-button strip has been allocated and can be measured: Adwaita's
// 40 px button strip plus the gap below.
const STATUS_CHIP_FALLBACK_END_INSET: i32 = 52;

// Breathing space between the status chip and the header's title buttons.
const STATUS_CHIP_END_GAP: i32 = 12;

impl PageId {
    pub(in crate::ui) fn name(self) -> &'static str {
        match self {
            Self::Playback => "playback",
            Self::Appearance => "appearance",
            Self::Layout => "layout",
            Self::Library => "library",
            Self::Plugins => "plugins",
        }
    }

    pub(in crate::ui) fn title(self) -> String {
        let message = match self {
            Self::Playback => strings::PREFERENCES_PLAYBACK,
            Self::Appearance => strings::PREFERENCES_APPEARANCE,
            Self::Layout => strings::PREFERENCES_LAYOUT,
            Self::Library => strings::PREFERENCES_LIBRARY,
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
/// The dialog used to build all five pages and hand them here, and both halves
/// of that cost scale with the page count: the pages themselves (measured 128 ms
/// median, Plugins alone 66–110 of it) and adding each one to the `ViewStack`,
/// which realises it (another 130 ms). Together that was two thirds of the
/// 314 ms it took the dialog to appear — spent on four pages nobody had asked
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
    edge_line: Option<&gtk4::Widget>,
    status_chip: Option<&gtk4::Widget>,
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
    let content_overlay = gtk4::Overlay::new();
    content_overlay.set_child(Some(&content_toolbar));
    if let Some(status_chip) = status_chip {
        status_chip.set_halign(gtk4::Align::End);
        status_chip.set_valign(gtk4::Align::Start);
        status_chip.set_margin_end(STATUS_CHIP_FALLBACK_END_INSET);
        content_overlay.add_overlay(status_chip);
        content_overlay.set_measure_overlay(status_chip, false);
        content_overlay.set_clip_overlay(status_chip, true);
    }
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
    if let Some(edge_line) = edge_line {
        edge_line.set_halign(gtk4::Align::Fill);
        edge_line.set_valign(gtk4::Align::Start);
        edge_line.set_hexpand(true);
        root_overlay.add_overlay(edge_line);
        root_overlay.set_measure_overlay(edge_line, false);
        root_overlay.set_clip_overlay(edge_line, true);
    }

    let dialog = adw::Dialog::builder()
        .child(&root_overlay)
        .title(strings::text(strings::PREFERENCES))
        .content_width(760)
        .content_height(680)
        .build();
    search.bind_shortcuts(&root_overlay);
    if let Some(status_chip) = status_chip {
        place_chip_when_visible(&dialog, &content_toolbar, &content_header, status_chip);
    }

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

/// Keeps the overlay chip aligned with the header it floats over: centred in
/// the header's height and clear of its title buttons.
///
/// GTK4 has no `size-allocate` signal, so the header's own re-allocation is
/// not directly observable — but `AdwToolbarView` publishes the very height
/// the placement depends on, and notifies when a font metric change (GNOME's
/// text scaling, a larger interface font) grows the header under an open
/// dialog.
fn place_chip_when_visible(
    dialog: &adw::Dialog,
    toolbar: &adw::ToolbarView,
    header: &adw::HeaderBar,
    chip: &gtk4::Widget,
) {
    let on_map = chip_placement_trigger(header, chip);
    dialog.connect_map(move |_| on_map());

    let on_header_resize = chip_placement_trigger(header, chip);
    toolbar.connect_top_bar_height_notify(move |_| on_header_resize());

    let on_visible = chip_placement_trigger(header, chip);
    chip.connect_visible_notify(move |_| on_visible());
}

fn chip_placement_trigger(header: &adw::HeaderBar, chip: &gtk4::Widget) -> impl Fn() {
    let header = header.downgrade();
    let chip = chip.downgrade();
    move || {
        let Some(header) = header.upgrade() else {
            return;
        };
        let Some(chip) = chip.upgrade().filter(gtk4::Widget::is_visible) else {
            return;
        };
        queue_chip_placement(&header, &chip);
    }
}

/// The header's end-title-button strip — the `GtkCenterBox` end child of
/// `AdwHeaderBar`'s template — once it is visible and allocated. `None` while
/// the header is unallocated, or if a future Adwaita lays its header out
/// differently; callers then keep the fallback inset.
fn header_end_strip(header: &adw::HeaderBar) -> Option<gtk4::Widget> {
    let center_box = descendant_center_box(header.upcast_ref())?;
    let end = center_box.end_widget()?;
    (end.is_visible() && end.width() > 0).then_some(end)
}

/// The horizontal inset that keeps the status chip clear of the header's
/// title buttons: the distance from the header's trailing edge to the strip's
/// leading edge, plus a gap.
fn header_end_inset(header: &adw::HeaderBar) -> Option<i32> {
    if header.width() <= 0 {
        return None;
    }
    let strip = header_end_strip(header)?;
    let origin = strip.compute_point(header, &gtk4::graphene::Point::new(0.0, 0.0))?;
    Some((header.width() - origin.x() as i32).max(0) + STATUS_CHIP_END_GAP)
}

fn descendant_center_box(widget: &gtk4::Widget) -> Option<gtk4::CenterBox> {
    if let Ok(center_box) = widget.clone().downcast::<gtk4::CenterBox>() {
        return Some(center_box);
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(center_box) = descendant_center_box(&current) {
            return Some(center_box);
        }
        child = current.next_sibling();
    }
    None
}

fn queue_chip_placement(header: &adw::HeaderBar, chip: &gtk4::Widget) {
    let header = header.downgrade();
    let chip = chip.downgrade();
    gtk4::glib::timeout_add_local(std::time::Duration::from_millis(1), move || {
        let Some(header) = header.upgrade() else {
            return gtk4::glib::ControlFlow::Break;
        };
        let Some(chip) = chip.upgrade() else {
            return gtk4::glib::ControlFlow::Break;
        };
        if header.height() <= 0 || chip.height() <= 0 {
            return gtk4::glib::ControlFlow::Continue;
        }
        chip.set_margin_top((header.height() - chip.height()).max(0) / 2);
        chip.set_margin_end(header_end_inset(&header).unwrap_or(STATUS_CHIP_FALLBACK_END_INSET));
        gtk4::glib::ControlFlow::Break
    });
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
mod tests {
    use std::path::PathBuf;

    use gtk4::gio;
    use gtk4::prelude::*;
    use libadwaita as adw;
    use libadwaita::prelude::*;
    use reprise_core::library::scanner::ScanProgress;

    use super::*;
    use crate::ui::scan_chrome::ScanChromeView;

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
    fn set_10_plugins_replaces_the_three_retired_peer_pages() {
        assert_eq!(
            PAGE_ORDER,
            [
                PageId::Playback,
                PageId::Appearance,
                PageId::Layout,
                PageId::Library,
                PageId::Plugins,
            ]
        );
        assert_eq!(page_index_by_name("plugins"), Some(4));
        for retired in ["online_sources", "new_releases", "concerts"] {
            assert_eq!(page_index_by_name(retired), None);
        }
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

        let shell = build(pages, None, None);

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
        let shell = build(pages, None, None);
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

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fb_9_header_and_content_allocations_do_not_move_when_chrome_appears() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let app = adw::Application::builder()
            .application_id("io.github.marvinbaudach.Reprise.PreferencesChromeGeometryTest")
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gio::Cancellable>).unwrap();
        let parent = adw::ApplicationWindow::new(&app);
        parent.set_default_size(900, 760);
        parent.present();
        crate::ui::style::install();
        let pages = test_pages();
        let chrome = ScanChromeView::new();
        let shell = build(
            pages,
            Some(chrome.line_widget()),
            Some(chrome.chip_widget()),
        );
        shell.dialog.present(Some(&parent));
        settle_layout();

        let header_height = shell.content_header.height();
        let title_position = shell
            .content_title
            .compute_point(&shell.content_header, &gtk4::graphene::Point::new(0.0, 0.0))
            .expect("content title must be allocated inside its header");
        let content_position = shell
            .stack
            .compute_point(&shell.root_overlay, &gtk4::graphene::Point::new(0.0, 0.0))
            .expect("page stack must be allocated inside the dialog");

        chrome.show(&ScanProgress::Scanning {
            processed: 39,
            total: Some(100),
            current_path: PathBuf::from("/music/track.flac"),
        });
        settle_layout();

        assert_eq!(shell.content_header.height(), header_height);
        assert_eq!(
            shell
                .content_title
                .compute_point(&shell.content_header, &gtk4::graphene::Point::new(0.0, 0.0),),
            Some(title_position)
        );
        assert_eq!(
            shell
                .stack
                .compute_point(&shell.root_overlay, &gtk4::graphene::Point::new(0.0, 0.0),),
            Some(content_position)
        );
        assert_eq!(
            chrome.chip_widget().margin_top(),
            (shell.content_header.height() - chrome.chip_widget().height()).max(0) / 2,
            "chip inset must be derived from the allocated header height"
        );
        let chip_position = chrome
            .chip_widget()
            .compute_point(&shell.content_header, &gtk4::graphene::Point::new(0.0, 0.0))
            .expect("chip and header must share the content overlay");
        assert!(
            chip_position.x() + chrome.chip_widget().width() as f32
                <= (shell.content_header.width() - 40) as f32,
            "the overlay chip must leave the header close-button strip clickable"
        );

        shell.dialog.force_close();
        parent.close();
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fb_9_one_chrome_instance_survives_all_five_page_switches() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let app = adw::Application::builder()
            .application_id("io.github.marvinbaudach.Reprise.PreferencesChromePagesTest")
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gio::Cancellable>).unwrap();
        let parent = adw::ApplicationWindow::new(&app);
        parent.set_default_size(900, 760);
        parent.present();
        crate::ui::style::install();
        let chrome = ScanChromeView::new();
        chrome.show(&ScanProgress::Scanning {
            processed: 39,
            total: Some(100),
            current_path: PathBuf::from("/music/track.flac"),
        });
        let shell = build(
            test_pages(),
            Some(chrome.line_widget()),
            Some(chrome.chip_widget()),
        );
        shell.dialog.present(Some(&parent));
        settle_layout();
        let line_parent = chrome.line_widget().parent();
        let chip_parent = chrome.chip_widget().parent();

        assert_eq!(
            chrome.chip_widget().margin_top(),
            (shell.content_header.height() - chrome.chip_widget().height()).max(0) / 2,
            "an initially visible replayed chip must also be centered"
        );

        for index in 0..PAGE_ORDER.len() as i32 {
            shell
                .sidebar
                .select_row(shell.sidebar.row_at_index(index).as_ref());
            settle_layout();
            assert!(chrome.line_widget().is_visible());
            assert!(chrome.chip_widget().is_visible());
            assert_eq!(chrome.line_widget().parent(), line_parent);
            assert_eq!(chrome.chip_widget().parent(), chip_parent);
            assert!(chrome.line_widget().is_ancestor(&shell.root_overlay));
            assert!(chrome.chip_widget().is_ancestor(&shell.root_overlay));
        }
        let mut ancestor = shell.root_overlay.parent();
        let mut clipped_inside_dialog = false;
        while let Some(widget) = ancestor {
            clipped_inside_dialog |= widget.overflow() == gtk4::Overflow::Hidden;
            ancestor = widget.parent();
        }
        assert!(
            clipped_inside_dialog,
            "the dialog host must clip the edge line inside its rounded surface"
        );

        shell.dialog.force_close();
        parent.close();
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fb_9_visual_scan_chrome_fixture() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let parent = gtk4::Window::builder()
            .title("Reprise FB-9 Visual Fixture")
            .default_width(900)
            .default_height(760)
            .build();
        parent.present();
        settle_layout();
        assert!(parent.is_mapped());
        crate::ui::style::install();

        let chrome = ScanChromeView::new();
        if std::env::var("REPRISE_FB9_VISUAL_STATE").as_deref() == Ok("running") {
            chrome.show(&ScanProgress::Scanning {
                processed: 748,
                total: Some(1_909),
                current_path: PathBuf::from("/music/Album/track.flac"),
            });
        }
        let shell = build(
            test_pages(),
            Some(chrome.line_widget()),
            Some(chrome.chip_widget()),
        );
        shell.dialog.present(Some(&parent));
        settle_layout();
        assert!(shell.dialog.is_mapped());

        let hold_ms = std::env::var("REPRISE_FB9_VISUAL_HOLD_MS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(200);
        settle_for(std::time::Duration::from_millis(hold_ms));

        shell.dialog.force_close();
        parent.close();
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

    fn settle_for(duration: std::time::Duration) {
        let main_loop = gtk4::glib::MainLoop::new(None, false);
        let quit = main_loop.clone();
        gtk4::glib::timeout_add_local_once(duration, move || quit.quit());
        main_loop.run();
    }
}
