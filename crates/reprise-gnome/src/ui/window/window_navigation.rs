use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::db::Db;

use super::device_sync_runtime::DeviceSyncRuntime;

pub(in crate::ui) fn apply_sidebar_visibility(
    split_view: &adw::OverlaySplitView,
    sidebar_page: &adw::NavigationPage,
    visible: bool,
) {
    if visible {
        sidebar_page.set_visible(true);
        split_view.set_show_sidebar(!split_view.is_collapsed());
    } else {
        clear_sidebar_focus(split_view, sidebar_page);
        split_view.set_show_sidebar(false);
        sidebar_page.set_visible(false);
    }
}

fn clear_sidebar_focus(split_view: &adw::OverlaySplitView, sidebar_page: &adw::NavigationPage) {
    let Some(window) = split_view
        .root()
        .and_then(|root| root.downcast::<gtk4::Window>().ok())
    else {
        return;
    };
    let Some(focus) = gtk4::prelude::GtkWindowExt::focus(&window) else {
        return;
    };
    if focus.is_ancestor(sidebar_page) {
        gtk4::prelude::GtkWindowExt::set_focus(&window, gtk4::Widget::NONE);
    }
}

fn sidebar_toggle_is_visible(has_sidebar: bool) -> bool {
    has_sidebar
}

fn sidebar_toggle_focus_on_click() -> bool {
    false
}

fn sync_sidebar_toggle(
    sidebar_toggle: &gtk4::ToggleButton,
    split_view: &adw::OverlaySplitView,
    sidebar_page: &adw::NavigationPage,
    updating: &std::cell::Cell<bool>,
) {
    updating.set(true);
    let has_sidebar = sidebar_page.is_visible();
    sidebar_toggle.set_visible(sidebar_toggle_is_visible(has_sidebar));
    sidebar_toggle.set_active(has_sidebar && split_view.shows_sidebar());
    updating.set(false);
}

fn show_sidebar(split_view: &adw::OverlaySplitView, manually_hidden: &std::cell::Cell<bool>) {
    manually_hidden.set(false);
    split_view.set_show_sidebar(true);
}

fn hide_sidebar(split_view: &adw::OverlaySplitView, manually_hidden: &std::cell::Cell<bool>) {
    manually_hidden.set(true);
    split_view.set_show_sidebar(false);
}

fn activate_sidebar_route(
    collapsed: bool,
    show_library_root: impl FnOnce(),
    hide_sidebar_overlay: impl FnOnce(),
) {
    show_library_root();
    if collapsed {
        hide_sidebar_overlay();
    }
}

fn show_library_content_root(content_navigation: &adw::NavigationView) {
    if let Some(root) = content_navigation.find_page(super::now_playing_wiring::LIBRARY_CONTENT_TAG)
    {
        content_navigation.pop_to_page(&root);
    }
}

pub(in crate::ui) fn show_content_page(
    content_navigation: &adw::NavigationView,
    content_stack: &gtk4::Stack,
    name: &str,
) {
    show_library_content_root(content_navigation);
    super::content_stack::show_page(content_stack, name);
}

pub(in crate::ui) fn show_content_callback(
    split_view: &adw::OverlaySplitView,
    content_navigation: &adw::NavigationView,
) -> Rc<dyn Fn()> {
    let split_view = split_view.downgrade();
    let content_navigation = content_navigation.downgrade();
    Rc::new(
        move || match (split_view.upgrade(), content_navigation.upgrade()) {
            (Some(split_view), Some(content_navigation)) => {
                activate_sidebar_route(
                    split_view.is_collapsed(),
                    || show_library_content_root(&content_navigation),
                    || split_view.set_show_sidebar(false),
                );
            }
            _ => {
                tracing::warn!(
                    "split view is gone; cannot show content pane after sidebar navigation"
                );
            }
        },
    )
}

