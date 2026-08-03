//! Cached near/far cover shadows whose opacities cross-fade on a bass hit.
//!
//! The blur geometry is CSS-static: changing a `box-shadow` blur every frame
//! invalidates GTK's cached shadow node, while changing opacity keeps both
//! nodes reusable.

use gtk4::prelude::*;

use crate::ui::{motion, style::tokens::RADIUS_SURFACE};

const LIFT_Y_REST: f64 = 0.048;
const LIFT_Y_PER_KICK: f64 = 0.048;
const LIFT_BLUR_REST: f64 = 0.14;
const LIFT_BLUR_PER_KICK: f64 = 0.155;
const LIFT_SPREAD_PER_KICK: f64 = -0.018;
const LIFT_COLOUR: &str = "rgba(0, 0, 0, 0.55)";

const PANEL_WIDTH: i32 = 168;
const BAR_WIDTH: i32 = 56;
const SHADOW_BASE_CLASS: &str = "reprise-cover-lift-shadow";
const PANEL_NEAR_CLASS: &str = "reprise-cover-lift-panel-near";
const PANEL_FAR_CLASS: &str = "reprise-cover-lift-panel-far";
const BAR_NEAR_CLASS: &str = "reprise-cover-lift-bar-near";
const BAR_FAR_CLASS: &str = "reprise-cover-lift-bar-far";

pub(in crate::ui) fn lift_shadow(width: f64, kick: f64) -> String {
    let kick = kick.clamp(0.0, 1.0);
    let y = ((LIFT_Y_REST + LIFT_Y_PER_KICK * kick) * width).round() as i64;
    let blur = ((LIFT_BLUR_REST + LIFT_BLUR_PER_KICK * kick) * width).round() as i64;
    let spread = (LIFT_SPREAD_PER_KICK * kick * width).round() as i64;
    let spread = if spread == 0 {
        "0".to_string()
    } else {
        format!("{spread}px")
    };
    format!("0 {y}px {blur}px {spread} {LIFT_COLOUR}")
}

pub(in crate::ui) fn near_opacity(kick: f64) -> f64 {
    1.0 - kick.clamp(0.0, 1.0)
}

pub(in crate::ui) fn far_opacity(kick: f64) -> f64 {
    kick.clamp(0.0, 1.0)
}

pub(in crate::ui) fn css() -> String {
    format!(
        ".{SHADOW_BASE_CLASS} {{ border-radius: {RADIUS_SURFACE}; }}\n\
         .{PANEL_NEAR_CLASS} {{ box-shadow: {}; }}\n\
         .{PANEL_FAR_CLASS} {{ box-shadow: {}; }}\n\
         .{BAR_NEAR_CLASS} {{ box-shadow: {}; }}\n\
         .{BAR_FAR_CLASS} {{ box-shadow: {}; }}",
        lift_shadow(f64::from(PANEL_WIDTH), 0.0),
        lift_shadow(f64::from(PANEL_WIDTH), 1.0),
        lift_shadow(f64::from(BAR_WIDTH), 0.0),
        lift_shadow(f64::from(BAR_WIDTH), 1.0),
    )
}

#[derive(Clone)]
pub(in crate::ui) struct CoverLift {
    root: gtk4::Overlay,
    near: gtk4::Box,
    far: gtk4::Box,
}

impl CoverLift {
    pub(in crate::ui) fn new(cover: &impl IsA<gtk4::Widget>, width: i32) -> Self {
        let (near_class, far_class) = match width {
            PANEL_WIDTH => (PANEL_NEAR_CLASS, PANEL_FAR_CLASS),
            BAR_WIDTH => (BAR_NEAR_CLASS, BAR_FAR_CLASS),
            _ => panic!("cover lift has no static shadow pair for {width}px"),
        };
        let near = shadow_layer(width, near_class, 1.0);
        let far = shadow_layer(width, far_class, 0.0);
        let shadows = gtk4::Overlay::new();
        shadows.set_can_target(false);
        shadows.set_child(Some(&near));
        shadows.add_overlay(&far);

        let root = gtk4::Overlay::new();
        root.set_size_request(width, width);
        root.set_child(Some(&shadows));
        root.add_overlay(cover);
        Self { root, near, far }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Overlay {
        &self.root
    }

    pub(in crate::ui) fn set_kick(&self, kick: f64) {
        let kick = motion::reactive_amplitude(kick);
        self.near.set_opacity(near_opacity(kick));
        self.far.set_opacity(far_opacity(kick));
    }
}

fn shadow_layer(width: i32, class: &str, opacity: f64) -> gtk4::Box {
    let layer = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    layer.set_size_request(width, width);
    layer.set_can_target(false);
    layer.set_can_focus(false);
    layer.set_opacity(opacity);
    layer.add_css_class(SHADOW_BASE_CLASS);
    layer.add_css_class(class);
    layer
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ac_24_the_lift_is_two_static_shadows_not_an_animated_blur() {
        // A changing blur radius throws away the cached shadow node every
        // frame. Two fixed shadows whose opacities cross-fade look the same
        // and cost two alpha writes.
        let css = css();
        assert!(css.contains("0 8px 24px"), "the resting shadow is missing");
        assert!(
            css.contains("0 16px 50px -3px"),
            "the lifted shadow is missing"
        );
        // Opacity is the only thing that may move.
        assert!(!css.contains("transition: box-shadow"));
    }

    #[test]
    fn ac_24_the_lift_crossfades_and_never_brightens() {
        assert!((near_opacity(0.0) - 1.0).abs() < 1e-9);
        assert!((near_opacity(1.0) - 0.0).abs() < 1e-9);
        assert!((far_opacity(0.0) - 0.0).abs() < 1e-9);
        assert!((far_opacity(1.0) - 1.0).abs() < 1e-9);
        // The pair always sums to one, so the total shadow weight is constant
        // and only its shape changes.
        for step in 0..=10 {
            let kick = f64::from(step) / 10.0;
            assert!((near_opacity(kick) + far_opacity(kick) - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn ac_24_the_lift_geometry_scales_with_the_cover() {
        // 168 px panel cover and 56 px bar thumbnail from one set of ratios.
        assert_eq!(lift_shadow(168.0, 0.0), "0 8px 24px 0 rgba(0, 0, 0, 0.55)");
        assert_eq!(
            lift_shadow(168.0, 1.0),
            "0 16px 50px -3px rgba(0, 0, 0, 0.55)"
        );
        assert_eq!(lift_shadow(56.0, 0.0), "0 3px 8px 0 rgba(0, 0, 0, 0.55)");
    }
}
