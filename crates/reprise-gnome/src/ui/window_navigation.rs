use std::rc::Rc;

use libadwaita as adw;
use libadwaita::prelude::*;

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
