use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use rusqlite::Connection;

use super::library_shell::SIDEBAR_BREAKPOINT_WIDTH;

pub(in crate::ui) fn apply_sidebar_visibility(
    split_view: &adw::NavigationSplitView,
    sidebar_page: &adw::NavigationPage,
    visible: bool,
) {
    if visible {
        sidebar_page.set_visible(true);
        let width = split_view
            .root()
            .map_or_else(|| split_view.width(), |root| root.width());
        split_view.set_collapsed(width < SIDEBAR_BREAKPOINT_WIDTH);
    } else {
        clear_sidebar_focus(split_view, sidebar_page);
        split_view.set_show_content(true);
        split_view.set_collapsed(true);
        sidebar_page.set_visible(false);
    }
}

fn clear_sidebar_focus(split_view: &adw::NavigationSplitView, sidebar_page: &adw::NavigationPage) {
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

fn sync_sidebar_toggle(
    sidebar_toggle: &gtk4::ToggleButton,
    split_view: &adw::NavigationSplitView,
    sidebar_page: &adw::NavigationPage,
    manually_hidden: &std::cell::Cell<bool>,
    updating: &std::cell::Cell<bool>,
) {
    updating.set(true);
    let has_sidebar = sidebar_page.is_visible();
    sidebar_toggle.set_visible(sidebar_toggle_is_visible(has_sidebar));
    sidebar_toggle.set_active(
        has_sidebar
            && !manually_hidden.get()
            && (!split_view.is_collapsed() || !split_view.shows_content()),
    );
    updating.set(false);
}

fn available_width(split_view: &adw::NavigationSplitView) -> i32 {
    split_view
        .root()
        .map_or_else(|| split_view.width(), |root| root.width())
}

fn show_sidebar(split_view: &adw::NavigationSplitView, manually_hidden: &std::cell::Cell<bool>) {
    manually_hidden.set(false);
    let collapsed = available_width(split_view) < SIDEBAR_BREAKPOINT_WIDTH;
    split_view.set_collapsed(collapsed);
    if collapsed {
        split_view.set_show_content(false);
    }
}

fn hide_sidebar(split_view: &adw::NavigationSplitView, manually_hidden: &std::cell::Cell<bool>) {
    manually_hidden.set(true);
    split_view.set_show_content(true);
    split_view.set_collapsed(true);
}

pub(in crate::ui) fn show_content_callback(split_view: &adw::NavigationSplitView) -> Rc<dyn Fn()> {
    let split_view = split_view.downgrade();
    Rc::new(move || match split_view.upgrade() {
        Some(split_view) => {
            if split_view.is_collapsed() {
                split_view.set_show_content(true);
            }
        }
        None => {
            tracing::warn!("split view is gone; cannot show content pane after sidebar navigation");
        }
    })
}

pub(in crate::ui) fn wire_sidebar_toggle(
    sidebar_toggle: &gtk4::ToggleButton,
    split_view: &adw::NavigationSplitView,
    sidebar_page: &adw::NavigationPage,
    conn: &Rc<RefCell<Connection>>,
) {
    let updating = Rc::new(std::cell::Cell::new(false));
    let manually_hidden = Rc::new(std::cell::Cell::new(false));
    // Restore last session's manual collapse before the initial toggle sync,
    // so the button starts in the matching state.
    if reprise_core::library::settings::get_sidebar_collapsed(&conn.borrow())
        && sidebar_page.is_visible()
    {
        hide_sidebar(split_view, &manually_hidden);
    }
    sync_sidebar_toggle(
        sidebar_toggle,
        split_view,
        sidebar_page,
        &manually_hidden,
        &updating,
    );
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
        let manually_hidden = manually_hidden.clone();
        let updating = updating.clone();
        split_view.connect_show_content_notify(move |split_view| {
            sync_sidebar_toggle(
                &sidebar_toggle,
                split_view,
                &sidebar_page,
                &manually_hidden,
                &updating,
            );
        });
    }
    {
        let sidebar_toggle = sidebar_toggle.clone();
        let sidebar_page = sidebar_page.clone();
        let manually_hidden = manually_hidden.clone();
        let updating = updating.clone();
        split_view.connect_collapsed_notify(move |split_view| {
            if (!sidebar_page.is_visible() || manually_hidden.get()) && !split_view.is_collapsed() {
                split_view.set_collapsed(true);
                return;
            }
            sync_sidebar_toggle(
                &sidebar_toggle,
                split_view,
                &sidebar_page,
                &manually_hidden,
                &updating,
            );
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
            }
            sync_sidebar_toggle(
                &sidebar_toggle,
                &split_view,
                sidebar_page,
                &manually_hidden,
                &updating,
            );
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
    #[ignore = "requires a display; run via xvfb-run"]
    fn wide_window_toggle_collapses_and_restores_the_sidebar_column() {
        gtk4::init().unwrap();
        let sidebar = adw::NavigationPage::builder()
            .title("Sidebar")
            .child(&gtk4::Label::new(Some("Sidebar")))
            .build();
        let content = adw::NavigationPage::builder()
            .title("Content")
            .child(&gtk4::Label::new(Some("Content")))
            .build();
        let split = adw::NavigationSplitView::builder()
            .sidebar(&sidebar)
            .content(&content)
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
        assert!(available_width(&split) >= SIDEBAR_BREAKPOINT_WIDTH);

        button.set_active(false);
        assert!(split.is_collapsed());
        assert!(split.shows_content());
        assert!(button.is_visible());

        split.set_collapsed(false);
        assert!(split.is_collapsed());

        button.set_active(true);
        assert!(!split.is_collapsed());
        assert!(button.is_active());
        window.close();
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn sidebar_visibility_removes_and_restores_the_complete_split_view_slot() {
        gtk4::init().unwrap();
        let list = gtk4::ListBox::new();
        list.append(&gtk4::Label::new(Some("Music")));
        let sidebar = adw::NavigationPage::builder()
            .title("Sidebar")
            .child(&list)
            .build();
        let content = adw::NavigationPage::builder()
            .title("Content")
            .child(&gtk4::Label::new(Some("Content")))
            .build();
        let split = adw::NavigationSplitView::builder()
            .sidebar(&sidebar)
            .content(&content)
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
        assert!(split.is_collapsed());
        assert!(split.shows_content());
        assert!(!sidebar.is_visible());

        apply_sidebar_visibility(&split, &sidebar, true);
        assert_eq!(split.sidebar().as_ref(), Some(&sidebar));
        assert!(sidebar.is_visible());
        assert!(!split.is_collapsed());
        window.close();
    }
}
