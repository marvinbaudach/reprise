use super::{cache, deezer, fetch, release_cover, sidecar, wikimedia, PortraitOutcome};
use crate::musicbrainz::FetchError;

const PHOTO_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const PHOTO_SEARCH: &str = r#"{"data":[{"id":7,"name":"Band","nb_fan":2,"picture_xl":"https://cdn-images.dzcdn.net/images/artist/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/1000x1000.jpg"}]}"#;

fn png_bytes() -> Vec<u8> {
    let mut buffer = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(1, 1)
        .write_to(&mut buffer, image::ImageFormat::Png)
        .unwrap();
    buffer.into_inner()
}

fn stale_portrait(dir: &std::path::Path) -> std::path::PathBuf {
    cache::store_image(dir, "Band", b"stale", "jpg").unwrap()
}

#[test]
fn net_8_will_ramos_release_cover_is_recognized() {
    let search = r#"{"data":[{"id":12773623,"name":" Will   Ramos ","nb_fan":12,"picture_xl":"https://cdn-images.dzcdn.net/images/artist/6c36703da072ad07d31351b20208acf8/1000x1000.jpg"}]}"#;
    let albums = r#"{"data":[{"title":"Rain","md5_image":"6c36703da072ad07d31351b20208acf8"}]}"#;

    let artist = deezer::parse_best_artist(search, "Will Ramos").unwrap();
    let covers = release_cover::parse_release_cover_identifiers(albums).unwrap();

    assert_eq!(artist.id, 12_773_623);
    assert!(release_cover::is_release_cover(
        artist.picture_url.as_deref().unwrap(),
        &covers,
    ));
}

#[test]
fn net_8_deezer_error_object_is_not_an_empty_cover_list() {
    assert!(release_cover::parse_release_cover_identifiers(
        r#"{"error":{"type":"QuotaException","code":4,"message":"Quota limit exceeded"}}"#,
    )
    .is_none());
}

#[test]
fn net_8_albums_url_has_the_named_page_bound() {
    assert_eq!(
        release_cover::albums_url(12_773_623),
        "https://api.deezer.com/artist/12773623/albums?limit=1000"
    );
}

