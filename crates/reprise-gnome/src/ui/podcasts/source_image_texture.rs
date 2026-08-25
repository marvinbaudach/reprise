//! Decoded source-artwork pixels and the main-thread texture cache.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use gtk4::prelude::*;
use reprise_core::cover::{CoverError, CoverSource, ThumbnailSize};
use reprise_core::remote_image::CacheScope;

const CACHE_LIMIT: usize = 128;

struct TextureCacheEntry {
    url: String,
    width: i32,
    height: i32,
    cache_scope: CacheScope,
    texture: gtk4::gdk::Texture,
}

thread_local! {
    static TEXTURE_CACHE: RefCell<VecDeque<TextureCacheEntry>> =
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
    decode_pixels_with_thumbnail(path, width, height, reprise_core::cover::thumbnail)
}

fn decode_pixels_with_thumbnail(
    path: &Path,
    width: i32,
    height: i32,
    resolve_thumbnail: impl FnOnce(&CoverSource, ThumbnailSize) -> Result<PathBuf, CoverError>,
) -> Result<DecodedPixels, gtk4::glib::Error> {
    let requested_edge = width.max(height).max(0).saturating_mul(2) as u32;
    let size = [
        ThumbnailSize::List,
        ThumbnailSize::Bar,
        ThumbnailSize::Portrait,
        ThumbnailSize::Grid,
        ThumbnailSize::Full,
    ]
    .into_iter()
    .find(|size| size.pixels() >= requested_edge)
    .ok_or_else(|| {
        gtk4::glib::Error::new(
            gtk4::gio::IOErrorEnum::Failed,
            &format!(
                "source artwork request of {requested_edge} pixels exceeds the largest desktop thumbnail"
            ),
        )
    })?;
    let thumbnail_path = resolve_thumbnail(&CoverSource::FolderImage(path.to_path_buf()), size)
        .map_err(|error| {
            gtk4::glib::Error::new(gtk4::gio::IOErrorEnum::Failed, &error.to_string())
        })?;
    let pixbuf = gtk4::gdk_pixbuf::Pixbuf::from_file_at_scale(
        thumbnail_path,
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
        let index = cache.iter().position(|entry| {
            entry.url == url
                && entry.width == width
                && entry.height == height
                && entry.cache_scope == cache_scope
        })?;
        let entry = cache.remove(index)?;
        let texture = entry.texture.clone();
        cache.push_front(entry);
        Some(texture)
    })
}

pub(super) fn cached_texture_at_any_size(
    url: &str,
    cache_scope: CacheScope,
) -> Option<gtk4::gdk::Texture> {
    TEXTURE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let index = cache
            .iter()
            .position(|entry| entry.url == url && entry.cache_scope == cache_scope)?;
        let entry = cache.remove(index)?;
        let texture = entry.texture.clone();
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
        if let Some(index) = cache.iter().position(|entry| {
            entry.url == url
                && entry.width == width
                && entry.height == height
                && entry.cache_scope == cache_scope
        }) {
            cache.remove(index);
        }
        cache.push_front(TextureCacheEntry {
            url,
            width,
            height,
            cache_scope,
            texture,
        });
        cache.truncate(CACHE_LIMIT);
    });
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use gtk4::prelude::*;
    use reprise_core::cover::{CoverSource, ThumbnailSize};

    #[test]
    fn source_artwork_uses_the_cached_thumbnail_for_texture_decode() {
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("original-must-not-be-opened.png");
        let cache_root = directory.path().join("cache");
        let source =
            gtk4::gdk_pixbuf::Pixbuf::new(gtk4::gdk_pixbuf::Colorspace::Rgb, true, 8, 1_200, 600)
                .unwrap();
        source.fill(0x336699ff);
        let source_bytes = source.save_to_bufferv("png", &[]).unwrap();
        let resolved_paths = RefCell::new(Vec::new());

        let load = || {
            super::decode_pixels_with_thumbnail(&original, 40, 40, |requested, size| {
                // The injected resolver stands in for `thumbnail()`'s required source
                // read/hash and runs the same Core thumbnail implementation over the
                // large fixture bytes. The path itself deliberately does not exist: a
                // Pixbuf regression back to that path must fail instead of accidentally
                // decoding the resolver's cached PNG.
                assert!(matches!(
                    requested,
                    CoverSource::FolderImage(path) if path == &original
                ));
                assert_eq!(size, ThumbnailSize::Bar);
                let path = reprise_core::cover::thumbnail_with_source(
                    &reprise_core::library::source::UnixLibrarySource,
                    &CoverSource::Embedded(source_bytes.clone()),
                    size,
                    &cache_root,
                )?;
                resolved_paths.borrow_mut().push(path.clone());
                Ok(path)
            })
        };

        let first = load().unwrap();
        let thumbnail_path = resolved_paths.borrow()[0].clone();
        assert!(thumbnail_path.exists());
        assert!(thumbnail_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .ends_with("-96.png"));
        assert!(!original.exists());

        let second = load().unwrap();
        let texture = super::memory_texture(second);

        assert_eq!(resolved_paths.borrow()[1], thumbnail_path);
        assert_eq!((first.width, first.height), (80, 40));
        assert_eq!((texture.width(), texture.height()), (80, 40));
        assert!(!original.exists());
    }
}
