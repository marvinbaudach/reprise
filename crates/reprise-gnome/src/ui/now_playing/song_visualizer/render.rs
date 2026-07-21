//! Cairo renderer for a portable `reprise_core::visuals::Scene`.
use reprise_core::visuals::{Fill, Geom, Scene};

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

pub(super) fn draw_scene(cr: &gtk4::cairo::Context, scene: &Scene) {
    use std::f64::consts::TAU;
    cr.set_line_cap(gtk4::cairo::LineCap::Round);
    cr.set_line_join(gtk4::cairo::LineJoin::Round);
    for shape in &scene.shapes {
        if let Geom::RadialGlow { cx, cy, r } = shape.geom {
            let (cx, cy, r) = (f64::from(cx), f64::from(cy), f64::from(r).max(1.0));
            if let Fill::Solid(c) = shape.fill {
                let grad = gtk4::cairo::RadialGradient::new(cx, cy, 0.0, cx, cy, r);
                grad.add_color_stop_rgba(
                    0.0,
                    f64::from(c.r),
                    f64::from(c.g),
                    f64::from(c.b),
                    f64::from(c.a).clamp(0.0, 1.0),
                );
                grad.add_color_stop_rgba(1.0, f64::from(c.r), f64::from(c.g), f64::from(c.b), 0.0);
                if cr.set_source(&grad).is_ok() {
                    cr.arc(cx, cy, r, 0.0, TAU);
                    let _ = cr.fill();
                }
            }
            continue;
        }
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
        apply_fill(cr, &shape.fill, 1.0);
        if shape.width > 0.0 {
            cr.set_line_width(f64::from(shape.width));
            trace(cr, &shape.geom);
            let _ = cr.stroke();
        } else {
            trace(cr, &shape.geom);
            let _ = cr.fill();
        }
    }
    cr.set_dash(&[], 0.0);
}