pub(in crate::ui) fn open_device_place(
    content_navigation: &adw::NavigationView,
    content_stack: &gtk4::Stack,
    window_title: &adw::WindowTitle,
    device_id: &str,
    runtime: &Rc<DeviceSyncRuntime>,
    split_view: &adw::OverlaySplitView,
) -> bool {
    show_library_content_root(content_navigation);
    if !super::device_sync_page::open(content_stack, window_title, device_id, runtime) {
        return false;
    }
    if split_view.is_collapsed() {
        split_view.set_show_sidebar(false);
    }
    true
}

pub(in crate::ui) fn open_device_callback(
    content_navigation: &adw::NavigationView,
    content_stack: &gtk4::Stack,
    window_title: &adw::WindowTitle,
    runtime: &Rc<DeviceSyncRuntime>,
    split_view: &adw::OverlaySplitView,
) -> super::device_sync_launcher::OpenDevice {
    let content_navigation = content_navigation.clone();
    let content_stack = content_stack.clone();
    let window_title = window_title.clone();
    let runtime = runtime.clone();
    let split_view = split_view.clone();
    Rc::new(move |device_id, _| {
        if !open_device_place(
            &content_navigation,
            &content_stack,
            &window_title,
            &device_id,
            &runtime,
            &split_view,
        ) {
            tracing::warn!(device_id, "could not open Android sync page");
        }
    })
}

