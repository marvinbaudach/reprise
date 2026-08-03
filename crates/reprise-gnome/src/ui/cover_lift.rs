//! Cached near/far cover shadows whose opacities cross-fade on a bass hit.
//!
//! The blur geometry is CSS-static: changing a `box-shadow` blur every frame
//! invalidates GTK's cached shadow node, while changing opacity keeps both
//! nodes reusable.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::{cairo, prelude::*};

use crate::ui::{motion, style::tokens::RADIUS_SURFACE};

const LIFT_Y_REST: f64 = 0.048;
const LIFT_Y_PER_KICK: f64 = 0.048;
const LIFT_BLUR_REST: f64 = 0.14;
const LIFT_BLUR_PER_KICK: f64 = 0.155;
const LIFT_SPREAD_PER_KICK: f64 = -0.018;
const LIFT_COLOUR: &str = "rgba(0, 0, 0, 0.55)";

const SHEEN_WIDTH: f64 = 0.30;
const SHEEN_ANGLE_DEG: f64 = 18.0;
const SHEEN_TRAVEL: f64 = 0.50;
const SHEEN_PERIOD_S: f64 = 13.0;
const SHEEN_REST_OPACITY: f64 = 0.22;
const SHEEN_OPACITY_PER_KICK: f64 = 0.18;
const SHEEN_ACCENT_ALPHA: f64 = 0.30;
const SHEEN_RADIUS: f64 = 12.0;

const PANEL_WIDTH: i32 = 168;
const BAR_WIDTH: i32 = 56;
const SHADOW_BASE_CLASS: &str = "reprise-cover-lift-shadow";
const PANEL_NEAR_CLASS: &str = "reprise-cover-lift-panel-near";
const PANEL_FAR_CLASS: &str = "reprise-cover-lift-panel-far";
const BAR_NEAR_CLASS: &str = "reprise-cover-lift-bar-near";
const BAR_FAR_CLASS: &str = "reprise-cover-lift-bar-far";
const SHEEN_CLASS: &str = "reprise-cover-sheen";

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

/// Alpha of a single shadow layer, mirrored from [`LIFT_COLOUR`]. The
/// cross-fade has to know it, because two translucent blacks do not add
/// linearly.
const SHADOW_ALPHA: f64 = 0.55;

/// Opacity of the resting layer, compensated so the *composite* coverage of
/// the two layers stays constant while their shape morphs.
///
/// A plain `1.0 - kick` looks right on paper and flickers on screen: at
/// `kick = 0.5` both layers sit at 0.275, and `1 - (1-0.275)²` is 0.474
/// against 0.550 at either end — a 14 % brightening in the middle of every
/// hit, then back. Solving `1 - (1 - a·near)(1 - a·far) = a` for `near`
/// removes it exactly.
pub(in crate::ui) fn near_opacity(kick: f64) -> f64 {
    let far = SHADOW_ALPHA * kick.clamp(0.0, 1.0);
    if far >= 1.0 {
        return 0.0;
    }
    ((1.0 - (1.0 - SHADOW_ALPHA) / (1.0 - far)) / SHADOW_ALPHA).clamp(0.0, 1.0)
}

pub(in crate::ui) fn far_opacity(kick: f64) -> f64 {
    kick.clamp(0.0, 1.0)
}

/// Composite coverage of both layers — the quantity that must not move.
#[cfg(test)]
fn composite_coverage(kick: f64) -> f64 {
    1.0 - (1.0 - SHADOW_ALPHA * near_opacity(kick)) * (1.0 - SHADOW_ALPHA * far_opacity(kick))
}

pub(in crate::ui) fn sheen_opacity(kick: f64) -> f64 {
    SHEEN_REST_OPACITY + SHEEN_OPACITY_PER_KICK * kick.clamp(0.0, 1.0)
}

/// Horizontal offset of the reflection's centre at `elapsed_s`.
pub(in crate::ui) fn sheen_offset(elapsed_s: f64, width: f64) -> f64 {
    let phase = std::f64::consts::TAU * elapsed_s / SHEEN_PERIOD_S;
    SHEEN_TRAVEL * width * phase.sin()
}

