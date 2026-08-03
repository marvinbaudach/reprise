//! Cached near/far cover shadows and the one-pixel edge seam, both driven by
//! the slow swell.
//!
//! The blur geometry is CSS-static: changing a `box-shadow` blur every frame
//! invalidates GTK's cached shadow node, while changing opacity keeps both
//! nodes reusable.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;

use crate::ui::style::tokens::RADIUS_SURFACE;

const LIFT_Y_REST: f64 = 0.048;
const LIFT_Y_PER_SWELL: f64 = 0.048;
const LIFT_BLUR_REST: f64 = 0.14;
const LIFT_BLUR_PER_SWELL: f64 = 0.155;
const LIFT_SPREAD_PER_SWELL: f64 = -0.018;
const LIFT_COLOUR: &str = "rgba(0, 0, 0, 0.55)";

/// The mockup's `opacity: calc(0.18 + var(--pres) * 0.10 + var(--sw) * 0.22)`.
const EDGE_REST_OPACITY: f64 = 0.18;
const EDGE_OPACITY_PER_PRESSURE: f64 = 0.10;
const EDGE_OPACITY_PER_SWELL: f64 = 0.22;
/// The seam sits one pixel outside the cover, so it is two pixels wider and
/// its radius is the cover's plus one.
const EDGE_OUTSET_PX: i32 = 1;
const EDGE_CLASS: &str = "reprise-cover-edge-light";

const PANEL_WIDTH: i32 = 168;
const BAR_WIDTH: i32 = 56;
const SHADOW_BASE_CLASS: &str = "reprise-cover-lift-shadow";
const PANEL_NEAR_CLASS: &str = "reprise-cover-lift-panel-near";
const PANEL_FAR_CLASS: &str = "reprise-cover-lift-panel-far";
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

pub(in crate::ui) fn edge_opacity(pressure: f64, swell: f64) -> f64 {
    EDGE_REST_OPACITY
        + EDGE_OPACITY_PER_PRESSURE * pressure.clamp(0.0, 1.0)
        + EDGE_OPACITY_PER_SWELL * swell.clamp(0.0, 1.0)
}

pub(in crate::ui) fn css() -> String {
    let edge_radius = RADIUS_SURFACE
        .strip_suffix("px")
        .and_then(|radius| radius.parse::<i32>().ok())
        .expect("surface radius must be an integer pixel value")
        + EDGE_OUTSET_PX;
    format!(
        ".{SHADOW_BASE_CLASS} {{ border-radius: {RADIUS_SURFACE}; }}\n\
         .{PANEL_NEAR_CLASS} {{ box-shadow: {}; }}\n\
         .{PANEL_FAR_CLASS} {{ box-shadow: {}; }}\n\
         .{BAR_NEAR_CLASS} {{ box-shadow: {}; }}\n\
         .{BAR_FAR_CLASS} {{ box-shadow: {}; }}\n\
         .{EDGE_CLASS} {{ border: 1px solid @reprise_cover_light; \
                          border-radius: {edge_radius}px; }}",
        lift_shadow(f64::from(PANEL_WIDTH), 0.0),
        lift_shadow(f64::from(PANEL_WIDTH), 1.0),
        lift_shadow(f64::from(BAR_WIDTH), 0.0),
        lift_shadow(f64::from(BAR_WIDTH), 1.0),
    )
}

/// The two readings every layer here runs on.
///
/// **There is deliberately no `kick`.** The cover used to switch to the beat
/// while the Visualizer view was open — round 5's one exception — and on screen
/// that read as the cover twitching under its own shadow. It was rejected for
/// the same reason the waveform's lens and playhead glow were: a hard transient
/// under a large, still object reads as a defect, not as life. Both readings
/// here move over seconds.
#[derive(Clone, Copy, Debug, Default)]
struct Readings {
    swell: f64,
    pressure: f64,
}

