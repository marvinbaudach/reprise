use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use rusqlite::Connection;

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
                    || {
                        if let Some(root) = content_navigation
                            .find_page(super::now_playing_wiring::LIBRARY_CONTENT_TAG)
                        {
                            content_navigation.pop_to_page(&root);
                        }
                    },
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

pub(in crate::ui) fn wire_sidebar_toggle(
    sidebar_toggle: &gtk4::ToggleButton,
    split_view: &adw::OverlaySplitView,
    sidebar_page: &adw::NavigationPage,
    conn: &Rc<RefCell<Connection>>,
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
    if reprise_core::library::settings::get_sidebar_collapsed(&conn.borrow())
        && sidebar_page.is_visible()
    {
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
                    let conn = conn.borrow();
                    reprise_core::library::settings::set_sidebar_collapsed(
                        &conn,
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
        let manually_hidden = manually_hidden.clone();
        let updating = updating.clone();
        split_view.connect_collapsed_notify(move |split_view| {
            if !sidebar_page.is_visible() || manually_hidden.get() {
                split_view.set_show_sidebar(false);
            } else {
                split_view.set_show_sidebar(!split_view.is_collapsed());
            }
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

    fn test_conn() -> Rc<RefCell<Connection>> {
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        Rc::new(RefCell::new(conn))
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
    fn doc_6b_sidebar_navigation_leaves_a_running_job_page_visible_in_the_background() {
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
            !split.shows_sidebar(),
            "the narrow breakpoint shows content"
        );

        button.set_active(true);
        assert!(split.shows_sidebar(), "the narrow toggle opens the overlay");
        show_content_callback(&split, &content_navigation)();
        assert!(!split.shows_sidebar());

        split.set_collapsed(false);
        assert!(
            split.shows_sidebar(),
            "a responsive hide reopens on the wide breakpoint"
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
