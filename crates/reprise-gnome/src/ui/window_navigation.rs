use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;

use super::library_shell::SIDEBAR_BREAKPOINT_WIDTH;

pub(super) fn apply_sidebar_visibility(
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

fn sidebar_toggle_is_visible(collapsed: bool, has_sidebar: bool) -> bool {
    collapsed && has_sidebar
}

fn sync_sidebar_toggle(
    sidebar_toggle: &gtk4::ToggleButton,
    split_view: &adw::NavigationSplitView,
    sidebar_page: &adw::NavigationPage,
    updating: &std::cell::Cell<bool>,
) {
    updating.set(true);
    let has_sidebar = sidebar_page.is_visible();
    sidebar_toggle.set_visible(sidebar_toggle_is_visible(
        split_view.is_collapsed(),
        has_sidebar,
    ));
    sidebar_toggle.set_active(has_sidebar && !split_view.shows_content());
    updating.set(false);
}

pub(super) fn show_content_callback(split_view: &adw::NavigationSplitView) -> Rc<dyn Fn()> {
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

pub(super) fn wire_sidebar_toggle(
    sidebar_toggle: &gtk4::ToggleButton,
    split_view: &adw::NavigationSplitView,
    sidebar_page: &adw::NavigationPage,
) {
    let updating = Rc::new(std::cell::Cell::new(false));
    sync_sidebar_toggle(sidebar_toggle, split_view, sidebar_page, &updating);
    {
        let split_view = split_view.clone();
        let sidebar_page = sidebar_page.clone();
        let updating = updating.clone();
        sidebar_toggle.connect_toggled(move |button| {
            if !updating.get() && sidebar_page.is_visible() {
                split_view.set_show_content(!button.is_active());
            }
        });
    }
    {
        let sidebar_toggle = sidebar_toggle.clone();
        let sidebar_page = sidebar_page.clone();
        let updating = updating.clone();
        split_view.connect_show_content_notify(move |split_view| {
            sync_sidebar_toggle(&sidebar_toggle, split_view, &sidebar_page, &updating);
        });
    }
    {
        let sidebar_toggle = sidebar_toggle.clone();
        let sidebar_page = sidebar_page.clone();
        let updating = updating.clone();
        split_view.connect_collapsed_notify(move |split_view| {
            if !sidebar_page.is_visible() && !split_view.is_collapsed() {
                split_view.set_collapsed(true);
                return;
            }
            sync_sidebar_toggle(&sidebar_toggle, split_view, &sidebar_page, &updating);
        });
    }
    {
        let sidebar_toggle = sidebar_toggle.clone();
        let split_view = split_view.clone();
        let updating = updating.clone();
        sidebar_page.connect_visible_notify(move |sidebar_page| {
            sync_sidebar_toggle(&sidebar_toggle, &split_view, sidebar_page, &updating);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidebar_toggle_requires_a_collapsed_view_with_a_sidebar_slot() {
        assert!(sidebar_toggle_is_visible(true, true));
        assert!(!sidebar_toggle_is_visible(false, true));
        assert!(!sidebar_toggle_is_visible(true, false));
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