#[derive(Clone)]
struct CoverLiftWidgets {
    root: gtk4::Overlay,
    near: gtk4::Box,
    far: gtk4::Box,
    edge: Option<gtk4::Box>,
}

#[derive(Clone)]
pub(in crate::ui) struct CoverLift {
    widgets: Option<CoverLiftWidgets>,
    readings: Rc<Cell<Readings>>,
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
        shadows.set_halign(gtk4::Align::Center);
        shadows.set_valign(gtk4::Align::Center);
        shadows.set_child(Some(&near));
        shadows.add_overlay(&far);

        let root = gtk4::Overlay::new();
        let assembly_width = if width == PANEL_WIDTH {
            width + 2 * EDGE_OUTSET_PX
        } else {
            width
        };
        root.set_size_request(assembly_width, assembly_width);
        root.set_child(Some(&shadows));
        cover.set_halign(gtk4::Align::Center);
        cover.set_valign(gtk4::Align::Center);
        root.add_overlay(cover);
        let edge = (width == PANEL_WIDTH).then(|| {
            let edge = edge_layer(width);
            root.add_overlay(&edge);
            edge
        });
        let widgets = CoverLiftWidgets {
            root,
            near,
            far,
            edge,
        };
        Self {
            widgets: Some(widgets),
            readings: Rc::new(Cell::new(Readings::default())),
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Overlay {
        &self
            .widgets
            .as_ref()
            .expect("production cover lift has widgets")
            .root
    }

    /// The player bar's entry point: it derives no pressure of its own and its
    /// small cover carries no edge seam, so the bed stays where it was.
    pub(in crate::ui) fn set_swell(&self, swell: f64) {
        let mut readings = self.readings.get();
        readings.swell = swell.clamp(0.0, 1.0);
        self.readings.set(readings);
        self.apply_reading();
    }

    pub(in crate::ui) fn feed(&self, swell: f64, pressure: f64) {
        self.readings.set(Readings {
            swell: swell.clamp(0.0, 1.0),
            pressure: pressure.clamp(0.0, 1.0),
        });
        self.apply_reading();
    }

    fn apply_reading(&self) {
        if let Some(widgets) = &self.widgets {
            apply_reading_to_widgets(widgets, self.readings.get());
        }
    }

    #[cfg(test)]
    fn headless_for_test() -> Self {
        Self {
            widgets: None,
            readings: Rc::new(Cell::new(Readings::default())),
        }
    }

    #[cfg(test)]
    fn edge_reading(&self) -> f64 {
        let readings = self.readings.get();
        edge_opacity(readings.pressure, readings.swell)
    }
}

fn apply_reading_to_widgets(widgets: &CoverLiftWidgets, readings: Readings) {
    widgets.near.set_opacity(near_opacity(readings.swell));
    widgets.far.set_opacity(far_opacity(readings.swell));
    if let Some(edge) = &widgets.edge {
        edge.set_opacity(edge_opacity(readings.pressure, readings.swell));
    }
}

fn edge_layer(width: i32) -> gtk4::Box {
    let edge = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    edge.set_size_request(width + 2 * EDGE_OUTSET_PX, width + 2 * EDGE_OUTSET_PX);
    edge.set_can_target(false);
    edge.set_can_focus(false);
    edge.set_halign(gtk4::Align::Center);
    edge.set_valign(gtk4::Align::Center);
    edge.set_opacity(EDGE_REST_OPACITY);
    edge.add_css_class(EDGE_CLASS);
    edge
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
    fn ac_24_the_edge_light_rides_a_pressure_bed_under_a_swell() {
        // Straight from the mockup: 0.18 + 0.10·pres + 0.22·sw.
        assert!((edge_opacity(0.0, 0.0) - 0.18).abs() < 1e-9);
        // A held breakdown: no swell left, but the contour stays lit.
        assert!((edge_opacity(1.0, 0.0) - 0.28).abs() < 1e-9);
        // A broad swell on a lit bed.
        assert!((edge_opacity(0.85, 0.8) - 0.441).abs() < 1e-9);
        // Both at full: the ceiling.
        assert!((edge_opacity(1.0, 1.0) - 0.50).abs() < 1e-9);
        // Out-of-range readings clamp instead of over-driving the seam.
        assert!((edge_opacity(-1.0, -1.0) - 0.18).abs() < 1e-9);
        assert!((edge_opacity(4.0, 4.0) - 0.50).abs() < 1e-9);
    }

    #[test]
    fn ac_24_the_edge_light_is_one_static_pixel_in_the_cover_accent() {
        let css = css();
        assert!(css.contains("border: 1px solid @reprise_cover_light"));
        // The cover's radius plus the one pixel the seam sits outside it.
        assert!(css.contains("border-radius: 13px"));
        // Only the alpha moves. A seam whose width or radius changed per frame
        // would throw away the cached node every frame — the same rule the
        // shadow lift follows.
        assert!(!css.contains("transition"));

        // Live spectrum frames and the backdrop's paused breath are the only
        // frame sources; the cover must not own another timer.
        let timer_api = ["add_tick", "callback"].concat();
        assert!(!include_str!("cover_lift.rs").contains(&timer_api));
    }

    #[test]
    fn ac_24_the_cover_reads_no_beat_anywhere() {
        // Round 5 gave the cover one exception: while the Visualizer view was
        // open its shadow switched from `swell` to the beat. Looked at in use,
        // that read as the cover twitching under its own shadow, and it was
        // rejected along with the waveform's lens and playhead glow. Both
        // layers here now move over seconds, in every view.
        //
        // The type is the guard: `Readings` carries a swell and a pressure and
        // nothing else, so there is no third reading to route anywhere. A
        // source-text assertion would only fight this comment.
        let lift = CoverLift::headless_for_test();
        lift.feed(0.8, 0.5);
        let before = lift.edge_reading();
        lift.feed(0.8, 0.5);
        assert!((lift.edge_reading() - before).abs() < 1e-9);
        assert!((before - edge_opacity(0.5, 0.8)).abs() < 1e-9);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn ac_24_the_edge_light_sits_one_pixel_outside_the_cover() {
        // The design puts the seam outside the artwork ("nichts dahinter"),
        // so the assembly is exactly two pixels wider than the cover and the
        // seam is centred on it — an Overlay child left at its default
        // Align::Fill would silently stretch instead.
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let cover = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        cover.set_size_request(PANEL_WIDTH, PANEL_WIDTH);
        let lift = CoverLift::new(&cover, PANEL_WIDTH);
        let window = gtk4::Window::builder().child(lift.widget()).build();
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        let edge = lift
            .widgets
            .as_ref()
            .and_then(|widgets| widgets.edge.as_ref())
            .expect("the panel cover must carry an edge light");
        let cover_bounds = cover
            .compute_bounds(lift.widget())
            .expect("cover and lift share a coordinate space");
        let edge_bounds = edge
            .compute_bounds(lift.widget())
            .expect("edge and lift share a coordinate space");
        assert_eq!(edge_bounds.width(), cover_bounds.width() + 2.0);
        assert_eq!(edge_bounds.height(), cover_bounds.height() + 2.0);
        assert_eq!(edge_bounds.center(), cover_bounds.center());
        window.close();
    }

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
        // 168 px panel cover and 56 px bar thumbnail from one set of ratios.
        assert_eq!(lift_shadow(168.0, 0.0), "0 8px 24px 0 rgba(0, 0, 0, 0.55)");
        assert_eq!(
            lift_shadow(168.0, 1.0),
            "0 16px 50px -3px rgba(0, 0, 0, 0.55)"
        );
        assert_eq!(lift_shadow(56.0, 0.0), "0 3px 8px 0 rgba(0, 0, 0, 0.55)");
    }
}
