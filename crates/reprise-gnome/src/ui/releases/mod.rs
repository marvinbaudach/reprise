//! Releases full-view composition boundary.

use gtk4::prelude::*;

pub(super) mod css;

/// Compile-safe composition stub filled by the Releases view tasks.
#[allow(dead_code)]
pub(in crate::ui) struct ReleasesView {
    root: gtk4::Widget,
}

#[allow(dead_code)]
impl ReleasesView {
    pub(in crate::ui) fn root(&self) -> &gtk4::Widget {
        &self.root
    }
}

#[allow(dead_code)]
pub(in crate::ui) fn install() -> ReleasesView {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    root.add_css_class("reprise-releases-view");
    ReleasesView {
        root: root.upcast(),
    }
}
