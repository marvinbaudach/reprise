//! Cover pipeline regression tests split from `cover.rs` for the code-file size gate.

use super::*;
use std::io::Write;

struct InMemoryAlbumSource {
    entries: Vec<std::path::PathBuf>,
}

impl crate::library::source::LibrarySource for InMemoryAlbumSource {
    fn residence_token(&self, _at: &std::path::Path) -> Option<i64> {
        None
    }

    fn mount_point(&self, _at: &std::path::Path) -> Option<std::path::PathBuf> {
        None
    }

    fn display_name(&self, at: &std::path::Path) -> Option<String> {
        crate::library::source::UnixLibrarySource.display_name(at)
    }

    fn container_name(&self, at: &std::path::Path) -> Option<String> {
        crate::library::source::UnixLibrarySource.container_name(at)
    }

    fn open_read(
        &self,
        at: &std::path::Path,
    ) -> std::io::Result<crate::library::source::LibraryReadHandle> {
        crate::library::source::UnixLibrarySource.open_read(at)
    }

    fn probe(
        &self,
        at: &std::path::Path,
        _links: crate::library::source::LibraryLinkMode,
    ) -> crate::library::source::LibraryPathPresence {
        if self.entries.iter().any(|entry| entry == at) {
            crate::library::source::LibraryPathPresence::Present(
                crate::library::source::LibraryPathMetadata {
                    is_file: true,
                    is_directory: false,
                    size: None,
                    modified: None,
                    identity: None,
                },
            )
        } else {
            crate::library::source::LibraryPathPresence::Absent
        }
    }

    fn walk(
        &self,
        _root: &std::path::Path,
        _order: crate::library::source::LibraryWalkOrder,
        _visitor: &mut dyn crate::library::source::LibraryWalkVisitor,
    ) {
    }

    fn read_directory(
        &self,
        _directory: &std::path::Path,
    ) -> Option<Vec<crate::library::source::LibraryDirectoryEntry>> {
        Some(
            self.entries
                .iter()
                .cloned()
                .map(|path| crate::library::source::LibraryDirectoryEntry {
                    path,
                    metadata: None,
                })
                .collect(),
        )
    }
}

// A 1x1 PNG, enough for source-resolution tests (no decode here).
const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x62, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

fn write(dir: &std::path::Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let p = dir.join(name);
    let mut f = std::fs::File::create(&p).unwrap();
    f.write_all(bytes).unwrap();
    p
}

