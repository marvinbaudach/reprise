//! Cairo renderer for a portable `reprise_core::visuals::Scene`.
use reprise_core::visuals::{Fill, Geom, Scene, Shape};

fn apply_fill(cr: &gtk4::cairo::Context, fill: &Fill, alpha_scale: f64) {
    match fill {
        Fill::Solid(c) => cr.set_source_rgba(
            f64::from(c.r),
            f64::from(c.g),
            f64::from(c.b),
            (f64::from(c.a) * alpha_scale).clamp(0.0, 1.0),
        ),
        Fill::HGradient { x0, x1, stops } => {
            let grad = gtk4::cairo::LinearGradient::new(f64::from(*x0), 0.0, f64::from(*x1), 0.0);
            for (off, c) in stops {
                grad.add_color_stop_rgba(
                    f64::from(*off),
                    f64::from(c.r),
                    f64::from(c.g),
                    f64::from(c.b),
                    (f64::from(c.a) * alpha_scale).clamp(0.0, 1.0),
                );
            }
            let _ = cr.set_source(&grad);
        }
    }
}

fn trace(cr: &gtk4::cairo::Context, geom: &Geom) {
    use std::f64::consts::TAU;
    match geom {
        Geom::Polyline { points, closed } => {
            let Some(first) = points.first() else {
                return;
            };
            cr.move_to(f64::from(first.0), f64::from(first.1));
            for p in &points[1..] {
                cr.line_to(f64::from(p.0), f64::from(p.1));
            }
            if *closed {
                cr.close_path();
            }
        }
        Geom::Arc { cx, cy, r, a0, a1 } => cr.arc(
            f64::from(*cx),
            f64::from(*cy),
            f64::from(*r),
            f64::from(*a0),
            f64::from(*a1),
        ),
        Geom::Disc { cx, cy, r } => {
            cr.arc(f64::from(*cx), f64::from(*cy), f64::from(*r), 0.0, TAU);
        }
        Geom::Rect { x, y, w, h } => {
            cr.rectangle(f64::from(*x), f64::from(*y), f64::from(*w), f64::from(*h));
        }
        Geom::RadialGlow { .. } => {}
    }
}

/// Draws one shape's normal geometry (fill or stroke, never the fake-bloom
/// under-stroke), with its fill alpha scaled by `alpha_scale`. Shared by
/// `draw_scene` (the Cairo fallback, after its own under-stroke bloom pass),
/// `draw_crisp` (`alpha_scale` always `1.0`), and `draw_glow_layer`
/// (`alpha_scale` = the shape's `glow`, for shapes that have one) so the
/// three renderers agree on exactly what "one shape, once" looks like.
fn draw_shape(cr: &gtk4::cairo::Context, shape: &Shape, alpha_scale: f64) {
    use std::f64::consts::TAU;
    if let Geom::RadialGlow { cx, cy, r } = shape.geom {
        let (cx, cy, r) = (f64::from(cx), f64::from(cy), f64::from(r).max(1.0));
        if let Fill::Solid(c) = shape.fill {
            let grad = gtk4::cairo::RadialGradient::new(cx, cy, 0.0, cx, cy, r);
            grad.add_color_stop_rgba(
                0.0,
                f64::from(c.r),
                f64::from(c.g),
                f64::from(c.b),
                (f64::from(c.a) * alpha_scale).clamp(0.0, 1.0),
            );
            grad.add_color_stop_rgba(1.0, f64::from(c.r), f64::from(c.g), f64::from(c.b), 0.0);
            if cr.set_source(&grad).is_ok() {
                cr.arc(cx, cy, r, 0.0, TAU);
                let _ = cr.fill();
            }
        }
        return;
    }
    match shape.dash {
        Some((on, off)) => cr.set_dash(&[f64::from(on), f64::from(off)], 0.0),
        None => cr.set_dash(&[], 0.0),
    }
    apply_fill(cr, &shape.fill, alpha_scale);
    if shape.width > 0.0 {
        cr.set_line_width(f64::from(shape.width));
        trace(cr, &shape.geom);
        let _ = cr.stroke();
    } else {
        trace(cr, &shape.geom);
        let _ = cr.fill();
    }
}

