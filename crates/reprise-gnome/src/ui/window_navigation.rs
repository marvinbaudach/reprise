use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;

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
    sidebar_toggle.set_visible(split_view.is_collapsed());
    let updating = Rc::new(std::cell::Cell::new(false));
    {
        let split_view = split_view.clone();
        let updating = updating.clone();
        sidebar_toggle.connect_toggled(move |button| {
            if !updating.get() {
                split_view.set_show_content(!button.is_active());
            }
        });
    }
    {
        let sidebar_toggle = sidebar_toggle.clone();
        let updating = updating.clone();
        split_view.connect_show_content_notify(move |split_view| {
            updating.set(true);
            sidebar_toggle.set_active(!split_view.shows_content());
            updating.set(false);
        });
    }
    {
        let sidebar_toggle = sidebar_toggle.clone();
        split_view.connect_collapsed_notify(move |split_view| {
            sidebar_toggle.set_visible(split_view.is_collapsed());
            sidebar_toggle.set_active(false);
        });
    }
}
