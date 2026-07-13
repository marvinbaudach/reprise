use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;

pub(super) fn apply_sidebar_visibility(
    split_view: &adw::NavigationSplitView,
    sidebar_page: &adw::NavigationPage,
    visible: bool,
) {
    if visible {
        if split_view.sidebar().is_none() {
            split_view.set_sidebar(Some(sidebar_page));
        }
    } else {
        split_view.set_show_content(true);
        if split_view.sidebar().is_some() {
            split_view.set_sidebar(adw::NavigationPage::NONE);
        }
    }
}

fn sidebar_toggle_is_visible(collapsed: bool, has_sidebar: bool) -> bool {
    collapsed && has_sidebar
}

fn sync_sidebar_toggle(
    sidebar_toggle: &gtk4::ToggleButton,
    split_view: &adw::NavigationSplitView,
    updating: &std::cell::Cell<bool>,
) {
    updating.set(true);
    let has_sidebar = split_view.sidebar().is_some();
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
) {
    let updating = Rc::new(std::cell::Cell::new(false));
    sync_sidebar_toggle(sidebar_toggle, split_view, &updating);
    {
        let split_view = split_view.clone();
        let updating = updating.clone();
        sidebar_toggle.connect_toggled(move |button| {
            if !updating.get() && split_view.sidebar().is_some() {
                split_view.set_show_content(!button.is_active());
            }
        });
    }
    {
        let sidebar_toggle = sidebar_toggle.clone();
        let updating = updating.clone();
        split_view.connect_show_content_notify(move |split_view| {
            sync_sidebar_toggle(&sidebar_toggle, split_view, &updating);
        });
    }
    {
        let sidebar_toggle = sidebar_toggle.clone();
        let updating = updating.clone();
        split_view.connect_collapsed_notify(move |split_view| {
            sync_sidebar_toggle(&sidebar_toggle, split_view, &updating);
        });
    }
    {
        let sidebar_toggle = sidebar_toggle.clone();
        let updating = updating.clone();
        split_view.connect_sidebar_notify(move |split_view| {
            sync_sidebar_toggle(&sidebar_toggle, split_view, &updating);
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

        apply_sidebar_visibility(&split, &sidebar, false);
        assert!(split.sidebar().is_none());
        assert!(split.shows_content());

        apply_sidebar_visibility(&split, &sidebar, true);
        assert_eq!(split.sidebar().as_ref(), Some(&sidebar));
    }
}
