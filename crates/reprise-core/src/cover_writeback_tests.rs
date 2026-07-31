use std::io::Cursor;

use tempfile::TempDir;

use super::*;
use crate::cover::{resolve_source, CoverSource};

fn image_bytes(format: image::ImageFormat) -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(2, 2)
        .write_to(&mut bytes, format)
        .unwrap();
    bytes.into_inner()
}

#[test]
fn cover_1_download_without_a_folder_image_writes_cover_jpg_and_resolves_it() {
    let album = TempDir::new().unwrap();
    let track = album.path().join("song.flac");
    std::fs::write(&track, b"fixture").unwrap();
    let target = album.path().join("cover.jpg");

    assert_eq!(
        write_album_cover(
            &[album.path().to_path_buf()],
            &image_bytes(image::ImageFormat::Jpeg),
            "jpg"
        ),
        vec![CoverWrite::Written(target.clone())]
    );
    assert!(target.is_file());
    assert!(matches!(
        resolve_source(&track),
        Some(CoverSource::FolderImage(path)) if path == target
    ));
}

#[test]
fn cover_1_every_known_folder_image_name_prevents_a_write() {
    let bytes = image_bytes(image::ImageFormat::Jpeg);
    for stem in ["cover", "folder", "front", "album"] {
        for extension in ["jpg", "jpeg", "png", "webp", "gif", "bmp"] {
            let album = TempDir::new().unwrap();
            let existing = album.path().join(format!("{stem}.{extension}"));
            std::fs::write(&existing, b"user image").unwrap();

            assert_eq!(
                write_album_cover(&[album.path().to_path_buf()], &bytes, "jpg"),
                vec![CoverWrite::AlreadyPresent],
                "{stem}.{extension} must block cover.jpg"
            );
            assert_eq!(std::fs::read(&existing).unwrap(), b"user image");
            if existing != album.path().join("cover.jpg") {
                assert!(!album.path().join("cover.jpg").exists());
            }
        }
    }
}

#[test]
fn cover_1_an_album_spanning_two_directories_gets_one_cover_in_each() {
    let first = TempDir::new().unwrap();
    let second = TempDir::new().unwrap();
    let bytes = image_bytes(image::ImageFormat::Png);
    let first_target = first.path().join("cover.png");
    let second_target = second.path().join("cover.png");

    assert_eq!(
        write_album_cover(
            &[first.path().to_path_buf(), second.path().to_path_buf()],
            &bytes,
            "png"
        ),
        vec![
            CoverWrite::Written(first_target.clone()),
            CoverWrite::Written(second_target.clone())
        ]
    );
    assert_eq!(std::fs::read(first_target).unwrap(), bytes);
    assert_eq!(std::fs::read(second_target).unwrap(), bytes);
}

#[cfg(unix)]
#[test]
fn cover_1_a_write_failure_is_silent_and_leaves_no_temporary_file() {
    use std::os::unix::fs::PermissionsExt;

    let album = TempDir::new().unwrap();
    std::fs::set_permissions(album.path(), std::fs::Permissions::from_mode(0o555)).unwrap();

    let result = write_album_cover(
        &[album.path().to_path_buf()],
        &image_bytes(image::ImageFormat::Png),
        "png",
    );

    std::fs::set_permissions(album.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(result, vec![CoverWrite::Failed]);
    assert!(std::fs::read_dir(album.path()).unwrap().next().is_none());
}

#[test]
fn cover_1_release_group_cover_without_local_album_directories_writes_nothing() {
    assert!(write_album_cover(&[], &image_bytes(image::ImageFormat::Png), "png").is_empty());
}

#[test]
fn cover_1_a_missing_album_directory_is_not_an_applicable_target() {
    let parent = TempDir::new().unwrap();
    let missing = parent.path().join("missing-album");

    assert_eq!(
        write_album_cover(
            std::slice::from_ref(&missing),
            &image_bytes(image::ImageFormat::Png),
            "png"
        ),
        vec![CoverWrite::NotApplicable]
    );
    assert!(!missing.exists());
}