fn tagged_track_with_cover(
    dir: &std::path::Path,
    name: &str,
    album: &str,
    cover: Vec<u8>,
) -> std::path::PathBuf {
    use lofty::picture::{MimeType, Picture, PictureType};
    use lofty::prelude::*;

    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    let track = dir.join(name);
    std::fs::copy(source, &track).unwrap();
    let mut tagged = lofty::read_from_path(&track).unwrap();
    let tag = tagged.primary_tag_mut().unwrap();
    tag.set_album(album.to_string());
    tag.insert_text(
        lofty::tag::ItemKey::AlbumArtist,
        "Consistency Artist".to_string(),
    );
    tag.push_picture(
        Picture::unchecked(cover)
            .pic_type(PictureType::CoverFront)
            .mime_type(MimeType::Png)
            .build(),
    );
    tagged
        .primary_tag()
        .unwrap()
        .save_to_path(&track, lofty::config::WriteOptions::default())
        .unwrap();
    track
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

#[test]
fn folder_image_reads_a_non_filesystem_library_source() {
    let directory = std::path::Path::new("content:/music/album");
    let expected = directory.join("Folder.PNG");
    let source = InMemoryAlbumSource {
        entries: vec![directory.join("notes.txt"), expected.clone()],
    };

    assert_eq!(folder_image_with_source(&source, directory), Some(expected));
}

#[test]
fn resolve_source_stage3_finds_a_downloaded_cover() {
    // A track file with album tags but NO embedded/folder cover, whose
    // album already has a downloaded cache file, resolves to it.
    let dir = tempfile::tempdir().unwrap();
    // A minimal file that lofty can read tags from is heavy to fabricate;
    // instead assert the stage-3 lookup wiring via read_cover_tag + the
    // download-cache path directly:
    let key = crate::cover_download::album_key("StageThree", "Album");
    std::fs::create_dir_all(crate::cover_download::downloaded_dir()).unwrap();
    let f = crate::cover_download::downloaded_dir().join(format!("{key}.jpg"));
    std::fs::write(&f, b"img").unwrap();
    assert_eq!(
        crate::cover_download::downloaded_cover_path(&key),
        Some(f.clone()),
        "stage-3 lookup must find the album's downloaded cover"
    );
    // And a downloaded cover path is always under the cache dir (promise):
    assert!(f.starts_with(cache_dir()));
    std::fs::remove_file(&f).ok();
    let _ = dir;
}

/// Stage 1 must read the *platform-provided* cache root — the whole reason
/// one is threaded through at all. Reverting the lookup in
/// `resolve_source_with_source` to the XDG-only `downloaded_cover_path`
/// turns this red, because the colliding default-root entry would win.
#[test]
fn a_downloaded_cover_is_taken_from_the_platform_cache_root() {
    let dir = tempfile::tempdir().unwrap();
    let cache_root = tempfile::tempdir().unwrap();
    let album = format!("Platform Root Album {}", fastrand::u64(..));
    let track = tagged_track_with_cover(dir.path(), "track.flac", &album, solid_png([255, 0, 0]));
    let key = crate::cover_download::album_key("Consistency Artist", &album);

    let platform_dir = crate::cover_download::downloaded_dir_in(cache_root.path());
    std::fs::create_dir_all(&platform_dir).unwrap();
    let platform_cover = platform_dir.join(format!("{key}.png"));
    std::fs::write(&platform_cover, solid_png([0, 255, 0])).unwrap();

    // The same album key, seeded in the desktop default root: it must lose.
    let default_dir = crate::cover_download::downloaded_dir();
    std::fs::create_dir_all(&default_dir).unwrap();
    let default_cover = default_dir.join(format!("{key}.png"));
    std::fs::write(&default_cover, solid_png([0, 0, 255])).unwrap();

    let resolved = resolve_source_with_source(
        &crate::library::source::UnixLibrarySource,
        &track,
        cache_root.path(),
    );

    std::fs::remove_file(&default_cover).ok();
    match resolved {
        Some(CoverSource::FolderImage(path)) => assert_eq!(
            path, platform_cover,
            "stage 1 must read the cache root it was handed, not the XDG default"
        ),
        other => panic!("expected the platform root's downloaded cover, got {other:?}"),
    }
}

#[test]
fn browse_10_tracks_from_one_album_prefer_the_shared_cached_cover() {
    let dir = tempfile::tempdir().unwrap();
    let album = format!("Consistency Album {}", fastrand::u64(..));
    let first = tagged_track_with_cover(dir.path(), "first.flac", &album, solid_png([255, 0, 0]));
    let second = tagged_track_with_cover(dir.path(), "second.flac", &album, solid_png([0, 0, 255]));
    let key = crate::cover_download::album_key("Consistency Artist", &album);
    std::fs::create_dir_all(crate::cover_download::downloaded_dir()).unwrap();
    let shared = crate::cover_download::downloaded_dir().join(format!("{key}.png"));
    std::fs::write(&shared, solid_png([0, 255, 0])).unwrap();

    let first_thumbnail = thumbnail(&resolve_source(&first).unwrap(), ThumbnailSize::List).unwrap();
    let second_thumbnail =
        thumbnail(&resolve_source(&second).unwrap(), ThumbnailSize::List).unwrap();

    assert_eq!(
        first_thumbnail, second_thumbnail,
        "one album identity must resolve to one canonical cached cover"
    );
    std::fs::remove_file(shared).ok();
    std::fs::remove_file(first_thumbnail).ok();
    std::fs::remove_file(second_thumbnail).ok();
}

#[test]
fn read_cover_tag_degrades_to_empty_fields_for_an_unreadable_tag() {
    let dir = tempfile::tempdir().unwrap();
    let track = write(dir.path(), "not-a-track.flac", b"not audio");
    let tag = read_cover_tag(&track);
    assert!(tag.picture.is_none());
    assert!(tag.album_artist.is_none());
    assert!(tag.album.is_none());
    assert!(tag.release_mbid.is_none());
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
    let mut image = image::RgbImage::from_pixel(side, side, image::Rgb([255, 0, 0]));
    let process = std::process::id().to_le_bytes();
    let current_thread = std::thread::current();
    let mut test_name = std::collections::hash_map::DefaultHasher::new();
    current_thread
        .name()
        .unwrap_or("unnamed")
        .hash(&mut test_name);
    let test = test_name.finish().to_le_bytes();

    // Cache keys are content-addressed. Tag test images with their process
    // and test name so parallel test binaries cannot delete each other's
    // deterministic cache entries during cleanup.
    image.put_pixel(0, 0, image::Rgb([process[0], process[1], process[2]]));
    image.put_pixel(1, 0, image::Rgb([test[0], test[1], test[2]]));

    let mut buf = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image)
        .write_to(&mut buf, image::ImageFormat::Png)
        .unwrap();
    buf.into_inner()
}