pub(in crate::ui) fn css() -> String {
    format!(
        ".{SHADOW_BASE_CLASS} {{ border-radius: {RADIUS_SURFACE}; }}\n\
         .{PANEL_NEAR_CLASS} {{ box-shadow: {}; }}\n\
         .{PANEL_FAR_CLASS} {{ box-shadow: {}; }}\n\
         .{BAR_NEAR_CLASS} {{ box-shadow: {}; }}\n\
         .{BAR_FAR_CLASS} {{ box-shadow: {}; }}\n\
         .{SHEEN_CLASS} {{ color: @reprise_player_accent; }}",
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
    sheen: Option<CoverSheen>,
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
        let sheen = (width == PANEL_WIDTH).then(|| {
            let sheen = CoverSheen::new(width);
            root.add_overlay(sheen.widget());
            sheen
        });
        Self {
            root,
            near,
            far,
            sheen,
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Overlay {
        &self.root
    }

    pub(in crate::ui) fn set_kick(&self, kick: f64) {
        let kick = motion::reactive_amplitude(kick);
        self.near.set_opacity(near_opacity(kick));
        self.far.set_opacity(far_opacity(kick));
        if let Some(sheen) = &self.sheen {
            sheen.set_kick(kick);
        }
    }

    pub(in crate::ui) fn set_frame_time(&self, frame_time_us: i64) {
        if let Some(sheen) = &self.sheen {
            sheen.set_frame_time(frame_time_us);
        }
    }
}

#[derive(Clone)]
struct CoverSheen {
    area: gtk4::DrawingArea,
    frame_time_us: Rc<Cell<i64>>,
}

impl CoverSheen {
    fn new(width: i32) -> Self {
        let area = gtk4::DrawingArea::builder()
            .width_request(width)
            .height_request(width)
            .can_target(false)
            .can_focus(false)
            .opacity(SHEEN_REST_OPACITY)
            .build();
        area.add_css_class(SHEEN_CLASS);
        let frame_time_us = Rc::new(Cell::new(0));
        area.set_draw_func({
            let frame_time_us = frame_time_us.clone();
            move |area, cr, width, height| {
                let elapsed_s = frame_time_us.get() as f64 / 1_000_000.0;
                draw_sheen(area, cr, width, height, elapsed_s);
            }
        });
        Self {
            area,
            frame_time_us,
        }
    }

    fn widget(&self) -> &gtk4::DrawingArea {
        &self.area
    }

    fn set_kick(&self, kick: f64) {
        self.area.set_opacity(sheen_opacity(kick));
    }

    fn set_frame_time(&self, frame_time_us: i64) {
        let frame_time_us = if motion::animations_enabled() {
            frame_time_us
        } else {
            0
        };
        self.frame_time_us.set(frame_time_us);
        self.area.queue_draw();
    }
}

fn draw_sheen(
    area: &gtk4::DrawingArea,
    cr: &cairo::Context,
    width: i32,
    height: i32,
    elapsed_s: f64,
) {
    if width <= 0 || height <= 0 {
        return;
    }
    let width = f64::from(width);
    let height = f64::from(height);
    let strip_width = SHEEN_WIDTH * width;
    let accent = area.color();

    cr.save().ok();
    rounded_rectangle(cr, 0.0, 0.0, width, height, SHEEN_RADIUS);
    cr.clip();
    cr.translate(width / 2.0 + sheen_offset(elapsed_s, width), height / 2.0);
    cr.rotate(SHEEN_ANGLE_DEG.to_radians());
    let gradient = cairo::LinearGradient::new(-strip_width / 2.0, 0.0, strip_width / 2.0, 0.0);
    gradient.add_color_stop_rgba(
        0.0,
        f64::from(accent.red()),
        f64::from(accent.green()),
        f64::from(accent.blue()),
        0.0,
    );
    gradient.add_color_stop_rgba(
        0.30,
        f64::from(accent.red()),
        f64::from(accent.green()),
        f64::from(accent.blue()),
        SHEEN_ACCENT_ALPHA,
    );
    gradient.add_color_stop_rgba(
        1.0,
        f64::from(accent.red()),
        f64::from(accent.green()),
        f64::from(accent.blue()),
        0.0,
    );
    if cr.set_source(&gradient).is_ok() {
        cr.rectangle(-strip_width / 2.0, -height, strip_width, height * 2.0);
        cr.fill().ok();
    }
    cr.restore().ok();
}

fn rounded_rectangle(cr: &cairo::Context, x: f64, y: f64, w: f64, h: f64, radius: f64) {
    let radius = radius.min(w / 2.0).min(h / 2.0);
    cr.new_sub_path();
    cr.arc(
        x + w - radius,
        y + radius,
        radius,
        -std::f64::consts::FRAC_PI_2,
        0.0,
    );
    cr.arc(
        x + w - radius,
        y + h - radius,
        radius,
        0.0,
        std::f64::consts::FRAC_PI_2,
    );
    cr.arc(
        x + radius,
        y + h - radius,
        radius,
        std::f64::consts::FRAC_PI_2,
        std::f64::consts::PI,
    );
    cr.arc(
        x + radius,
        y + radius,
        radius,
        std::f64::consts::PI,
        std::f64::consts::PI + std::f64::consts::FRAC_PI_2,
    );
    cr.close_path();
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
        // What must stay constant is the COMPOSITE coverage, not the sum of
        // the two opacities. Two translucent blacks do not add linearly: a
        // plain `1 - kick` pair sums to one and still dips to 0.474 against
        // 0.550 in the middle, which reads as a bright/dark flicker on every
        // hit. Assert the thing the eye actually sees.
        for step in 0..=100 {
            let kick = f64::from(step) / 100.0;
            assert!(
                (composite_coverage(kick) - SHADOW_ALPHA).abs() < 1e-9,
                "the shadow changes weight at kick {kick}: {}",
                composite_coverage(kick)
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

    #[test]
    fn ac_24_the_sheen_travels_on_time_and_only_brightens_on_the_kick() {
        // Opacity is the only thing the music touches.
        assert!((sheen_opacity(0.0) - 0.22).abs() < 1e-9);
        assert!((sheen_opacity(1.0) - 0.40).abs() < 1e-9);
        assert!((sheen_opacity(4.0) - 0.40).abs() < 1e-9);

        // The travel is a 13 s sine over ±0.5 w and depends on nothing else.
        let w = 168.0;
        assert!((sheen_offset(0.0, w) - 0.0).abs() < 1e-9);
        assert!((sheen_offset(13.0 / 4.0, w) - 0.5 * w).abs() < 1e-6);
        assert!((sheen_offset(13.0 * 3.0 / 4.0, w) + 0.5 * w).abs() < 1e-6);
        assert!((sheen_offset(0.0, w) - sheen_offset(13.0, w)).abs() < 1e-6);

        // Live spectrum frames and the backdrop's existing paused breath are
        // the only frame sources; the cover must not own another timer.
        let timer_api = ["add_tick", "callback"].concat();
        assert!(!include_str!("cover_lift.rs").contains(&timer_api));
    }
}
