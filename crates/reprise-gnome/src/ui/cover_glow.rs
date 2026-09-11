//! Shared cover-to-light rasterization for the now-playing panel and player bar.
//!
//! The texture is reduced once per cover generation. Enlarging that tiny
//! cached surface with bilinear filtering supplies the blur; live spectrum
//! frames only alter drawing alpha and scale.

use gtk4::cairo;
use gtk4::prelude::SnapshotExt;

pub(in crate::ui) const BLUR_EDGE: i32 = 32;

pub(in crate::ui) fn blurred_surface(
    texture: &gtk4::gdk::Texture,
    size: i32,
) -> Option<cairo::ImageSurface> {
    let snapshot = gtk4::Snapshot::new();
    let bounds = gtk4::graphene::Rect::new(0.0, 0.0, size as f32, size as f32);
    snapshot.append_texture(texture, &bounds);
    let node = snapshot.to_node()?;
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, size, size).ok()?;
    {
        let cr = cairo::Context::new(&surface).ok()?;
        node.draw(&cr);
    }
    surface.flush();
    Some(surface)
}

#[cfg(test)]
mod tests {
    use gtk4::prelude::Cast;

    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn blurred_surface_uses_the_requested_raster_size() {
        gtk4::init().expect("gtk");
        let texture: gtk4::gdk::Texture = gtk4::gdk::MemoryTexture::new(
            1,
            1,
            gtk4::gdk::MemoryFormat::B8g8r8a8Premultiplied,
            &gtk4::glib::Bytes::from_static(&[0, 0, 0, 255]),
            4,
        )
        .upcast();

        for size in [BLUR_EDGE, 28] {
            let surface = blurred_surface(&texture, size).expect("blurred surface");
            assert_eq!(surface.width(), size);
            assert_eq!(surface.height(), size);
        }
    }
}