pub(in crate::ui) fn wire_sidebar_toggle(
    sidebar_toggle: &gtk4::ToggleButton,
    split_view: &adw::OverlaySplitView,
    sidebar_page: &adw::NavigationPage,
    conn: &Rc<Db>,
) {
    sidebar_toggle.add_css_class(crate::ui::shortcuts::SIDEBAR_TOGGLE_CSS_CLASS);
    // Keep pointer use from moving focus away from the content surface:
    // the button remains focusable for Enter, while the window-level Space
    // controller recognizes its dedicated class and routes Space exclusively
    // to global play/pause.
    sidebar_toggle.set_focus_on_click(sidebar_toggle_focus_on_click());
    let updating = Rc::new(std::cell::Cell::new(false));
    let manually_hidden = Rc::new(std::cell::Cell::new(false));
    // Restore last session's manual collapse before the initial toggle sync,
    // so the button starts in the matching state.
    if reprise_core::library::settings::get_sidebar_collapsed(conn) && sidebar_page.is_visible() {
        hide_sidebar(split_view, &manually_hidden);
    }
    sync_sidebar_toggle(sidebar_toggle, split_view, sidebar_page, &updating);
    {
        let split_view = split_view.clone();
        let sidebar_page = sidebar_page.clone();
        let manually_hidden = manually_hidden.clone();
        let updating = updating.clone();
        let conn = conn.clone();
        sidebar_toggle.connect_toggled(move |button| {
            if !updating.get() && sidebar_page.is_visible() {
                if button.is_active() {
                    show_sidebar(&split_view, &manually_hidden);
                } else {
                    hide_sidebar(&split_view, &manually_hidden);
                }
                // Persist only real user toggles — never the responsive
                // (width-driven) collapse, which does not go through here.
                let saved = {
                    let conn = &conn;
                    reprise_core::library::settings::set_sidebar_collapsed(
                        conn,
                        !button.is_active(),
                    )
                };
                if let Err(error) = saved {
                    tracing::warn!(%error, "could not save sidebar collapse state");
                }
            }
        });
    }
    {
        let sidebar_toggle = sidebar_toggle.clone();
        let sidebar_page = sidebar_page.clone();
        let updating = updating.clone();
        split_view.connect_show_sidebar_notify(move |split_view| {
            sync_sidebar_toggle(&sidebar_toggle, split_view, &sidebar_page, &updating);
        });
    }
    {
        let sidebar_toggle = sidebar_toggle.clone();
        let sidebar_page = sidebar_page.clone();
        let updating = updating.clone();
        split_view.connect_collapsed_notify(move |split_view| {
            // Visibility is owned by the user toggle and
            // responsive_side_panels. A collapse transition only changes
            // pinned-vs-overlay presentation, so this handler merely mirrors
            // the already-set visibility and cannot emit show-sidebar again.
            sync_sidebar_toggle(&sidebar_toggle, split_view, &sidebar_page, &updating);
        });
    }
    {
        let sidebar_toggle = sidebar_toggle.clone();
        let split_view = split_view.clone();
        let manually_hidden = manually_hidden.clone();
        let updating = updating.clone();
        sidebar_page.connect_visible_notify(move |sidebar_page| {
            if !sidebar_page.is_visible() {
                manually_hidden.set(false);
                split_view.set_show_sidebar(false);
            } else if !manually_hidden.get() {
                split_view.set_show_sidebar(!split_view.is_collapsed());
            }
            sync_sidebar_toggle(&sidebar_toggle, &split_view, sidebar_page, &updating);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Rc<Db> {
        Rc::new(crate::test_db::open().unwrap())
    }

    #[test]
    fn sidebar_toggle_remains_available_whenever_the_sidebar_slot_exists() {
        assert!(sidebar_toggle_is_visible(true));
        assert!(!sidebar_toggle_is_visible(false));
    }

    #[test]
    fn sidebar_pointer_activation_preserves_content_focus() {
        assert!(!sidebar_toggle_focus_on_click());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn sidebar_toggle_is_marked_as_a_global_space_target() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let sidebar = adw::NavigationPage::builder()
            .title("Sidebar")
            .child(&gtk4::Label::new(Some("Sidebar")))
            .build();
        let split = adw::OverlaySplitView::builder()
            .sidebar(&sidebar)
            .content(&gtk4::Label::new(Some("Content")))
            .collapsed(false)
            .show_sidebar(true)
            .build();
        let toggle = gtk4::ToggleButton::new();

        wire_sidebar_toggle(&toggle, &split, &sidebar, &test_conn());

        assert!(
            !toggle.gets_focus_on_click(),
            "a pointer-clicked sidebar toggle must not own the next global Space shortcut"
        );
        assert!(toggle.has_css_class(crate::ui::shortcuts::SIDEBAR_TOGGLE_CSS_CLASS));
    }

    #[test]
    fn doc_6b_sidebar_activation_routes_to_library_while_the_job_keeps_running() {
        let showed_library = std::cell::Cell::new(false);
        let hid_overlay = std::cell::Cell::new(false);

        activate_sidebar_route(false, || showed_library.set(true), || hid_overlay.set(true));

        assert!(showed_library.get());
        assert!(!hid_overlay.get(), "wide navigation keeps the sidebar open");
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn doc_6b_sidebar_navigation_leaves_the_running_summary_visible_in_the_background() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let sidebar = adw::NavigationPage::builder()
            .title("Sidebar")
            .child(&gtk4::Label::new(Some("Sidebar")))
            .build();
        let library = adw::NavigationPage::builder()
            .title("Library")
            .tag(super::super::now_playing_wiring::LIBRARY_CONTENT_TAG)
            .child(&gtk4::Label::new(Some("Library")))
            .build();
        let doctor = adw::NavigationPage::builder()
            .title("Library Doctor")
            .tag("library-doctor")
            .child(&gtk4::Label::new(Some("Running")))
            .build();
        let navigation = adw::NavigationView::new();
        navigation.add(&library);
        navigation.push(&doctor);
        let split = adw::OverlaySplitView::builder()
            .sidebar(&sidebar)
            .content(&navigation)
            .collapsed(false)
            .show_sidebar(true)
            .build();

        show_content_callback(&split, &navigation)();

        assert_eq!(navigation.visible_page().as_ref(), Some(&library));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn opening_a_device_from_a_pushed_page_shows_the_device_page() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let device_root = tempfile::tempdir().unwrap();
        let backend = Rc::new(
            crate::ui::device_sync_smoke::SimulatedMtpDeviceBackend::for_root(device_root.path())
                .unwrap(),
        );
        let runtime = super::super::device_sync_runtime::DeviceSyncRuntime::with_backend(
            &test_conn(),
            backend,
        );
        let content_stack = gtk4::Stack::new();
        content_stack.add_named(&gtk4::Label::new(Some("Library")), Some("library"));
        let library = adw::NavigationPage::builder()
            .title("Library")
            .tag(super::super::now_playing_wiring::LIBRARY_CONTENT_TAG)
            .child(&content_stack)
            .build();
        let pushed = adw::NavigationPage::builder()
            .title("Pushed page")
            .child(&gtk4::Label::new(Some("Pushed page")))
            .build();
        let content_navigation = adw::NavigationView::new();
        content_navigation.add(&library);
        content_navigation.push(&pushed);
        let sidebar = adw::NavigationPage::builder()
            .title("Sidebar")
            .child(&gtk4::Label::new(Some("Sidebar")))
            .build();
        let split = adw::OverlaySplitView::builder()
            .sidebar(&sidebar)
            .content(&content_navigation)
            .build();
        let title = adw::WindowTitle::new("Library", "");

        assert!(open_device_place(
            &content_navigation,
            &content_stack,
            &title,
            crate::ui::device_sync_smoke::DEVICE_ID,
            &runtime,
            &split,
        ));

        assert_eq!(content_navigation.visible_page().as_ref(), Some(&library));
        assert_eq!(
            content_stack.visible_child_name().as_deref(),
            Some("device-sync")
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn switching_a_content_page_from_a_pushed_page_reveals_the_switch() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let content_stack = gtk4::Stack::new();
        content_stack.add_named(&gtk4::Label::new(Some("Library")), Some("library"));
        content_stack.add_named(&gtk4::Label::new(Some("Stats")), Some("stats"));
        let library = adw::NavigationPage::builder()
            .title("Library")
            .tag(super::super::now_playing_wiring::LIBRARY_CONTENT_TAG)
            .child(&content_stack)
            .build();
        let pushed = adw::NavigationPage::builder()
            .title("Pushed page")
            .child(&gtk4::Label::new(Some("Pushed page")))
            .build();
        let content_navigation = adw::NavigationView::new();
        content_navigation.add(&library);
        content_navigation.push(&pushed);

        show_content_page(&content_navigation, &content_stack, "stats");

        assert_eq!(content_navigation.visible_page().as_ref(), Some(&library));
        assert_eq!(content_stack.visible_child_name().as_deref(), Some("stats"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn mot_3_left_sidebar_matches_the_info_panel_and_roundtrips_at_the_breakpoint() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let sidebar = adw::NavigationPage::builder()
            .title("Sidebar")
            .child(&gtk4::Label::new(Some("Sidebar")))
            .build();
        let content = gtk4::Label::new(Some("Content"));
        let content_page = adw::NavigationPage::builder()
            .title("Library")
            .tag(super::super::now_playing_wiring::LIBRARY_CONTENT_TAG)
            .child(&content)
            .build();
        let content_navigation = adw::NavigationView::new();
        content_navigation.add(&content_page);
        let split = adw::OverlaySplitView::builder()
            .sidebar(&sidebar)
            .content(&content_navigation)
            .sidebar_position(gtk4::PackType::Start)
            .collapsed(false)
            .show_sidebar(true)
            .pin_sidebar(true)
            .build();
        let info_sidebar = adw::ToolbarView::new();
        let info_content = gtk4::Label::new(Some("Info content"));
        let information = crate::ui::now_playing::now_playing_column::NowPlayingColumn::new(
            &info_content,
            &info_sidebar,
            true,
        );
        let button = gtk4::ToggleButton::builder()
            .icon_name("sidebar-show-symbolic")
            .build();
        let window = adw::Window::builder()
            .default_width(900)
            .default_height(600)
            .content(&split)
            .build();
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}
        let conn = test_conn();
        wire_sidebar_toggle(&button, &split, &sidebar, &conn);

        assert_eq!(split.type_(), information.widget().type_());
        assert_eq!(split.sidebar_position(), gtk4::PackType::Start);
        assert_eq!(information.widget().sidebar_position(), gtk4::PackType::End);
        assert!(!split.is_collapsed());
        assert!(split.shows_sidebar());
        assert!(button.is_active());

        button.set_active(false);
        assert!(!split.shows_sidebar());
        split.set_collapsed(true);
        split.set_collapsed(false);
        assert!(!split.shows_sidebar(), "manual hiding survives resizing");

        button.set_active(true);
        assert!(split.shows_sidebar());
        split.set_collapsed(true);
        assert!(
            split.shows_sidebar(),
            "the narrow breakpoint changes presentation, not visibility"
        );

        show_content_callback(&split, &content_navigation)();
        assert!(!split.shows_sidebar());

        button.set_active(true);
        assert!(
            split.shows_sidebar(),
            "the narrow toggle opens the sidebar overlay"
        );
        button.set_active(false);
        assert!(
            !split.shows_sidebar(),
            "the narrow toggle dismisses the sidebar overlay"
        );

        split.set_collapsed(false);
        assert!(
            !split.shows_sidebar(),
            "the breakpoint cannot undo a navigation-driven close"
        );
        window.close();
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn wide_window_toggle_collapses_and_restores_the_sidebar_column() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let sidebar = adw::NavigationPage::builder()
            .title("Sidebar")
            .child(&gtk4::Label::new(Some("Sidebar")))
            .build();
        let content = gtk4::Label::new(Some("Content"));
        let split = adw::OverlaySplitView::builder()
            .sidebar(&sidebar)
            .content(&content)
            .sidebar_position(gtk4::PackType::Start)
            .show_sidebar(true)
            .collapsed(false)
            .build();
        let button = gtk4::ToggleButton::builder()
            .icon_name("sidebar-show-symbolic")
            .build();
        let window = adw::Window::builder()
            .default_width(900)
            .default_height(600)
            .content(&split)
            .build();
        window.set_size_request(900, 600);
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}
        let conn = test_conn();
        wire_sidebar_toggle(&button, &split, &sidebar, &conn);

        assert!(button.is_visible());
        assert!(button.is_active());
        assert!(!split.is_collapsed());
        assert!(split.shows_sidebar());

        button.set_active(false);
        assert!(!split.shows_sidebar());
        assert!(button.is_visible());

        split.set_collapsed(true);
        split.set_collapsed(false);
        assert!(!split.shows_sidebar());

        button.set_active(true);
        assert!(!split.is_collapsed());
        assert!(split.shows_sidebar());
        assert!(button.is_active());
        window.close();
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn sidebar_visibility_removes_and_restores_the_complete_split_view_slot() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let list = gtk4::ListBox::new();
        list.append(&gtk4::Label::new(Some("Music")));
        let sidebar = adw::NavigationPage::builder()
            .title("Sidebar")
            .child(&list)
            .build();
        let content = gtk4::Label::new(Some("Content"));
        let split = adw::OverlaySplitView::builder()
            .sidebar(&sidebar)
            .content(&content)
            .sidebar_position(gtk4::PackType::Start)
            .show_sidebar(true)
            .collapsed(false)
            .build();
        let window = adw::Window::builder()
            .default_width(900)
            .default_height(600)
            .content(&split)
            .build();
        window.set_size_request(900, 600);
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert!(list.grab_focus());

        apply_sidebar_visibility(&split, &sidebar, false);
        assert!(split.sidebar().is_some());
        assert!(!split.shows_sidebar());
        assert!(!sidebar.is_visible());
        let focus = gtk4::prelude::GtkWindowExt::focus(&window);
        assert!(focus.is_none_or(|focus| !focus.is_ancestor(&sidebar)));

        apply_sidebar_visibility(&split, &sidebar, true);
        assert_eq!(
            split.sidebar().as_ref(),
            Some(sidebar.upcast_ref::<gtk4::Widget>())
        );
        assert!(sidebar.is_visible());
        assert!(!split.is_collapsed());
        assert!(split.shows_sidebar());
        window.close();
    }
}