#[test]
fn net_8_real_photo_is_stored_as_deezer_without_credit() {
    let dir = tempfile::tempdir().unwrap();
    let mut deezer_text = |url: &str| {
        if url.contains("/albums?") {
            Ok(r#"{"data":[{"md5_image":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}]}"#.to_owned())
        } else {
            Ok(PHOTO_SEARCH.to_owned())
        }
    };
    let mut musicbrainz = |_: &str| panic!("Commons must not be consulted");
    let mut wikimedia = |_: &str| panic!("Commons must not be consulted");
    let mut image = |url: &str| {
        assert!(url.contains(PHOTO_ID));
        Ok(png_bytes())
    };
    let mut fetchers = fetch::Fetchers {
        deezer_text: &mut deezer_text,
        musicbrainz_text: &mut musicbrainz,
        wikimedia_text: &mut wikimedia,
        image: &mut image,
    };

    let outcome = fetch::load_or_fetch("Band", None, 1_000, dir.path(), &mut fetchers).unwrap();

    assert!(matches!(outcome, PortraitOutcome::Found(_)));
    assert_eq!(
        sidecar::read(dir.path(), "Band"),
        Some(sidecar::PortraitRecord::deezer())
    );
}

#[test]
fn net_8_album_transport_error_preserves_stale_portrait_without_writes() {
    let dir = tempfile::tempdir().unwrap();
    let stale = stale_portrait(dir.path());
    let now = cache::file_epoch_secs(&stale) + 31 * 24 * 60 * 60;
    let mut deezer_text = |url: &str| {
        if url.contains("/albums?") {
            Err(FetchError::Transport)
        } else {
            Ok(PHOTO_SEARCH.to_owned())
        }
    };
    let mut unused_musicbrainz = |_: &str| panic!("must stop at the album failure");
    let mut unused_wikimedia = |_: &str| panic!("must stop at the album failure");
    let mut image = |_: &str| panic!("must not download");
    let mut fetchers = fetch::Fetchers {
        deezer_text: &mut deezer_text,
        musicbrainz_text: &mut unused_musicbrainz,
        wikimedia_text: &mut unused_wikimedia,
        image: &mut image,
    };

    let outcome = fetch::load_or_fetch("Band", None, now, dir.path(), &mut fetchers).unwrap();

    assert!(matches!(outcome, PortraitOutcome::Found(path) if path == stale));
    assert_eq!(std::fs::read(stale).unwrap(), b"stale");
    assert!(!cache::negative_marker_path(dir.path(), "Band").exists());
}

#[test]
fn net_8_deezer_error_object_preserves_stale_portrait_without_writes() {
    let dir = tempfile::tempdir().unwrap();
    let stale = stale_portrait(dir.path());
    let now = cache::file_epoch_secs(&stale) + 31 * 24 * 60 * 60;
    let mut deezer_text = |url: &str| {
        if url.contains("/albums?") {
            Ok(r#"{"error":{"code":4,"message":"Quota"}}"#.to_owned())
        } else {
            Ok(PHOTO_SEARCH.to_owned())
        }
    };
    let mut unused_musicbrainz = |_: &str| panic!("must stop at the Deezer error object");
    let mut unused_wikimedia = |_: &str| panic!("must stop at the Deezer error object");
    let mut image = |_: &str| panic!("must not download");
    let mut fetchers = fetch::Fetchers {
        deezer_text: &mut deezer_text,
        musicbrainz_text: &mut unused_musicbrainz,
        wikimedia_text: &mut unused_wikimedia,
        image: &mut image,
    };

    let outcome = fetch::load_or_fetch("Band", None, now, dir.path(), &mut fetchers).unwrap();

    assert!(matches!(outcome, PortraitOutcome::Found(path) if path == stale));
    assert!(!cache::negative_marker_path(dir.path(), "Band").exists());
}

#[test]
fn net_8_rejected_cover_deletes_stale_portrait_before_commons_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let stale = stale_portrait(dir.path());
    let now = cache::file_epoch_secs(&stale) + 31 * 24 * 60 * 60;
    let mut deezer_text = |url: &str| {
        if url.contains("/albums?") {
            Ok(format!(r#"{{"data":[{{"md5_image":"{PHOTO_ID}"}}]}}"#))
        } else {
            Ok(PHOTO_SEARCH.to_owned())
        }
    };
    let mut musicbrainz = |_: &str| Ok(r#"{"artists":[]}"#.to_owned());
    let mut unused_wikimedia = |_: &str| panic!("no MBID means no Wikimedia request");
    let mut image = |_: &str| panic!("release cover must not download");
    let mut fetchers = fetch::Fetchers {
        deezer_text: &mut deezer_text,
        musicbrainz_text: &mut musicbrainz,
        wikimedia_text: &mut unused_wikimedia,
        image: &mut image,
    };

    let outcome = fetch::load_or_fetch("Band", None, now, dir.path(), &mut fetchers).unwrap();

    assert!(matches!(outcome, PortraitOutcome::NotFound));
    assert!(!stale.exists());
    assert!(sidecar::read(dir.path(), "Band").is_none());
    assert!(cache::negative_marker_path(dir.path(), "Band").exists());
}

#[test]
fn net_9_commons_credit_strips_html_from_the_author() {
    let json = r#"{"query":{"pages":{"1":{"imageinfo":[{"thumburl":"https://upload.wikimedia.org/example.jpg","descriptionurl":"https://commons.wikimedia.org/wiki/File:Example.jpg","extmetadata":{"Artist":{"value":"<a href=\"/wiki/User:Photo\">Photo Name</a>"},"LicenseShortName":{"value":"CC BY-SA 4.0"},"LicenseUrl":{"value":"https://creativecommons.org/licenses/by-sa/4.0"}}}]}}}}"#;

    let image = wikimedia::parse_commons_imageinfo(json).unwrap().unwrap();

    assert_eq!(image.credit.author, "Photo Name");
    assert_eq!(image.credit.license, "CC BY-SA 4.0");
}

#[test]
fn net_9_cached_image_without_sidecar_needs_fetch() {
    let dir = tempfile::tempdir().unwrap();
    let key = cache::key_for("Band");
    let image = dir.path().join(format!("{key}.jpg"));
    std::fs::write(&image, b"image").unwrap();
    let now = cache::file_epoch_secs(&image);

    assert!(matches!(
        cache::verdict(dir.path(), "Band", now),
        cache::CacheVerdict::NeedsFetch {
            stale_portrait: Some(path)
        } if path == image
    ));
}

#[test]
fn net_9_portrait_record_round_trips_with_credit() {
    let dir = tempfile::tempdir().unwrap();
    let credit = sidecar::PortraitCredit {
        author: "Photographer".to_owned(),
        license: "CC BY 4.0".to_owned(),
        license_url: "https://creativecommons.org/licenses/by/4.0".to_owned(),
        file_page_url: "https://commons.wikimedia.org/wiki/File:Portrait.jpg".to_owned(),
    };
    let record = sidecar::PortraitRecord::commons(credit.clone());

    cache::store_portrait(dir.path(), "Band", b"image", "jpg", &record).unwrap();

    assert_eq!(sidecar::load_credit(dir.path(), "Band"), Some(credit));
}

#[test]
fn net_9_mbid_hint_follows_commons_chain_and_stores_credit() {
    let dir = tempfile::tempdir().unwrap();
    let musicbrainz_urls = std::cell::RefCell::new(Vec::new());
    let wikimedia_urls = std::cell::RefCell::new(Vec::new());
    let mut deezer_text = |_: &str| Ok(r#"{"data":[]}"#.to_owned());
    let mut musicbrainz = |url: &str| {
        musicbrainz_urls.borrow_mut().push(url.to_owned());
        Ok(
            r#"{"relations":[{"url":{"resource":"https://www.wikidata.org/wiki/Q42"}}]}"#
                .to_owned(),
        )
    };
    let mut wikimedia_text = |url: &str| {
        wikimedia_urls.borrow_mut().push(url.to_owned());
        if url.contains("wbgetclaims") {
            Ok(
                r#"{"claims":{"P18":[{"mainsnak":{"datavalue":{"value":"Portrait.jpg"}}}]}}"#
                    .to_owned(),
            )
        } else {
            Ok(r#"{"query":{"pages":{"1":{"imageinfo":[{"thumburl":"https://upload.wikimedia.org/portrait.jpg","descriptionurl":"https://commons.wikimedia.org/wiki/File:Portrait.jpg","extmetadata":{"Artist":{"value":"<b>Photo Name</b>"},"LicenseShortName":{"value":"CC BY 4.0"},"LicenseUrl":{"value":"https://creativecommons.org/licenses/by/4.0"}}}]}}}}"#.to_owned())
        }
    };
    let mut image = |url: &str| {
        assert_eq!(url, "https://upload.wikimedia.org/portrait.jpg");
        Ok(png_bytes())
    };
    let mut fetchers = fetch::Fetchers {
        deezer_text: &mut deezer_text,
        musicbrainz_text: &mut musicbrainz,
        wikimedia_text: &mut wikimedia_text,
        image: &mut image,
    };

    let outcome = fetch::load_or_fetch(
        "Band",
        Some("artist-mbid"),
        1_000,
        dir.path(),
        &mut fetchers,
    )
    .unwrap();

    assert!(matches!(outcome, PortraitOutcome::Found(_)));
    assert_eq!(musicbrainz_urls.borrow().len(), 1);
    assert!(musicbrainz_urls.borrow()[0].contains("/artist/artist-mbid?inc=url-rels"));
    assert_eq!(wikimedia_urls.borrow().len(), 2);
    assert_eq!(
        sidecar::load_credit(dir.path(), "Band").unwrap().author,
        "Photo Name"
    );
}

#[test]
fn net_9_without_hint_uses_exact_name_musicbrainz_search() {
    let dir = tempfile::tempdir().unwrap();
    let urls = std::cell::RefCell::new(Vec::new());
    let mut deezer_text = |_: &str| Ok(r#"{"data":[]}"#.to_owned());
    let mut musicbrainz = |url: &str| {
        urls.borrow_mut().push(url.to_owned());
        if url.contains("query=") {
            Ok(r#"{"artists":[{"id":"exact-mbid","name":"Band","score":100}]}"#.to_owned())
        } else {
            Ok(r#"{"relations":[]}"#.to_owned())
        }
    };
    let mut unused_wikimedia = |_: &str| panic!("missing relation stops the chain");
    let mut image = |_: &str| panic!("missing relation has no image");
    let mut fetchers = fetch::Fetchers {
        deezer_text: &mut deezer_text,
        musicbrainz_text: &mut musicbrainz,
        wikimedia_text: &mut unused_wikimedia,
        image: &mut image,
    };

    let outcome = fetch::load_or_fetch("Band", None, 1_000, dir.path(), &mut fetchers).unwrap();

    assert!(matches!(outcome, PortraitOutcome::NotFound));
    assert_eq!(urls.borrow().len(), 2);
    assert!(urls.borrow()[0].contains("query=artist%3A%22Band%22"));
    assert!(urls.borrow()[1].contains("/artist/exact-mbid?inc=url-rels"));
}

#[test]
fn net_9_missing_relation_writes_negative_only_after_deezer_miss() {
    let dir = tempfile::tempdir().unwrap();
    let mut deezer_text = |_: &str| Ok(r#"{"data":[]}"#.to_owned());
    let mut musicbrainz = |_: &str| Ok(r#"{"relations":[]}"#.to_owned());
    let mut unused_wikimedia = |_: &str| panic!("missing relation stops the chain");
    let mut unused_image = |_: &str| panic!("missing relation has no image");
    let mut fetchers = fetch::Fetchers {
        deezer_text: &mut deezer_text,
        musicbrainz_text: &mut musicbrainz,
        wikimedia_text: &mut unused_wikimedia,
        image: &mut unused_image,
    };

    let outcome =
        fetch::load_or_fetch("Band", Some("mbid"), 1_000, dir.path(), &mut fetchers).unwrap();

    assert!(matches!(outcome, PortraitOutcome::NotFound));
    assert!(cache::negative_marker_path(dir.path(), "Band").exists());
}

#[test]
fn net_9_missing_p18_writes_negative_only_after_deezer_miss() {
    let dir = tempfile::tempdir().unwrap();
    let mut deezer_text = |_: &str| Ok(r#"{"data":[]}"#.to_owned());
    let mut musicbrainz = |_: &str| {
        Ok(
            r#"{"relations":[{"url":{"resource":"https://www.wikidata.org/wiki/Q42"}}]}"#
                .to_owned(),
        )
    };
    let mut wikimedia_text = |_: &str| Ok(r#"{"claims":{}}"#.to_owned());
    let mut image = |_: &str| panic!("missing P18 has no image");
    let mut fetchers = fetch::Fetchers {
        deezer_text: &mut deezer_text,
        musicbrainz_text: &mut musicbrainz,
        wikimedia_text: &mut wikimedia_text,
        image: &mut image,
    };

    let outcome =
        fetch::load_or_fetch("Band", Some("mbid"), 1_000, dir.path(), &mut fetchers).unwrap();

    assert!(matches!(outcome, PortraitOutcome::NotFound));
    assert!(cache::negative_marker_path(dir.path(), "Band").exists());
}

#[test]
fn net_9_transport_error_anywhere_in_chain_preserves_stale_without_negative_marker() {
    for failed_step in ["search", "relations", "claims", "imageinfo", "image"] {
        let dir = tempfile::tempdir().unwrap();
        let stale = stale_portrait(dir.path());
        let now = cache::file_epoch_secs(&stale) + 31 * 24 * 60 * 60;
        let mut deezer_text = |_: &str| Ok(r#"{"data":[]}"#.to_owned());
        let mut musicbrainz = |url: &str| {
            if url.contains("query=") {
                if failed_step == "search" {
                    return Err(FetchError::Transport);
                }
                return Ok(r#"{"artists":[{"id":"mbid","name":"Band","score":100}]}"#.to_owned());
            }
            if failed_step == "relations" {
                return Err(FetchError::Transport);
            }
            Ok(
                r#"{"relations":[{"url":{"resource":"https://www.wikidata.org/wiki/Q42"}}]}"#
                    .to_owned(),
            )
        };
        let mut wikimedia_text = |url: &str| {
            if url.contains("wbgetclaims") {
                if failed_step == "claims" {
                    return Err(FetchError::Transport);
                }
                return Ok(
                    r#"{"claims":{"P18":[{"mainsnak":{"datavalue":{"value":"Portrait.jpg"}}}]}}"#
                        .to_owned(),
                );
            }
            if failed_step == "imageinfo" {
                return Err(FetchError::Transport);
            }
            Ok(r#"{"query":{"pages":{"1":{"imageinfo":[{"thumburl":"https://upload.wikimedia.org/portrait.jpg","descriptionurl":"https://commons.wikimedia.org/wiki/File:Portrait.jpg","extmetadata":{"Artist":{"value":"Photo"},"LicenseShortName":{"value":"CC0"},"LicenseUrl":{"value":"https://creativecommons.org/publicdomain/zero/1.0"}}}]}}}}"#.to_owned())
        };
        let mut image = |_: &str| {
            if failed_step == "image" {
                Err(FetchError::Transport)
            } else {
                Ok(png_bytes())
            }
        };
        let mut fetchers = fetch::Fetchers {
            deezer_text: &mut deezer_text,
            musicbrainz_text: &mut musicbrainz,
            wikimedia_text: &mut wikimedia_text,
            image: &mut image,
        };

        let outcome = fetch::load_or_fetch("Band", None, now, dir.path(), &mut fetchers).unwrap();

        assert!(matches!(outcome, PortraitOutcome::Found(path) if path == stale));
        assert!(!cache::negative_marker_path(dir.path(), "Band").exists());
    }
}
