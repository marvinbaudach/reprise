//! Reusable chrome surface with backdrop, hairline, and controls.

use gtk4::prelude::*;

use super::backdrop::GlassBackdrop;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GlassEdge {
    Top,
    Bottom,
}

impl GlassEdge {
    pub(crate) fn css_class(self) -> &'static str {
        match self {
            Self::Top => "reprise-glass-edge-top",
            Self::Bottom => "reprise-glass-edge-bottom",
        }
    }

    fn alignment(self) -> gtk4::Align {
        match self {
            Self::Top => gtk4::Align::Start,
            Self::Bottom => gtk4::Align::End,
        }
    }
}

pub(crate) struct GlassSurface {
    root: gtk4::Overlay,
    backdrop: GlassBackdrop,
}

impl GlassSurface {
    pub(crate) fn new(
        source: &impl IsA<gtk4::Widget>,
        controls: &impl IsA<gtk4::Widget>,
        edge: GlassEdge,
    ) -> Self {
        let backdrop = GlassBackdrop::new(source);
        let root = gtk4::Overlay::new();
        root.add_css_class("reprise-glass-surface");
        root.add_css_class(edge.css_class());
        root.set_child(Some(&backdrop));

        let hairline = gtk4::Separator::new(gtk4::Orientation::Horizontal);
        hairline.set_valign(edge.alignment());
        hairline.set_can_target(false);
        hairline.add_css_class("reprise-glass-hairline");
        root.add_overlay(&hairline);

        controls.add_css_class("reprise-glass-controls");
        root.add_overlay(controls);
        root.set_measure_overlay(controls, true);
        Self { root, backdrop }
    }

    pub(crate) fn root(&self) -> &gtk4::Overlay {
        &self.root
    }

    pub(crate) fn backdrop(&self) -> &GlassBackdrop {
        &self.backdrop
    }
}
