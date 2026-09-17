//! Flattens a portable [`Scene`] into the byte layout the phone reads.
//!
//! A scene is flattened as little-endian `f32` bytes, one record per shape:
//! `[kind, r, g, b, a, width, glow, point_count, geometry...]`.
//! `kind` is `0` for a rectangle (`x, y, w, h`), `1` for a polyline
//! (`x1, y1, ...`), and `2` for a radial glow (`cx, cy, radius`). The rectangle
//! and radial-glow `point_count` fields are respectively `4` and `3`, matching
//! the number of geometry scalars rather than a literal number of points.
//!
//! The shared scene format also carries closed-path and dash metadata. Bars do
//! not use either, so this boundary deliberately omits them rather than adding
//! fields the phone would copy on every rendered frame.

use reprise_core::visuals::{Fill, Geom, Scene};

const RECT_KIND: f32 = 0.0;
const POLYLINE_KIND: f32 = 1.0;
const RADIAL_GLOW_KIND: f32 = 2.0;
const RECORD_PREFIX_LEN: usize = 8;

pub(crate) fn encode_scene(scene: &Scene) -> Vec<u8> {
    let geometry_len = scene
        .shapes
        .iter()
        .map(|shape| match &shape.geom {
            Geom::Rect { .. } => 4,
            Geom::Polyline { points, .. } => points.len() * 2,
            Geom::RadialGlow { .. } => 3,
        })
        .sum::<usize>();
    let scalar_count = scene.shapes.len() * RECORD_PREFIX_LEN + geometry_len;
    let mut buffer = Vec::with_capacity(scalar_count * size_of::<f32>());

    for shape in &scene.shapes {
        let Fill::Solid(color) = &shape.fill;
        let (kind, point_count) = match &shape.geom {
            Geom::Rect { .. } => (RECT_KIND, 4),
            Geom::Polyline { points, .. } => (POLYLINE_KIND, points.len()),
            Geom::RadialGlow { .. } => (RADIAL_GLOW_KIND, 3),
        };
        for value in [
            kind,
            color.r,
            color.g,
            color.b,
            color.a,
            shape.width,
            shape.glow,
            point_count as f32,
        ] {
            push_float_bytes(&mut buffer, value);
        }
        match &shape.geom {
            Geom::Rect { x, y, w, h } => {
                for value in [*x, *y, *w, *h] {
                    push_float_bytes(&mut buffer, value);
                }
            }
            Geom::Polyline { points, .. } => {
                for (x, y) in points {
                    push_float_bytes(&mut buffer, *x);
                    push_float_bytes(&mut buffer, *y);
                }
            }
            Geom::RadialGlow { cx, cy, r } => {
                for value in [*cx, *cy, *r] {
                    push_float_bytes(&mut buffer, value);
                }
            }
        }
    }

    buffer
}

fn push_float_bytes(buffer: &mut Vec<u8>, value: f32) {
    buffer.extend_from_slice(&value.to_le_bytes());
}
