//! Cover-art resolution and thumbnailing (portable, GUI-free). Reads covers
//! from the user's files (embedded picture, or a sidecar image in the album
//! folder) and produces cached thumbnails under the XDG cache dir. NEVER
//! writes into the user's library — see `thumbnail`'s cache path.

use std::path::{Path, PathBuf};

/// Where a cover for a track comes from.
#[derive(Debug)]
pub enum CoverSource {
    /// A picture embedded in the audio file (via lofty).
    Embedded(Vec<u8>),
    /// An image file sitting in the album folder (cover.*, folder.*).
    FolderImage(PathBuf),
}

/// Canonical sidecar cover file stems and extensions, in priority order.
const FOLDER_STEMS: &[&str] = &["cover", "folder", "front", "album"];
const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp"];

/// Resolves the best available cover source for a track: first an embedded
/// picture, otherwise the first matching sidecar image in the track's folder,
/// otherwise None. Pure read — touches no cache and writes nothing.
pub fn resolve_source(track_path: &Path) -> Option<CoverSource> {
    if let Some(bytes) = embedded_picture(track_path) {
        return Some(CoverSource::Embedded(bytes));
    }
    let dir = track_path.parent()?;
    folder_image(dir).map(CoverSource::FolderImage)
}

/// Reads the first embedded picture's bytes from an audio file, if any.
fn embedded_picture(track_path: &Path) -> Option<Vec<u8>> {
    use lofty::prelude::*;
    let tagged = lofty::read_from_path(track_path).ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;
    let picture = tag.pictures().first()?;
    Some(picture.data().to_vec())
}

/// Finds a sidecar cover image in `dir` by canonical stem + known extension,
/// case-insensitively, deterministically (stem-then-ext priority).
fn folder_image(dir: &Path) -> Option<PathBuf> {
    let entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    for stem in FOLDER_STEMS {
        for ext in IMAGE_EXTS {
            for path in &entries {
                let matches_stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.eq_ignore_ascii_case(stem));
                let matches_ext = path
                    .extension()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.eq_ignore_ascii_case(ext));
                if matches_stem && matches_ext {
                    return Some(path.clone());
                }
            }
        }
    }
    None
}

use std::hash::{Hash, Hasher};

/// The three cached edge lengths — one per consumer (list row / player bar /
/// Now-Playing view). Exactly three (YAGNI).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbnailSize {
    List,
    Bar,
    Full,
}

impl ThumbnailSize {
    pub fn pixels(self) -> u32 {
        match self {
            ThumbnailSize::List => 48,
            ThumbnailSize::Bar => 96,
            ThumbnailSize::Full => 1024,
        }
    }
}

#[derive(Debug)]
pub enum CoverError {
    Decode(String),
    Io(String),
}

impl std::fmt::Display for CoverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoverError::Decode(m) => write!(f, "cover decode failed: {m}"),
            CoverError::Io(m) => write!(f, "cover cache I/O failed: {m}"),
        }
    }
}

impl std::error::Error for CoverError {}

/// The cover thumbnail cache directory: `<XDG cache>/reprise/covers`. NEVER a
/// path inside the user's library — this is the load-bearing half of the
/// "we don't touch your files" promise.
pub fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("reprise/covers")
}

/// Returns the cache path to a thumbnail of `source` at `size`, creating it if
/// missing: hash the source bytes -> cache hit? -> else decode, resize (aspect
/// preserved, longest side = size), write PNG atomically (temp + rename).
pub fn thumbnail(source: &CoverSource, size: ThumbnailSize) -> Result<PathBuf, CoverError> {
    let bytes = source_bytes(source)?;
    let key = hash_hex(&bytes);
    let dir = cache_dir();
    let out = dir.join(format!("{key}-{}.png", size.pixels()));
    if out.exists() {
        return Ok(out);
    }
    std::fs::create_dir_all(&dir).map_err(|e| CoverError::Io(e.to_string()))?;

    let decoded = image::load_from_memory(&bytes).map_err(|e| CoverError::Decode(e.to_string()))?;
    // Never upscale: clamp the target to the source's longest side. A source
    // smaller than the requested box is kept at native size (best available,
    // no blur) instead of being blown up by `image::thumbnail`, which does
    // upscale when the source is smaller than the target box.
    let longest = decoded.width().max(decoded.height());
    let target = size.pixels().min(longest);
    let thumb = decoded.thumbnail(target, target); // aspect-preserving

    // Atomic publish: write a UNIQUE temp file in the same dir, then rename.
    // Uniqueness matters because concurrent calls for the same cache key must
    // not race on one temp path (the loser would otherwise see a spurious
    // ENOENT on rename after the winner already unlinked it).
    let tmp = dir.join(format!(
        ".{key}-{}-{}.png.tmp",
        size.pixels(),
        fastrand::u64(..)
    ));
    if let Err(e) = thumb.save_with_format(&tmp, image::ImageFormat::Png) {
        let _ = std::fs::remove_file(&tmp);
        return Err(CoverError::Io(e.to_string()));
    }
    if let Err(e) = std::fs::rename(&tmp, &out) {
        let _ = std::fs::remove_file(&tmp);
        // A concurrent writer may have already published this exact key —
        // that's success, not an error.
        if out.exists() {
            return Ok(out);
        }
        return Err(CoverError::Io(e.to_string()));
    }
    Ok(out)
}

fn source_bytes(source: &CoverSource) -> Result<Vec<u8>, CoverError> {
    match source {
        CoverSource::Embedded(b) => Ok(b.clone()),
        CoverSource::FolderImage(p) => std::fs::read(p).map_err(|e| CoverError::Io(e.to_string())),
    }
}

