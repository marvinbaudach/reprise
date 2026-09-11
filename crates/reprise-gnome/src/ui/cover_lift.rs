//! Cached near/far player-bar cover shadows driven by the slow swell.
//!
//! The blur geometry is CSS-static: changing a `box-shadow` blur every frame
//! invalidates GTK's cached shadow node, while changing opacity keeps both
//! nodes reusable.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;

const LIFT_Y_REST: f64 = 0.048;
const LIFT_Y_PER_SWELL: f64 = 0.048;
const LIFT_BLUR_REST: f64 = 0.14;
const LIFT_BLUR_PER_SWELL: f64 = 0.155;
const LIFT_SPREAD_PER_SWELL: f64 = -0.018;
const LIFT_COLOUR: &str = "rgba(0, 0, 0, 0.55)";

const BAR_WIDTH: i32 = 56;
const SHADOW_BASE_CLASS: &str = "reprise-cover-lift-shadow";
const BAR_NEAR_CLASS: &str = "reprise-cover-lift-bar-near";
const BAR_FAR_CLASS: &str = "reprise-cover-lift-bar-far";

pub(in crate::ui) fn lift_shadow(width: f64, swell: f64) -> String {
    let swell = swell.clamp(0.0, 1.0);
    let y = ((LIFT_Y_REST + LIFT_Y_PER_SWELL * swell) * width).round() as i64;
    let blur = ((LIFT_BLUR_REST + LIFT_BLUR_PER_SWELL * swell) * width).round() as i64;
    let spread = (LIFT_SPREAD_PER_SWELL * swell * width).round() as i64;
    let spread = if spread == 0 {
        "0".to_string()
    } else {
        format!("{spread}px")
    };
    format!("0 {y}px {blur}px {spread} {LIFT_COLOUR}")
}

/// Alpha of a single shadow layer, mirrored from [`LIFT_COLOUR`]. The
/// cross-fade has to know it, because two translucent blacks do not add
/// linearly.
const SHADOW_ALPHA: f64 = 0.55;

/// Opacity of the resting layer, compensated so the *composite* coverage of
/// the two layers stays constant while their shape morphs.
///
/// A plain `1.0 - swell` looks right on paper and flickers on screen: at
/// `swell = 0.5` both layers sit at 0.275, and `1 - (1-0.275)²` is 0.474
/// against 0.550 at either end — a 14 % brightening in the middle of every
/// hit, then back. Solving `1 - (1 - a·near)(1 - a·far) = a` for `near`
/// removes it exactly.
pub(in crate::ui) fn near_opacity(swell: f64) -> f64 {
    let far = SHADOW_ALPHA * swell.clamp(0.0, 1.0);
    if far >= 1.0 {
        return 0.0;
    }
    ((1.0 - (1.0 - SHADOW_ALPHA) / (1.0 - far)) / SHADOW_ALPHA).clamp(0.0, 1.0)
}

pub(in crate::ui) fn far_opacity(swell: f64) -> f64 {
    swell.clamp(0.0, 1.0)
}

/// Composite coverage of both layers — the quantity that must not move.
#[cfg(test)]
fn composite_coverage(swell: f64) -> f64 {
    1.0 - (1.0 - SHADOW_ALPHA * near_opacity(swell)) * (1.0 - SHADOW_ALPHA * far_opacity(swell))
}

pub(in crate::ui) fn css() -> String {
    format!(
        ".{SHADOW_BASE_CLASS} {{ border-radius: {}; }}\n\
         .{BAR_NEAR_CLASS} {{ box-shadow: {}; }}\n\
         .{BAR_FAR_CLASS} {{ box-shadow: {}; }}",
        crate::ui::style::tokens::RADIUS_SURFACE,
        lift_shadow(f64::from(BAR_WIDTH), 0.0),
        lift_shadow(f64::from(BAR_WIDTH), 1.0),
    )
}

#[derive(Clone)]
struct CoverLiftWidgets {
    near: gtk4::Box,
    far: gtk4::Box,
}

#[derive(Clone)]
pub(in crate::ui) struct CoverLift {
    root: Option<gtk4::Overlay>,
    widgets: Option<CoverLiftWidgets>,
    swell: Rc<Cell<f64>>,
}

impl CoverLift {
    pub(in crate::ui) fn new(cover: &impl IsA<gtk4::Widget>, width: i32) -> Self {
        assert_eq!(width, BAR_WIDTH, "cover lift only serves the player bar");
        let near = shadow_layer(width, BAR_NEAR_CLASS, 1.0);
        let far = shadow_layer(width, BAR_FAR_CLASS, 0.0);
        let shadows = gtk4::Overlay::new();
        shadows.set_can_target(false);
        shadows.set_halign(gtk4::Align::Center);
        shadows.set_valign(gtk4::Align::Center);
        shadows.set_child(Some(&near));
        shadows.add_overlay(&far);

        let root = gtk4::Overlay::new();
        root.set_size_request(width, width);
        root.set_child(Some(&shadows));
        cover.set_halign(gtk4::Align::Center);
        cover.set_valign(gtk4::Align::Center);
        root.add_overlay(cover);
        let widgets = CoverLiftWidgets { near, far };
        Self {
            root: Some(root),
            widgets: Some(widgets),
            swell: Rc::new(Cell::new(0.0)),
        }
    }

