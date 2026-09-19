use std::io::Cursor;

use super::{resolve_source_with_source, thumbnail_with_source, CoverSource, ThumbnailSize};

fn solid_png(color: [u8; 3]) -> Vec<u8> {
    let image = image::RgbImage::from_pixel(32, 32, image::Rgb(color));
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}

#[test]
fn source_aware_mobile_thumbnail_uses_the_platform_cache_root() {
    let cache_root = tempfile::tempdir().unwrap();
    let source = CoverSource::Embedded(solid_png([26, 82, 118]));

    let path = thumbnail_with_source(
        &crate::library::source::UnixLibrarySource,
        &source,
        ThumbnailSize::MobileList,
        cache_root.path(),
    )
    .unwrap();

    assert!(path.starts_with(cache_root.path().join("reprise/covers")));
    assert!(path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .ends_with(&format!("-{}.png", 56 * 3)));
}

#[test]
fn mobile_full_thumbnail_uses_the_measured_three_x_rung() {
    assert_eq!(ThumbnailSize::MobileFull.pixels(), 1_092);
}

#[test]
fn mobile_portrait_thumbnail_is_the_measured_210_dp_rung() {
    assert_eq!(ThumbnailSize::MobilePortrait.pixels(), 640);
}

#[test]
fn mobile_portrait_thumbnails_land_in_the_platform_cache_root() {
    let cache_root = tempfile::tempdir().unwrap();
    let source = CoverSource::Embedded(solid_png([26, 82, 118]));

    let path = thumbnail_with_source(
        &crate::library::source::UnixLibrarySource,
        &source,
        ThumbnailSize::MobilePortrait,
        cache_root.path(),
    )
    .unwrap();

    assert!(path.starts_with(cache_root.path().join("reprise/covers")));
    assert!(path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .ends_with("-640.png"));
}

/// A document-provider library the way Android's SAF adapter behaves: every
/// path below the granted tree maps to a real file, and every path outside
/// it — the app-private cache included — is refused with the provider's own
/// "No content provider" error, never served from the filesystem.
struct DocumentTreeSource {
    tree: std::path::PathBuf,
    files: Vec<(std::path::PathBuf, std::path::PathBuf)>,
    refused: std::sync::Mutex<Vec<std::path::PathBuf>>,
}

impl DocumentTreeSource {
    fn refuse(&self, at: &std::path::Path) -> std::io::Error {
        self.refused.lock().unwrap().push(at.to_path_buf());
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("No content provider: {}", at.display()),
        )
    }

    fn backing_file(&self, at: &std::path::Path) -> Option<&std::path::Path> {
        self.files
            .iter()
            .find(|(document, _)| document == at)
            .map(|(_, file)| file.as_path())
    }
}

impl crate::library::source::LibrarySource for DocumentTreeSource {
    fn residence_token(&self, _at: &std::path::Path) -> Option<i64> {
        None
    }

    fn mount_point(&self, _at: &std::path::Path) -> Option<std::path::PathBuf> {
        None
    }

    fn display_name(&self, at: &std::path::Path) -> Option<String> {
        at.file_name()
            .map(|name| name.to_string_lossy().into_owned())
    }

    fn container_name(&self, _at: &std::path::Path) -> Option<String> {
        None
    }

    fn relative_path(
        &self,
        root: &std::path::Path,
        at: &std::path::Path,
    ) -> Option<std::path::PathBuf> {
        at.strip_prefix(root).ok().map(std::path::Path::to_path_buf)
    }

    fn open_read(
        &self,
        at: &std::path::Path,
    ) -> std::io::Result<crate::library::source::LibraryReadHandle> {
        if !at.starts_with(&self.tree) {
            return Err(self.refuse(at));
        }
        let Some(file) = self.backing_file(at) else {
            return Err(self.refuse(at));
        };
        std::fs::File::open(file).map(crate::library::source::LibraryReadHandle::new)
    }

    fn probe(
        &self,
        at: &std::path::Path,
        _links: crate::library::source::LibraryLinkMode,
    ) -> crate::library::source::LibraryPathPresence {
        if self.backing_file(at).is_some() {
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
            crate::library::source::LibraryPathPresence::Unknown
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
        directory: &std::path::Path,
    ) -> Option<Vec<crate::library::source::LibraryDirectoryEntry>> {
        directory.starts_with(&self.tree).then(|| {
            self.files
                .iter()
                .filter(|(document, _)| document.parent() == Some(directory))
                .map(
                    |(document, _)| crate::library::source::LibraryDirectoryEntry {
                        path: document.clone(),
                        metadata: None,
                    },
                )
                .collect()
        })
    }
}

/// A FLAC with album tags and no embedded picture, so the only cover the
/// resolver can find is the downloaded one.
fn tagged_track_without_picture(dir: &std::path::Path, album: &str) -> std::path::PathBuf {
    use lofty::prelude::*;

    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    let track = dir.join("track.flac");
    std::fs::copy(fixture, &track).unwrap();
    let mut tagged = lofty::read_from_path(&track).unwrap();
    let tag = tagged.primary_tag_mut().unwrap();
    tag.set_album(album.to_string());
    tag.insert_text(lofty::tag::ItemKey::AlbumArtist, "Cache Artist".to_string());
    tagged
        .primary_tag()
        .unwrap()
        .save_to_path(&track, lofty::config::WriteOptions::default())
        .unwrap();
    track
}

/// Issue #995: a downloaded cover lives in the app-private cache, not in the
/// library tree. Reading it through the document-provider source fails with
/// "No content provider" on Android, so the placeholder stayed on screen even
/// though the file was there. The cache is plain file I/O on every platform:
/// the provider must never be asked for it.
#[test]
fn a_downloaded_cover_is_read_from_the_cache_not_through_the_document_provider() {
    let library = tempfile::tempdir().unwrap();
    let cache_root = tempfile::tempdir().unwrap();
    let album = format!("Provider Album {}", fastrand::u64(..));
    let backing = tagged_track_without_picture(library.path(), &album);
    let tree = std::path::PathBuf::from("content:/tree/primary%3AMusic");
    let document = tree.join("Cache Artist").join("track.flac");
    let source = DocumentTreeSource {
        tree,
        files: vec![(document.clone(), backing)],
        refused: std::sync::Mutex::new(Vec::new()),
    };

    let key = crate::cover_download::album_key("Cache Artist", &album);
    let downloaded_dir = crate::cover_download::downloaded_dir_in(cache_root.path());
    std::fs::create_dir_all(&downloaded_dir).unwrap();
    let downloaded = downloaded_dir.join(format!("{key}.png"));
    std::fs::write(&downloaded, solid_png([200, 40, 40])).unwrap();

    let resolved = resolve_source_with_source(&source, &document, cache_root.path())
        .expect("the downloaded cover resolves for the document-tree track");
    let thumbnail = thumbnail_with_source(
        &source,
        &resolved,
        ThumbnailSize::MobileList,
        cache_root.path(),
    )
    .expect("the downloaded cover is thumbnailed without asking the provider");

    assert!(thumbnail.starts_with(cache_root.path().join("reprise/covers")));
    let refused = source.refused.lock().unwrap().clone();
    assert!(
        !refused.iter().any(|path| path == &downloaded),
        "the provider was asked for the cache file: {refused:?}"
    );
}
