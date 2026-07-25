//! Concerts full-view composition boundary.

use gtk4::prelude::*;

pub(super) mod css;

/// Compile-safe composition stub filled by the Concerts view tasks.
#[allow(dead_code)]
pub(in crate::ui) struct ConcertsView {
    root: gtk4::Widget,
}

#[allow(dead_code)]
impl ConcertsView {
    pub(in crate::ui) fn root(&self) -> &gtk4::Widget {
        &self.root
    }
}

#[allow(dead_code)]
pub(in crate::ui) fn install() -> ConcertsView {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    root.add_css_class("reprise-concerts-view");
    ConcertsView {
        root: root.upcast(),
    }
}