    /// Wraps a cover without installing the reactive edge and shadow layers.
    ///
    /// The settled Now Playing head owns its one static shadow in panel CSS;
    /// the player bar remains the sole consumer of the animated lift.
    pub(in crate::ui) fn new_still(cover: &impl IsA<gtk4::Widget>, width: i32) -> Self {
        let root = gtk4::Overlay::new();
        root.set_size_request(width, width);
        cover.set_halign(gtk4::Align::Center);
        cover.set_valign(gtk4::Align::Center);
        root.set_child(Some(cover));
        Self {
            root: Some(root),
            widgets: None,
            swell: Rc::new(Cell::new(0.0)),
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Overlay {
        self.root
            .as_ref()
            .expect("production cover lift has a root widget")
    }

    /// The player bar's entry point: it derives no pressure of its own and its
    /// small cover carries no edge seam, so the bed stays where it was.
    pub(in crate::ui) fn set_swell(&self, swell: f64) {
        self.swell.set(swell.clamp(0.0, 1.0));
        self.apply_reading();
    }

    fn apply_reading(&self) {
        if let Some(widgets) = &self.widgets {
            apply_reading_to_widgets(widgets, self.swell.get());
        }
    }
}

fn apply_reading_to_widgets(widgets: &CoverLiftWidgets, swell: f64) {
    widgets.near.set_opacity(near_opacity(swell));
    widgets.far.set_opacity(far_opacity(swell));
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
    fn ac_24_the_player_bar_is_the_only_reactive_cover_lift() {
        let css = css();
        assert!(css.contains(BAR_NEAR_CLASS));
        assert!(css.contains(BAR_FAR_CLASS));
        assert!(!css.contains("reprise-cover-lift-panel-near"));
        assert!(!css.contains("reprise-cover-lift-panel-far"));
        assert!(!css.contains("reprise-cover-edge-light"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn ac_24_the_player_bar_lift_matches_the_live_cover_size() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let cover = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        cover.set_size_request(BAR_WIDTH, BAR_WIDTH);
        let lift = CoverLift::new(&cover, BAR_WIDTH);
        let window = gtk4::Window::builder().child(lift.widget()).build();
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        let widgets = lift.widgets.as_ref().expect("the player bar has a lift");
        let cover_bounds = cover
            .compute_bounds(lift.widget())
            .expect("cover and lift share a coordinate space");
        let near_bounds = widgets
            .near
            .compute_bounds(lift.widget())
            .expect("near shadow and lift share a coordinate space");
        let far_bounds = widgets
            .far
            .compute_bounds(lift.widget())
            .expect("far shadow and lift share a coordinate space");
        assert_eq!(cover_bounds.width(), BAR_WIDTH as f32);
        assert_eq!(cover_bounds.height(), BAR_WIDTH as f32);
        assert_eq!(near_bounds, cover_bounds);
        assert_eq!(far_bounds, cover_bounds);
        window.close();
    }

    #[test]
    fn ac_24_the_lift_is_two_static_shadows_not_an_animated_blur() {
        // A changing blur radius throws away the cached shadow node every
        // frame. Two fixed shadows whose opacities cross-fade look the same
        // and cost two alpha writes.
        let css = css();
        assert!(css.contains("0 3px 8px"), "the resting shadow is missing");
        assert!(
            css.contains("0 5px 17px -1px"),
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
        // What must stay constant is the COMPOSITE coverage, not the sum of
        // the two opacities. Two translucent blacks do not add linearly: a
        // plain `1 - swell` pair sums to one and still dips to 0.474 against
        // 0.550 in the middle, which reads as a bright/dark flicker on every
        // hit. Assert the thing the eye actually sees.
        for step in 0..=100 {
            let swell = f64::from(step) / 100.0;
            assert!(
                (composite_coverage(swell) - SHADOW_ALPHA).abs() < 1e-9,
                "the shadow changes weight at swell {swell}: {}",
                composite_coverage(swell)
            );
        }
    }

    #[test]
    fn ac_24_the_lift_geometry_scales_with_the_cover() {
        // The 56 px player-bar thumbnail keeps the settled lift ratios.
        assert_eq!(lift_shadow(56.0, 0.0), "0 3px 8px 0 rgba(0, 0, 0, 0.55)");
        assert_eq!(
            lift_shadow(56.0, 1.0),
            "0 5px 17px -1px rgba(0, 0, 0, 0.55)"
        );
    }
}