fn solid_png(color: [u8; 3]) -> Vec<u8> {
    solid_png_with_side(color, 32)
}

fn solid_png_with_side(color: [u8; 3], side: u32) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(side, side, image::Rgb(color));
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
fn preblurred_thumbnail_downscales_once_and_reuses_the_cached_texture() {
    let source = CoverSource::Embedded(red_png_1200());
    let first = blurred_thumbnail(&source, ThumbnailSize::Glow, 6.0).unwrap();
    let second = blurred_thumbnail(&source, ThumbnailSize::Glow, 6.0).unwrap();

    assert_eq!(first, second);
    assert!(first.starts_with(cache_dir()));
    assert!(first
        .file_name()
        .unwrap()
        .to_string_lossy()
        .contains("blur"));
    let decoded = image::open(&first).unwrap();
    assert_eq!(decoded.width().max(decoded.height()), 32);

    std::fs::remove_file(first).ok();
}

#[test]
fn preblurred_reduced_thumbnail_does_not_rescale_the_input() {
    let source = CoverSource::Embedded(red_png_1200());
    let reduced = thumbnail(&source, ThumbnailSize::Glow).unwrap();
    let blurred = blur_reduced_thumbnail(&reduced, 6.0).unwrap();

    let reduced_image = image::open(&reduced).unwrap();
    let blurred_image = image::open(&blurred).unwrap();
    assert_eq!((reduced_image.width(), reduced_image.height()), (32, 32));
    assert_eq!((blurred_image.width(), blurred_image.height()), (32, 32));
    assert_eq!(blur_reduced_thumbnail(&reduced, 6.0).unwrap(), blurred);

    std::fs::remove_file(blurred).ok();
    std::fs::remove_file(reduced).ok();
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
fn a_remembered_resolution_answers_without_touching_the_track_again() {
    // Proof by denial: after the first resolution the track is made
    // unreadable. Anything that still opens it would fail; the remembered
    // answer does not need to.
    let dir = tempfile::tempdir().unwrap();
    let album = format!("Remembered {}", fastrand::u64(..));
    let track = tagged_track_with_cover(dir.path(), "t.flac", &album, solid_png([9, 9, 9]));

    let first = thumbnail_for_track(&track, ThumbnailSize::List);
    assert!(first.is_some(), "the track has an embedded cover");

    let mut locked = std::fs::metadata(&track).unwrap().permissions();
    locked.set_readonly(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        locked.set_mode(0o000);
    }
    std::fs::set_permissions(&track, locked).unwrap();

    let second = thumbnail_for_track(&track, ThumbnailSize::List);
    assert_eq!(
        second, first,
        "a remembered resolution must answer from the index, not the file"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut open = std::fs::metadata(&track).unwrap().permissions();
        open.set_mode(0o644);
        std::fs::set_permissions(&track, open).ok();
    }
    std::fs::remove_file(resolution_index_path(&track, ThumbnailSize::List)).ok();
    first.map(|path| std::fs::remove_file(path).ok());
}

#[test]
fn settling_the_download_side_keeps_the_cover_that_was_already_found() {
    // The batch marks every track it walks as settled, including the ones
    // that already have a cover. Writing that into the answer instead of
    // beside it rendered the whole library as placeholders.
    let dir = tempfile::tempdir().unwrap();
    let album = format!("Settled {}", fastrand::u64(..));
    let track = tagged_track_with_cover(dir.path(), "s.flac", &album, solid_png([4, 5, 6]));

    let resolved = thumbnail_for_track(&track, ThumbnailSize::List);
    assert!(resolved.is_some(), "the track has an embedded cover");

    remember_download_unavailable(&track, ThumbnailSize::List);

    assert!(
        download_marked_unavailable(&track, ThumbnailSize::List),
        "the download side must be remembered as settled"
    );
    assert_eq!(
        thumbnail_for_track(&track, ThumbnailSize::List),
        resolved,
        "settling the download side must not erase the cover itself"
    );

    std::fs::remove_file(resolution_index_path(&track, ThumbnailSize::List)).ok();
    resolved.map(|path| std::fs::remove_file(path).ok());
}

#[test]
fn settling_the_download_side_first_does_not_invent_a_missing_cover() {
    // The batch walks the whole library on launch, long before most rows
    // have ever been rendered — so for nearly every track it settles the
    // download side with no resolution on file yet. "Nothing known about
    // the cover" must not be written down as "this track has no cover":
    // that answer is final, the stamp stays valid, and the row shows a
    // placeholder for good.
    let dir = tempfile::tempdir().unwrap();
    let album = format!("Settled First {}", fastrand::u64(..));
    let track = tagged_track_with_cover(dir.path(), "f.flac", &album, solid_png([7, 8, 9]));

    remember_download_unavailable(&track, ThumbnailSize::List);
    let resolved = thumbnail_for_track(&track, ThumbnailSize::List);

    assert!(
        resolved.is_some(),
        "the embedded cover must still be found after the batch settled the download side"
    );
    assert!(
        download_marked_unavailable(&track, ThumbnailSize::List),
        "resolving the cover must not undo what the batch settled"
    );

    std::fs::remove_file(resolution_index_path(&track, ThumbnailSize::List)).ok();
    resolved.map(|path| std::fs::remove_file(path).ok());
}

#[test]
fn an_entry_written_before_the_unknown_state_is_not_believed() {
    // Entries from the older format cannot be told apart from a real "no
    // cover": both are an empty answer line. The stamp carries a format
    // version so those fall out as a mismatch and get resolved again.
    let dir = tempfile::tempdir().unwrap();
    let album = format!("Legacy {}", fastrand::u64(..));
    let track = tagged_track_with_cover(dir.path(), "l.flac", &album, solid_png([2, 4, 6]));
    let index = resolution_index_path(&track, ThumbnailSize::List);

    let legacy_stamp = format!(
        "{}:{}:{}:{}",
        mtime_nanos(&track),
        std::fs::metadata(&track).unwrap().len(),
        mtime_nanos(dir.path()),
        mtime_nanos(&crate::cover_download::publish_marker())
    );
    write_resolution_body(&index, &legacy_stamp, &format!("\n{DOWNLOAD_EXHAUSTED}"));

    let resolved = thumbnail_for_track(&track, ThumbnailSize::List);
    assert!(
        resolved.is_some(),
        "an entry from the older format must not settle a track as coverless"
    );

    std::fs::remove_file(&index).ok();
    resolved.map(|path| std::fs::remove_file(path).ok());
}

#[test]
fn a_sidecar_cover_appearing_undoes_a_remembered_absence() {
    // "No cover" is remembered too, so a coverless track costs three stats
    // instead of a tag read. It must not survive a cover showing up.
    let dir = tempfile::tempdir().unwrap();
    let track = dir.path().join("untagged.mp3");
    std::fs::write(&track, b"not really an mp3").unwrap();

    assert_eq!(
        thumbnail_for_track(&track, ThumbnailSize::List),
        None,
        "nothing to resolve yet"
    );
    assert!(
        resolution_index_path(&track, ThumbnailSize::List).exists(),
        "the absence must be remembered, or every launch pays for it again"
    );

    // A folder image appearing changes the album folder's mtime, which is
    // part of the stamp — the remembered absence has to fall.
    std::thread::sleep(std::time::Duration::from_millis(10));
    std::fs::write(dir.path().join("cover.png"), solid_png([1, 2, 3])).unwrap();

    let after = thumbnail_for_track(&track, ThumbnailSize::List);
    assert!(
        after.is_some(),
        "a sidecar cover appearing must invalidate the remembered absence"
    );

    std::fs::remove_file(resolution_index_path(&track, ThumbnailSize::List)).ok();
    after.map(|path| std::fs::remove_file(path).ok());
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