/// Fast, non-cryptographic content hash (std DefaultHasher) over the source
/// bytes, hex-encoded. The key only needs to be deterministic on one machine
/// and collision-resistant enough for a cache — no crypto property required,
/// so no new hashing dependency.
pub(crate) fn hash_hex(bytes: &[u8]) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    format!("{:016x}", h.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // A 1x1 PNG, enough for source-resolution tests (no decode here).
    const TINY_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x62, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    fn write(dir: &std::path::Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let p = dir.join(name);
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(bytes).unwrap();
        p
    }

    #[test]
    fn returns_none_when_no_embedded_and_no_folder_image() {
        let dir = tempfile::tempdir().unwrap();
        let track = write(dir.path(), "song.flac", b"not audio, no tag");
        assert!(resolve_source(&track).is_none());
    }

    #[test]
    fn falls_back_to_folder_image_cover_jpg() {
        let dir = tempfile::tempdir().unwrap();
        let track = write(dir.path(), "song.flac", b"not audio");
        let cover = write(dir.path(), "cover.jpg", TINY_PNG); // content irrelevant here
        match resolve_source(&track) {
            Some(CoverSource::FolderImage(p)) => assert_eq!(p, cover),
            other => panic!("expected FolderImage, got {other:?}"),
        }
    }

    #[test]
    fn folder_image_matches_are_case_insensitive_and_prioritized() {
        // "Folder.PNG" must be found; the first canonical name wins deterministically.
        let dir = tempfile::tempdir().unwrap();
        let track = write(dir.path(), "song.flac", b"x");
        let _ = write(dir.path(), "Folder.PNG", TINY_PNG);
        assert!(matches!(
            resolve_source(&track),
            Some(CoverSource::FolderImage(_))
        ));
    }

    // A real, decodable 1200x1200 red PNG — larger than the biggest thumbnail
    // (1024 px) so every size exercises the DOWNSCALE path, and the exact-size
    // assertion below holds. (`image::thumbnail` itself CAN upscale a source
    // smaller than the target box; `thumbnail()` clamps against that, which
    // `small_source_is_not_upscaled` below covers.) Solid-color PNG encodes tiny.
    fn red_png_1200() -> Vec<u8> {
        red_png(1200)
    }

    // A solid-color square red PNG of the given side length.
    fn red_png(side: u32) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(side, side, image::Rgb([255, 0, 0]));
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    }

    #[test]
    fn thumbnail_produces_png_of_requested_size_and_caches_under_cache_dir() {
        let src = CoverSource::Embedded(red_png_1200());
        let path = thumbnail(&src, ThumbnailSize::List).unwrap();
        // Lands under the cache dir, NEVER in a track folder (core promise).
        assert!(
            path.starts_with(cache_dir()),
            "thumb must be in the cache dir"
        );
        assert_eq!(path.extension().unwrap(), "png");
        // Decodes back to a PNG whose largest side is the requested pixel count.
        let decoded = image::open(&path).unwrap();
        let (w, h) = (decoded.width(), decoded.height());
        assert!(w.max(h) == ThumbnailSize::List.pixels(), "got {w}x{h}");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn identical_bytes_hash_to_the_same_cache_path() {
        let a = thumbnail(&CoverSource::Embedded(red_png_1200()), ThumbnailSize::Bar).unwrap();
        let b = thumbnail(&CoverSource::Embedded(red_png_1200()), ThumbnailSize::Bar).unwrap();
        assert_eq!(a, b, "same source bytes + size -> same cache key");
        std::fs::remove_file(&a).ok();
    }

    #[test]
    fn different_sizes_get_distinct_cache_paths() {
        let bytes = red_png_1200();
        let list = thumbnail(&CoverSource::Embedded(bytes.clone()), ThumbnailSize::List).unwrap();
        let full = thumbnail(&CoverSource::Embedded(bytes), ThumbnailSize::Full).unwrap();
        assert_ne!(list, full);
        std::fs::remove_file(&list).ok();
        std::fs::remove_file(&full).ok();
    }

    #[test]
    fn corrupt_image_returns_error_never_panics() {
        let src = CoverSource::Embedded(b"definitely not an image".to_vec());
        assert!(matches!(
            thumbnail(&src, ThumbnailSize::List),
            Err(CoverError::Decode(_))
        ));
    }

    #[test]
    fn small_source_is_not_upscaled() {
        let src = CoverSource::Embedded(red_png(32));
        let path = thumbnail(&src, ThumbnailSize::Full).unwrap();
        let decoded = image::open(&path).unwrap();
        let (w, h) = (decoded.width(), decoded.height());
        assert_eq!(
            w.max(h),
            32,
            "small source must stay native size, got {w}x{h}"
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn concurrent_same_key_calls_all_succeed() {
        let bytes = red_png_1200();
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let bytes = bytes.clone();
                std::thread::spawn(move || {
                    thumbnail(&CoverSource::Embedded(bytes), ThumbnailSize::List)
                })
            })
            .collect();

        let results: Vec<Result<PathBuf, CoverError>> = handles
            .into_iter()
            .map(|h| h.join().expect("thread panicked"))
            .collect();

        for r in &results {
            assert!(r.is_ok(), "expected Ok, got {r:?}");
        }
        let first = results[0].as_ref().unwrap();
        for r in &results {
            assert_eq!(
                r.as_ref().unwrap(),
                first,
                "all callers must agree on the cache path"
            );
        }
        assert!(
            image::open(first).is_ok(),
            "final file must decode as a valid image"
        );
        std::fs::remove_file(first).ok();
    }
}
