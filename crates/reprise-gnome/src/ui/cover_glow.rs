//! Shared cover-to-light rasterization for the now-playing panel and player bar.
//!
//! The texture is reduced once per cover generation. Enlarging that tiny
//! cached surface with bilinear filtering supplies the blur; live spectrum
//! frames only alter drawing alpha and scale.

use gtk4::cairo;
use gtk4::prelude::SnapshotExt;

pub(in crate::ui) const BLUR_EDGE: i32 = 32;

pub(in crate::ui) fn blurred_surface(texture: &gtk4::gdk::Texture) -> Option<cairo::ImageSurface> {
    let snapshot = gtk4::Snapshot::new();
    let bounds = gtk4::graphene::Rect::new(0.0, 0.0, BLUR_EDGE as f32, BLUR_EDGE as f32);
    snapshot.append_texture(texture, &bounds);
    let node = snapshot.to_node()?;
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, BLUR_EDGE, BLUR_EDGE).ok()?;
    {
        let cr = cairo::Context::new(&surface).ok()?;
        node.draw(&cr);
    }
    surface.flush();
    Some(surface)
}
