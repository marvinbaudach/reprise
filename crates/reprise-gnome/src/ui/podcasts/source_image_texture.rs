//! Decoded source-artwork pixels and the main-thread texture cache.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::Path;

use gtk4::prelude::*;
use reprise_core::remote_image::CacheScope;

const CACHE_LIMIT: usize = 128;

thread_local! {
    static TEXTURE_CACHE: RefCell<VecDeque<(String, i32, i32, CacheScope, gtk4::gdk::Texture)>> =
        const { RefCell::new(VecDeque::new()) };
}

pub(super) struct DecodedPixels {
    pub(super) bytes: Vec<u8>,
    pub(super) width: i32,
    pub(super) height: i32,
    pub(super) rowstride: usize,
    pub(super) has_alpha: bool,
}

pub(super) fn decode_pixels(
    path: &Path,
    width: i32,
    height: i32,
) -> Result<DecodedPixels, gtk4::glib::Error> {
    let pixbuf = gtk4::gdk_pixbuf::Pixbuf::from_file_at_scale(
        path,
        width.saturating_mul(2),
        height.saturating_mul(2),
        true,
    )?;
    let bytes = pixbuf.read_pixel_bytes();
    Ok(DecodedPixels {
        bytes: bytes.as_ref().to_vec(),
        width: pixbuf.width(),
        height: pixbuf.height(),
        rowstride: pixbuf.rowstride() as usize,
        has_alpha: pixbuf.has_alpha(),
    })
}

pub(super) fn memory_texture(pixels: DecodedPixels) -> gtk4::gdk::Texture {
    let format = if pixels.has_alpha {
        gtk4::gdk::MemoryFormat::R8g8b8a8
    } else {
        gtk4::gdk::MemoryFormat::R8g8b8
    };
    let bytes = gtk4::glib::Bytes::from_owned(pixels.bytes);
    gtk4::gdk::MemoryTexture::new(
        pixels.width,
        pixels.height,
        format,
        &bytes,
        pixels.rowstride,
    )
    .upcast()
}

pub(super) fn cached_texture(
    url: &str,
    width: i32,
    height: i32,
    cache_scope: CacheScope,
) -> Option<gtk4::gdk::Texture> {
    TEXTURE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let index =
            cache
                .iter()
                .position(|(cached, cached_width, cached_height, cached_scope, _)| {
                    cached == url
                        && *cached_width == width
                        && *cached_height == height
                        && *cached_scope == cache_scope
                })?;
        let entry = cache.remove(index)?;
        let texture = entry.4.clone();
        cache.push_front(entry);
        Some(texture)
    })
}

pub(in crate::ui::podcasts) fn remember_texture(
    url: String,
    width: i32,
    height: i32,
    cache_scope: CacheScope,
    texture: gtk4::gdk::Texture,
) {
    TEXTURE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(index) =
            cache
                .iter()
                .position(|(cached, cached_width, cached_height, cached_scope, _)| {
                    cached == &url
                        && *cached_width == width
                        && *cached_height == height
                        && *cached_scope == cache_scope
                })
        {
            cache.remove(index);
        }
        cache.push_front((url, width, height, cache_scope, texture));
        cache.truncate(CACHE_LIMIT);
    });
}
