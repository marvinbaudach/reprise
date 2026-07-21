//! Self-contained cover/texture rasterization and time-formatting helpers for
//! `song_visualizer.rs`, split out to keep that file under the 800-line cap.
//! These take every input as a parameter (no module constants) and are used
//! only within `song_visualizer.rs`, so they move cleanly as `pub(super)` free
//! functions.

use gtk4::prelude::*;

pub(super) fn downscale_cover_rgba(texture: &gtk4::gdk::Texture, edge: i32) -> Option<Vec<u8>> {
    let snapshot = gtk4::Snapshot::new();
    let bounds = gtk4::graphene::Rect::new(0.0, 0.0, edge as f32, edge as f32);
    snapshot.append_texture(texture, &bounds);
    let node = snapshot.to_node()?;

    let mut surface =
        gtk4::cairo::ImageSurface::create(gtk4::cairo::Format::ARgb32, edge, edge).ok()?;
    {
        let cr = gtk4::cairo::Context::new(&surface).ok()?;
        node.draw(&cr);
    }
    surface.flush();
    let stride = surface.stride() as usize;
    let data = surface.data().ok()?;

    let edge = edge as usize;
    let mut rgba = Vec::with_capacity(edge * edge * 4);
    for y in 0..edge {
        for x in 0..edge {
            let o = y * stride + x * 4;
            if o + 3 >= data.len() {
                continue;
            }
            // Cairo's ARGB32 is premultiplied, native-endian — on the
            // little-endian hosts this app targets that's byte order
            // [B, G, R, A], same assumption `render_mode_gallery_ppm`
            // (in tests) makes when writing PPM output.
            rgba.push(data[o + 2]);
            rgba.push(data[o + 1]);
            rgba.push(data[o]);
            rgba.push(data[o + 3]);
        }
    }
    Some(rgba)
}

/// Rasterizes `texture` down to a `width`-px-wide (aspect-preserving) surface
/// for the fullscreen backdrop and wraps the raw bytes in a
/// `gdk::MemoryTexture` — no RGB reordering needed here (unlike
/// `downscale_cover_rgba`, which feeds a byte-order-agnostic palette
/// sampler): Cairo's premultiplied `ARGB32` byte layout on the little-endian
/// hosts this app targets is exactly GDK's `B8g8r8a8Premultiplied`, so the
/// surface's bytes are handed to `MemoryTexture` as-is. `None` if
/// rasterization fails or the source texture reports a zero size.
pub(super) fn backdrop_texture(
    texture: &gtk4::gdk::Texture,
    width: i32,
) -> Option<gtk4::gdk::Texture> {
    let (tex_w, tex_h) = (texture.width(), texture.height());
    if tex_w <= 0 || tex_h <= 0 || width <= 0 {
        return None;
    }
    let height = ((width as f32) * (tex_h as f32) / (tex_w as f32))
        .round()
        .max(1.0) as i32;

    let snapshot = gtk4::Snapshot::new();
    let bounds = gtk4::graphene::Rect::new(0.0, 0.0, width as f32, height as f32);
    snapshot.append_texture(texture, &bounds);
    let node = snapshot.to_node()?;

    let mut surface =
        gtk4::cairo::ImageSurface::create(gtk4::cairo::Format::ARgb32, width, height).ok()?;
    {
        let cr = gtk4::cairo::Context::new(&surface).ok()?;
        node.draw(&cr);
    }
    surface.flush();
    let stride = surface.stride() as usize;
    let bytes = gtk4::glib::Bytes::from_owned(surface.data().ok()?.to_vec());
    Some(
        gtk4::gdk::MemoryTexture::new(
            width,
            height,
            gtk4::gdk::MemoryFormat::B8g8r8a8Premultiplied,
            &bytes,
            stride,
        )
        .upcast(),
    )
}

pub(super) fn format_time(ms: i64) -> String {
    let seconds = ms.max(0) / 1_000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

pub(super) fn seek_fraction(position_ms: i64, duration_ms: i64) -> f64 {
    if duration_ms <= 0 {
        0.0
    } else {
        (position_ms as f64 / duration_ms as f64).clamp(0.0, 1.0)
    }
}
