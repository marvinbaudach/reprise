//! Cairo renderer for a portable `reprise_core::visuals::Scene`.
use reprise_core::visuals::{Fill, Geom, Scene, Shape};

const MAX_SCENE_WIDTH: i32 = 800;
const MAX_SCENE_HEIGHT: i32 = 450;

/// Caps only the expensive scene raster. The finished image is scaled back to
/// the widget allocation, so fullscreen remains fullscreen while Grid/Bars
/// geometry and Cairo work stay inside the 60 Hz render budget.
pub(super) fn capped_scene_size(width: i32, height: i32) -> (i32, i32) {
    let width = width.max(1);
    let height = height.max(1);
    if width <= MAX_SCENE_WIDTH && height <= MAX_SCENE_HEIGHT {
        return (width, height);
    }

    let scale = (f64::from(MAX_SCENE_WIDTH) / f64::from(width))
        .min(f64::from(MAX_SCENE_HEIGHT) / f64::from(height));
    (
        (f64::from(width) * scale).round() as i32,
        (f64::from(height) * scale).round() as i32,
    )
}

#[derive(Default)]
pub(super) struct SceneRenderer {
    surface: Option<gtk4::cairo::ImageSurface>,
    surface_size: (i32, i32),
}

impl SceneRenderer {
    fn surface(
        &mut self,
        width: i32,
        height: i32,
    ) -> Result<&gtk4::cairo::ImageSurface, gtk4::cairo::Error> {
        if self.surface.is_none() || self.surface_size != (width, height) {
            self.surface = Some(gtk4::cairo::ImageSurface::create(
                gtk4::cairo::Format::ARgb32,
                width,
                height,
            )?);
            self.surface_size = (width, height);
        }
        Ok(self.surface.as_ref().expect("surface was just initialized"))
    }

    pub(super) fn draw(
        &mut self,
        cr: &gtk4::cairo::Context,
        scene: &Scene,
        output_width: i32,
        output_height: i32,
    ) {
        if output_width <= 0 || output_height <= 0 {
            return;
        }
        let scene_size = capped_scene_size(output_width, output_height);
        if scene_size == (output_width, output_height) {
            draw_scene(cr, scene);
            return;
        }

        let Ok(surface) = self.surface(scene_size.0, scene_size.1) else {
            draw_scaled_scene(cr, scene, scene_size, (output_width, output_height));
            return;
        };
        let Ok(buffer_cr) = gtk4::cairo::Context::new(surface) else {
            draw_scaled_scene(cr, scene, scene_size, (output_width, output_height));
            return;
        };
        let _ = buffer_cr.save();
        buffer_cr.set_operator(gtk4::cairo::Operator::Clear);
        let _ = buffer_cr.paint();
        buffer_cr.set_operator(gtk4::cairo::Operator::Over);
        draw_scene(&buffer_cr, scene);
        let _ = buffer_cr.restore();
        surface.flush();

        let _ = cr.save();
        cr.scale(
            f64::from(output_width) / f64::from(scene_size.0),
            f64::from(output_height) / f64::from(scene_size.1),
        );
        if cr.set_source_surface(surface, 0.0, 0.0).is_ok() {
            cr.source().set_filter(gtk4::cairo::Filter::Bilinear);
            let _ = cr.paint();
        }
        let _ = cr.restore();
    }
}

fn draw_scaled_scene(
    cr: &gtk4::cairo::Context,
    scene: &Scene,
    scene_size: (i32, i32),
    output_size: (i32, i32),
) {
    let _ = cr.save();
    cr.scale(
        f64::from(output_size.0) / f64::from(scene_size.0),
        f64::from(output_size.1) / f64::from(scene_size.1),
    );
    draw_scene(cr, scene);
    let _ = cr.restore();
}

fn apply_fill(cr: &gtk4::cairo::Context, fill: &Fill, alpha_scale: f64) {
    match fill {
        Fill::Solid(c) => cr.set_source_rgba(
            f64::from(c.r),
            f64::from(c.g),
            f64::from(c.b),
            (f64::from(c.a) * alpha_scale).clamp(0.0, 1.0),
        ),
    }
}

fn trace(cr: &gtk4::cairo::Context, geom: &Geom) {
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
        Geom::Rect { x, y, w, h } => {
            cr.rectangle(f64::from(*x), f64::from(*y), f64::from(*w), f64::from(*h));
        }
        Geom::RadialGlow { .. } => {}
    }
}

/// Draws one shape's normal geometry (fill or stroke, never the bloom
/// under-stroke), with its fill alpha scaled by `alpha_scale`. Used by
/// `draw_scene` after its own under-stroke bloom pass.
fn draw_shape(cr: &gtk4::cairo::Context, shape: &Shape, alpha_scale: f64) {
    use std::f64::consts::TAU;
    if let Geom::RadialGlow { cx, cy, r } = shape.geom {
        let (cx, cy, r) = (f64::from(cx), f64::from(cy), f64::from(r).max(1.0));
        match shape.fill {
            Fill::Solid(c) => {
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

/// Draws the scene: every shape crisp, plus a wide translucent under-stroke
/// pass for `glow > 0` shapes — the bloom, approximated with Cairo since
/// there is no GPU blur node in this path.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ac_20_fullscreen_render_size_is_capped_without_upscaling_inline_canvases() {
        assert_eq!(capped_scene_size(548, 300), (548, 300));
        assert_eq!(capped_scene_size(800, 450), (800, 450));
        assert_eq!(capped_scene_size(1920, 1080), (800, 450));
        assert_eq!(capped_scene_size(2560, 1080), (800, 338));
    }
}
