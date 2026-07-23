//! Self-contained cover/texture rasterization helpers for
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
