//! Cached artwork rasters for the independently drifting cover-cloud drops.

use gtk4::cairo;

use super::cover_cloud::{DriftAxis, DriftProfile};
use crate::ui::cover_glow;

pub(super) const BLOBS_PER_LAYER: usize = 3;

/// Edge of each cached drop raster. Its falloff stays smooth however far the
/// drift stretches it.
pub(super) const FIELD_RASTER_EDGE: i32 = 320;

/// A proportionally smaller source makes the front layer softer.
pub(super) const BACK_BLUR_EDGE: i32 = cover_glow::BLUR_EDGE;
pub(super) const FRONT_BLUR_EDGE: i32 = 28;

/// One independently moving drop cut from the blurred artwork.
#[derive(Clone, Copy)]
pub(super) struct Blob {
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) alpha: f64,
    pub(super) radius: f64,
    pub(super) drift: DriftProfile,
}

const fn axis(slow_s: f64, fast_s: f64, slow_phase: f64, fast_phase: f64) -> DriftAxis {
    DriftAxis {
        slow_s,
        fast_s,
        slow_phase,
        fast_phase,
    }
}

const fn profile(x: DriftAxis, y: DriftAxis, scale: DriftAxis) -> DriftProfile {
    DriftProfile { x, y, scale }
}

pub(super) const BACK_BLOBS: [Blob; BLOBS_PER_LAYER] = [
    Blob {
        x: 0.40,
        y: 0.35,
        alpha: 0.85,
        radius: 0.50,
        drift: profile(
            axis(89.0, 61.0, 0.03, 0.41),
            axis(127.0, 101.0, 0.19, 0.73),
            axis(173.0, 149.0, 0.37, 0.89),
        ),
    },
    Blob {
        x: 0.82,
        y: 0.55,
        alpha: 0.80,
        radius: 0.50,
        drift: profile(
            axis(83.0, 67.0, 0.23, 0.67),
            axis(131.0, 103.0, 0.47, 0.07),
            axis(179.0, 151.0, 0.71, 0.31),
        ),
    },
    Blob {
        x: 0.24,
        y: 0.76,
        alpha: 0.78,
        radius: 0.50,
        drift: profile(
            axis(97.0, 59.0, 0.11, 0.53),
            axis(137.0, 107.0, 0.17, 0.79),
            axis(181.0, 157.0, 0.43, 0.97),
        ),
    },
];

pub(super) const FRONT_BLOBS: [Blob; BLOBS_PER_LAYER] = [
    Blob {
        x: 0.75,
        y: 0.25,
        alpha: 0.70,
        radius: 0.45,
        drift: profile(
            axis(79.0, 71.0, 0.13, 0.59),
            axis(139.0, 109.0, 0.29, 0.83),
            axis(191.0, 163.0, 0.61, 0.05),
        ),
    },
    Blob {
        x: 0.30,
        y: 0.80,
        alpha: 0.60,
        radius: 0.45,
        drift: profile(
            axis(113.0, 73.0, 0.21, 0.69),
            axis(193.0, 167.0, 0.39, 0.87),
            axis(197.0, 211.0, 0.55, 0.01),
        ),
    },
    Blob {
        x: 0.52,
        y: 0.48,
        alpha: 0.65,
        radius: 0.45,
        drift: profile(
            axis(81.0, 77.0, 0.09, 0.49),
            axis(199.0, 223.0, 0.27, 0.77),
            axis(227.0, 229.0, 0.63, 0.93),
        ),
    },
];

pub(super) type BlobRasters = [cairo::ImageSurface; BLOBS_PER_LAYER];

/// Bakes one raster per drop while sharing the layer's blurred source.
pub(super) fn build_blob_rasters(
    texture: &gtk4::gdk::Texture,
    blur_edge: i32,
    blobs: &[Blob; BLOBS_PER_LAYER],
) -> Option<BlobRasters> {
    let blurred = cover_glow::blurred_surface(texture, blur_edge)?;
    let rasters: Vec<_> = blobs
        .iter()
        .map(|blob| build_blob_raster(&blurred, *blob))
        .collect::<Option<_>>()?;
    rasters.try_into().ok()
}

fn build_blob_raster(blurred: &cairo::ImageSurface, blob: Blob) -> Option<cairo::ImageSurface> {
    let surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, FIELD_RASTER_EDGE, FIELD_RASTER_EDGE)
            .ok()?;
    let cr = cairo::Context::new(&surface).ok()?;
    let edge = f64::from(FIELD_RASTER_EDGE);

    let scale = edge / f64::from(blurred.width());
    cr.save().ok();
    cr.scale(scale, scale);
    if cr.set_source_surface(blurred, 0.0, 0.0).is_ok() {
        cr.source().set_filter(cairo::Filter::Bilinear);
        cr.source().set_extend(cairo::Extend::Pad);
        cr.paint().ok();
    }
    cr.restore().ok();

    let cx = blob.x * edge;
    let cy = blob.y * edge;
    let radius = blob.radius * edge;
    let mask = cairo::RadialGradient::new(cx, cy, 0.0, cx, cy, radius);
    mask.add_color_stop_rgba(0.0, 0.0, 0.0, 0.0, blob.alpha);
    mask.add_color_stop_rgba(1.0, 0.0, 0.0, 0.0, 0.0);
    cr.set_operator(cairo::Operator::DestIn);
    cr.set_source(&mask).ok();
    cr.paint().ok();
    drop(cr);
    Some(surface)
}
