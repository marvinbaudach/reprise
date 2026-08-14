use std::io::Cursor;

use super::{thumbnail_with_source, CoverSource, ThumbnailSize};

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