/// The Cairo fallback: every shape crisp, plus a wide translucent
/// under-stroke pass for `glow > 0` shapes — a cheap fake bloom for hosts
/// with no GPU blur node. See `draw_crisp`/`draw_glow_layer` for the GPU
/// path, which draws the same crisp pass but blurs a real glow layer via
/// `gtk4::Snapshot::push_blur` instead of faking it here.
pub(super) fn draw_scene(cr: &gtk4::cairo::Context, scene: &Scene) {
    cr.set_line_cap(gtk4::cairo::LineCap::Round);
    cr.set_line_join(gtk4::cairo::LineJoin::Round);
    for shape in &scene.shapes {
        match shape.dash {
            Some((on, off)) => cr.set_dash(&[f64::from(on), f64::from(off)], 0.0),
            None => cr.set_dash(&[], 0.0),
        }
        if shape.glow > 0.0 && shape.width > 0.0 {
            apply_fill(cr, &shape.fill, f64::from(shape.glow) * 0.35);
            cr.set_line_width(f64::from(shape.width) * 3.0);
            trace(cr, &shape.geom);
            let _ = cr.stroke();
        }
        draw_shape(cr, shape, 1.0);
    }
    cr.set_dash(&[], 0.0);
}

/// All shapes drawn crisp — no fake-bloom under-stroke pass, since the GPU
/// path's real blur (see `draw_glow_layer`) provides the bloom instead.
pub(super) fn draw_crisp(cr: &gtk4::cairo::Context, scene: &Scene) {
    cr.set_line_cap(gtk4::cairo::LineCap::Round);
    cr.set_line_join(gtk4::cairo::LineJoin::Round);
    for shape in &scene.shapes {
        draw_shape(cr, shape, 1.0);
    }
    cr.set_dash(&[], 0.0);
}

/// Only shapes with `glow > 0`, drawn at their normal geometry with alpha
/// scaled by `glow` — the layer the GPU path wraps in
/// `gtk4::Snapshot::push_blur`/`pop` for a real Gaussian bloom.
pub(super) fn draw_glow_layer(cr: &gtk4::cairo::Context, scene: &Scene) {
    cr.set_line_cap(gtk4::cairo::LineCap::Round);
    cr.set_line_join(gtk4::cairo::LineJoin::Round);
    for shape in &scene.shapes {
        if shape.glow > 0.0 {
            draw_shape(cr, shape, f64::from(shape.glow));
        }
    }
    cr.set_dash(&[], 0.0);
}

/// Max blur radius (px) for the scene's glow layer, derived from the
/// brightest `glow` across all shapes; `0.0` tells the caller to skip the
/// blur pass entirely (nothing glows this frame).
pub(super) fn scene_blur_radius(scene: &Scene) -> f32 {
    let max_glow = scene.shapes.iter().map(|s| s.glow).fold(0.0_f32, f32::max);
    if max_glow <= 0.0 {
        0.0
    } else {
        2.0 + 14.0 * max_glow.min(1.0)
    }
}

#[cfg(test)]
/// Counts shapes with `glow > 0` — the same predicate `draw_glow_layer` uses
/// to pick its layer, exposed so the split can be unit-tested without a
/// Cairo surface.
fn glow_shape_count(scene: &Scene) -> usize {
    scene.shapes.iter().filter(|s| s.glow > 0.0).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::visuals::Rgba;

    fn shape(glow: f32) -> Shape {
        Shape {
            geom: Geom::Disc {
                cx: 10.0,
                cy: 10.0,
                r: 3.0,
            },
            fill: Fill::Solid(Rgba {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            }),
            width: 0.0,
            glow,
            dash: None,
        }
    }

    #[test]
    fn blur_radius_is_zero_for_an_all_crisp_scene() {
        let scene = Scene {
            shapes: vec![shape(0.0), shape(0.0)],
        };
        assert_eq!(scene_blur_radius(&scene), 0.0);
        assert_eq!(glow_shape_count(&scene), 0);
    }

    #[test]
    fn blur_radius_scales_with_the_brightest_glow() {
        let scene = Scene {
            shapes: vec![shape(0.0), shape(0.8)],
        };
        let radius = scene_blur_radius(&scene);
        assert!(radius > 0.0);
        assert_eq!(radius, 2.0 + 14.0 * 0.8_f32);
        assert_eq!(glow_shape_count(&scene), 1);
    }

    #[test]
    fn blur_radius_clamps_glow_above_one() {
        let scene = Scene {
            shapes: vec![shape(5.0)],
        };
        assert_eq!(scene_blur_radius(&scene), 2.0 + 14.0);
    }
}
