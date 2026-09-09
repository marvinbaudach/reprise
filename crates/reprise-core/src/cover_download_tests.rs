use std::io::Cursor;

use super::*;
use crate::source_error::{SourceError, SourceErrorKind};

const MB_STRONG: &str = r#"{"releases":[
  {"id":"11111111-1111-1111-1111-111111111111","score":100,
   "title":"The Wall","artist-credit":[{"name":"Pink Floyd"}]}]}"#;
const MB_WEAK: &str = r#"{"releases":[
  {"id":"22222222-2222-2222-2222-222222222222","score":42,
   "title":"Something Else","artist-credit":[{"name":"Other Band"}]}]}"#;

fn matching_releases(album_artist: &str, album: &str, ids: &[&str]) -> String {
    let releases = ids
        .iter()
        .map(|id| {
            serde_json::json!({
                "id": id,
                "score": 100,
                "title": album,
                "artist-credit": [{"name": album_artist}],
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({"releases": releases}).to_string()
}

#[test]
fn cover_failures_project_without_displaying_the_http_status() {
    let error = SourceError::from(musicbrainz::FetchError::HttpStatus(599));

    assert_eq!(error.kind(), &SourceErrorKind::Unreachable);
    assert!(!error.to_string().contains("599"));
    assert!(error
        .details("2026-07-30 14:12")
        .to_string()
        .contains("HTTP status 599"));
}

#[test]
fn album_key_normalizes_case_and_whitespace() {
    assert_eq!(
        album_key("Pink Floyd", "The Wall"),
        album_key("  pink   floyd ", "the wall")
    );
}

#[test]
fn album_key_distinguishes_different_albums() {
    assert_ne!(album_key("A", "X"), album_key("A", "Y"));
    assert_ne!(album_key("A", "X"), album_key("B", "X"));
}

#[test]
fn release_decoration_stripping_removes_exactly_one_trailing_decoration() {
    for (album, expected) in [
        ("Leave (Get Out) - Single", Some("Leave (Get Out)")),
        ("Number[s] (Deluxe Version)", Some("Number[s]")),
        ("Self Inflicted (Deluxe Edition)", Some("Self Inflicted")),
        ("Evolve [Explicit]", Some("Evolve")),
        ("My Forever Drug - Single", Some("My Forever Drug")),
        ("Album – Single", Some("Album")),
        ("Album — EP", Some("Album")),
        ("X-Single", None),
        ("The Black Crown (2011)", Some("The Black Crown")),
        ("Mixtape (Vol. 1) Part 2)", None),
        ("(What's the Story) Morning Glory?", None),
        ("Genesi[s]", Some("Genesi")),
        ("The Wall", None),
    ] {
        assert_eq!(
            strip_release_decoration(album).as_deref(),
            expected,
            "unexpected decoration stripping for {album:?}"
        );
    }
}

#[test]
fn downloaded_dir_is_under_cache_dir() {
    assert!(downloaded_dir().starts_with(crate::cover::cache_dir()));
}

#[test]
fn downloaded_cover_path_finds_an_existing_file_and_none_otherwise() {
    let key = album_key("FetchTest", "OnlyHere");
    assert!(downloaded_cover_path(&key).is_none());
    std::fs::create_dir_all(downloaded_dir()).unwrap();
    let f = downloaded_dir().join(format!("{key}.jpg"));
    std::fs::write(&f, b"x").unwrap();
    assert_eq!(downloaded_cover_path(&key), Some(f.clone()));
    std::fs::remove_file(&f).ok();
}

#[test]
fn parse_best_release_accepts_a_strong_match() {
    assert_eq!(
        parse_best_release(MB_STRONG, "Pink Floyd", "The Wall"),
        ReleaseSearchResult::Match(vec!["11111111-1111-1111-1111-111111111111".to_owned()])
    );
}

#[test]
fn parse_best_release_keeps_five_matching_releases_in_response_order() {
    let releases = r#"{"releases":[
      {"id":"release-3","score":100,"title":"The Wall","artist-credit":[{"name":"Pink Floyd"}]},
      {"id":"release-1","score":100,"title":"The Wall","artist-credit":[{"name":"Pink Floyd"}]},
      {"id":"release-5","score":99,"title":"The Wall","artist-credit":[{"name":"Pink Floyd"}]},
      {"id":"release-2","score":95,"title":"The Wall","artist-credit":[{"name":"Pink Floyd"}]},
      {"id":"release-4","score":90,"title":"The Wall","artist-credit":[{"name":"Pink Floyd"}]}
    ]}"#;

    assert_eq!(
        parse_best_release(releases, "Pink Floyd", "The Wall"),
        ReleaseSearchResult::Match(vec![
            "release-3".to_owned(),
            "release-1".to_owned(),
            "release-5".to_owned(),
            "release-2".to_owned(),
            "release-4".to_owned(),
        ])
    );
}

#[test]
fn parse_best_release_keeps_an_earlier_match_when_a_later_match_has_no_id() {
    let releases = r#"{"releases":[
      {"id":"release-with-id","score":100,"title":"The Wall","artist-credit":[{"name":"Pink Floyd"}]},
      {"score":100,"title":"The Wall","artist-credit":[{"name":"Pink Floyd"}]}
    ]}"#;

    assert_eq!(
        parse_best_release(releases, "Pink Floyd", "The Wall"),
        ReleaseSearchResult::Match(vec!["release-with-id".to_owned()])
    );
}

#[test]
fn parse_best_release_caps_matching_candidates_at_five() {
    let releases = matching_releases(
        "Pink Floyd",
        "The Wall",
        &[
            "release-1",
            "release-2",
            "release-3",
            "release-4",
            "release-5",
            "release-6",
        ],
    );

    let ReleaseSearchResult::Match(matches) =
        parse_best_release(&releases, "Pink Floyd", "The Wall")
    else {
        panic!("the matching releases should be retained");
    };
    assert_eq!(matches.len(), 5);
    assert_eq!(matches.last().map(String::as_str), Some("release-5"));
}

#[test]
fn parse_best_release_rejects_a_weak_match() {
    assert_eq!(
        parse_best_release(MB_WEAK, "Pink Floyd", "The Wall"),
        ReleaseSearchResult::NoMatch
    );
}

#[test]
fn parse_best_release_rejects_a_strong_match_from_another_artist() {
    let compilation = r#"{"releases":[
      {"id":"compilation","score":100,"title":"The Wall","artist-credit":[{"name":"Various Artists"}]}
    ]}"#;

    assert_eq!(
        parse_best_release(compilation, "Pink Floyd", "The Wall"),
        ReleaseSearchResult::NoMatch
    );
}

#[test]
fn parse_best_release_handles_empty_and_garbage() {
    assert_eq!(
        parse_best_release(r#"{"releases":[]}"#, "A", "B"),
        ReleaseSearchResult::NoMatch
    );
    assert_eq!(
        parse_best_release("not json", "A", "B"),
        ReleaseSearchResult::Malformed
    );
}

#[test]
fn urls_are_well_formed() {
    assert!(musicbrainz_search_url("Pink Floyd", "The Wall")
        .starts_with("https://musicbrainz.org/ws/2/release"));
    assert_eq!(
        caa_front_url("11111111-1111-1111-1111-111111111111"),
        "https://coverartarchive.org/release/11111111-1111-1111-1111-111111111111/front"
    );
    assert_eq!(
        caa_front_url("release id/with?delimiters"),
        "https://coverartarchive.org/release/release%20id%2Fwith%3Fdelimiters/front"
    );
    assert_eq!(
        caa_release_group_front_url("11111111-1111-1111-1111-111111111111"),
        "https://coverartarchive.org/release-group/11111111-1111-1111-1111-111111111111/front-250"
    );
}

#[test]
fn fetch_returns_cached_path_without_network_when_already_downloaded() {
    let key = album_key("CachedBand", "CachedAlbum");
    std::fs::create_dir_all(downloaded_dir()).unwrap();
    let f = downloaded_dir().join(format!("{key}.png"));
    std::fs::write(&f, b"img").unwrap();
    // Already cached -> must return it, never touching the network.
    assert_eq!(
        fetch_and_cache_with(
            "CachedBand",
            "CachedAlbum",
            None,
            &[],
            &mut |_| panic!("a cached cover must not search MusicBrainz"),
            &mut |_| panic!("a cached cover must not reach Cover Art Archive"),
        ),
        CoverFetchOutcome::Downloaded(f.clone())
    );
    std::fs::remove_file(&f).ok();
}

#[test]
fn fetch_short_circuits_on_negative_marker_without_network() {
    let key = album_key("MissBand", "MissAlbum");
    std::fs::create_dir_all(downloaded_dir()).unwrap();
    let marker = negative_marker_path(&key);
    std::fs::write(&marker, b"").unwrap();
    assert_eq!(
        fetch_and_cache_with(
            "MissBand",
            "MissAlbum",
            None,
            &[],
            &mut |_| panic!("a fresh marker must not search MusicBrainz"),
            &mut |_| panic!("a fresh marker must not reach Cover Art Archive"),
        ),
        CoverFetchOutcome::NotFound
    );
    std::fs::remove_file(&marker).ok();
}

#[test]
fn album_negative_marker_uses_the_one_shot_generation() {
    let marker = negative_marker_path("album-key");

    assert_eq!(
        marker.file_name().and_then(|name| name.to_str()),
        Some("album-key.notfound2")
    );
}

#[test]
fn stale_album_negative_marker_does_not_block_a_new_search() {
    let album = format!("Stale marker {:016x}", fastrand::u64(..));
    let key = album_key("Marker Band", &album);
    let marker = negative_marker_path(&key);
    std::fs::create_dir_all(downloaded_dir()).unwrap();
    std::fs::write(&marker, b"").unwrap();
    let stale = SystemTime::now() - Duration::from_secs(8 * 24 * 60 * 60);
    std::fs::File::options()
        .write(true)
        .open(&marker)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(stale))
        .unwrap();
    let mut mb_calls = 0;

    let outcome = fetch_and_cache_with(
        "Marker Band",
        &album,
        None,
        &[],
        &mut |_| {
            mb_calls += 1;
            Some(r#"{"releases":[]}"#.to_owned())
        },
        &mut |_| panic!("a definitive search miss must not reach Cover Art Archive"),
    );

    assert_eq!(outcome, CoverFetchOutcome::NotFound);
    assert_eq!(mb_calls, 1);
    std::fs::remove_file(marker).ok();
}

#[test]
fn transient_album_fetch_does_not_write_a_negative_marker() {
    let key = album_key("Retry Band", "Retry Album");
    let marker = negative_marker_path(&key);
    std::fs::remove_file(&marker).ok();

    let outcome = fetch_and_cache_with(
        "Retry Band",
        "Retry Album",
        Some("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa"),
        &[],
        &mut |_| panic!("an embedded release id must skip MusicBrainz search"),
        &mut |_| CaaFetchResult::TransientFailure,
    );

    assert_eq!(outcome, CoverFetchOutcome::TransientFailure);
    assert!(!marker.exists());
}

#[test]
fn definitive_album_miss_writes_a_negative_marker() {
    let key = album_key("Missing Band", "Missing Album");
    let marker = negative_marker_path(&key);
    std::fs::remove_file(&marker).ok();

    let outcome = fetch_and_cache_with(
        "Missing Band",
        "Missing Album",
        Some("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb"),
        &[],
        &mut |_| panic!("an embedded release id must skip MusicBrainz search"),
        &mut |_| CaaFetchResult::NotFound,
    );

    assert_eq!(outcome, CoverFetchOutcome::NotFound);
    assert!(marker.exists());
    std::fs::remove_file(marker).ok();
}

#[test]
fn candidate_walk_downloads_from_the_first_release_with_art() {
    let album = format!("Candidate fallback {:016x}", fastrand::u64(..));
    let key = album_key("Candidate Band", &album);
    let marker = negative_marker_path(&key);
    let body = matching_releases("Candidate Band", &album, &["no-art", "has-art"]);
    let mut requested = Vec::new();

    let outcome = fetch_and_cache_with(
        "Candidate Band",
        &album,
        None,
        &[],
        &mut |_| Some(body.clone()),
        &mut |url| {
            requested.push(url.to_owned());
            if url == caa_front_url("has-art") {
                CaaFetchResult::Found(b"art".to_vec(), "jpg")
            } else {
                CaaFetchResult::NotFound
            }
        },
    );

    let CoverFetchOutcome::Downloaded(path) = outcome else {
        panic!("the second release should supply the cover");
    };
    assert_eq!(
        requested,
        [caa_front_url("no-art"), caa_front_url("has-art")]
    );
    assert!(!marker.exists());
    std::fs::remove_file(path).ok();
}

#[test]
fn candidate_walk_writes_one_marker_after_every_release_is_missing() {
    let album = format!("All candidates missing {:016x}", fastrand::u64(..));
    let key = album_key("Candidate Band", &album);
    let marker = negative_marker_path(&key);
    let body = matching_releases("Candidate Band", &album, &["missing-1", "missing-2"]);
    let mut caa_calls = 0;

    let outcome = fetch_and_cache_with(
        "Candidate Band",
        &album,
        None,
        &[],
        &mut |_| Some(body.clone()),
        &mut |_| {
            caa_calls += 1;
            CaaFetchResult::NotFound
        },
    );

    assert_eq!(outcome, CoverFetchOutcome::NotFound);
    assert_eq!(caa_calls, 2);
    assert!(marker.exists());
    std::fs::remove_file(marker).ok();
}

#[test]
fn candidate_walk_continues_after_a_transient_failure_and_downloads_art() {
    let album = format!("Transient then art {:016x}", fastrand::u64(..));
    let key = album_key("Candidate Band", &album);
    let marker = negative_marker_path(&key);
    let body = matching_releases("Candidate Band", &album, &["transient", "has-art"]);

    let outcome = fetch_and_cache_with(
        "Candidate Band",
        &album,
        None,
        &[],
        &mut |_| Some(body.clone()),
        &mut |url| {
            if url == caa_front_url("has-art") {
                CaaFetchResult::Found(b"art".to_vec(), "jpg")
            } else {
                CaaFetchResult::TransientFailure
            }
        },
    );

    let CoverFetchOutcome::Downloaded(path) = outcome else {
        panic!("a later candidate should recover from the transient failure");
    };
    assert!(!marker.exists());
    std::fs::remove_file(path).ok();
}

#[test]
fn candidate_walk_keeps_a_transient_failure_retryable_after_clean_misses() {
    let album = format!("Transient then missing {:016x}", fastrand::u64(..));
    let key = album_key("Candidate Band", &album);
    let marker = negative_marker_path(&key);
    let body = matching_releases("Candidate Band", &album, &["transient", "missing"]);
    let mut caa_calls = 0;

    let outcome = fetch_and_cache_with(
        "Candidate Band",
        &album,
        None,
        &[],
        &mut |_| Some(body.clone()),
        &mut |url| {
            caa_calls += 1;
            if url == caa_front_url("transient") {
                CaaFetchResult::TransientFailure
            } else {
                CaaFetchResult::NotFound
            }
        },
    );

    assert_eq!(outcome, CoverFetchOutcome::TransientFailure);
    assert_eq!(caa_calls, 2);
    assert!(!marker.exists());
}

#[test]
fn strict_search_match_never_issues_a_stripped_fallback() {
    let album = "Songs (Live)";
    let key = album_key("Strict Band", album);
    let marker = negative_marker_path(&key);
    let body = matching_releases("Strict Band", album, &["strict-match"]);
    let mut mb_urls = Vec::new();

    let outcome = fetch_and_cache_with(
        "Strict Band",
        album,
        None,
        &[],
        &mut |url| {
            mb_urls.push(url.to_owned());
            Some(body.clone())
        },
        &mut |_| CaaFetchResult::Found(b"art".to_vec(), "jpg"),
    );

    let CoverFetchOutcome::Downloaded(path) = outcome else {
        panic!("the strict match should download");
    };
    assert_eq!(mb_urls, [musicbrainz_search_url("Strict Band", album)]);
    assert!(!marker.exists());
    std::fs::remove_file(path).ok();
}

#[test]
fn stripped_search_fallback_uses_the_stripped_title_for_query_and_comparison() {
    let album = format!("Evolve {:016x} [Explicit]", fastrand::u64(..));
    let stripped = strip_release_decoration(&album).unwrap();
    let raw_key = album_key("Fallback Band", &album);
    let marker = negative_marker_path(&raw_key);
    let body = matching_releases("Fallback Band", &stripped, &["stripped-match"]);
    let mut mb_urls = Vec::new();

    let outcome = fetch_and_cache_with(
        "Fallback Band",
        &album,
        None,
        &[],
        &mut |url| {
            mb_urls.push(url.to_owned());
            if mb_urls.len() == 1 {
                Some(r#"{"releases":[]}"#.to_owned())
            } else {
                Some(body.clone())
            }
        },
        &mut |_| CaaFetchResult::Found(b"art".to_vec(), "jpg"),
    );

    let CoverFetchOutcome::Downloaded(path) = outcome else {
        panic!("the stripped fallback should download");
    };
    assert_eq!(
        mb_urls,
        [
            musicbrainz_search_url("Fallback Band", &album),
            musicbrainz_search_url("Fallback Band", &stripped),
        ]
    );
    assert_eq!(
        path.file_stem().and_then(|stem| stem.to_str()),
        Some(raw_key.as_str())
    );
    assert!(!marker.exists());
    std::fs::remove_file(path).ok();
}

#[test]
fn undecorated_search_miss_issues_one_request_and_writes_a_marker() {
    let album = format!("Undecorated miss {:016x}", fastrand::u64(..));
    let key = album_key("Missing Band", &album);
    let marker = negative_marker_path(&key);
    let mut mb_calls = 0;

    let outcome = fetch_and_cache_with(
        "Missing Band",
        &album,
        None,
        &[],
        &mut |_| {
            mb_calls += 1;
            Some(r#"{"releases":[]}"#.to_owned())
        },
        &mut |_| panic!("a definitive search miss must not reach Cover Art Archive"),
    );

    assert_eq!(outcome, CoverFetchOutcome::NotFound);
    assert_eq!(mb_calls, 1);
    assert!(marker.exists());
    std::fs::remove_file(marker).ok();
}

#[test]
fn only_caa_not_found_is_a_clean_http_miss() {
    assert!(is_clean_caa_miss(404));
    assert!(!is_clean_caa_miss(500));
    assert!(!is_clean_caa_miss(429));
}

#[test]
fn nr_2a_missing_cover_uses_fallback_tile() {
    let mbid = "99999999-9999-9999-9999-999999999999";
    let mut fetch = |_url: &str| CaaFetchResult::NotFound;

    let result = fetch_release_group_cover_with(mbid, &mut fetch);

    assert_eq!(result, ReleaseGroupCover::Fallback);
    std::fs::remove_file(negative_marker_path(&release_group_key(mbid))).ok();
}

#[test]
fn cover_state_distinguishes_cached_known_missing_and_unknown() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000_000);
    let cached_path = PathBuf::from("/isolated/release-cover.png");
    assert_eq!(
        cover_state_from(Some(cached_path.clone()), None, now),
        CoverState::Cached(cached_path.clone())
    );

    let modified = now - Duration::from_secs(60);
    assert_eq!(
        cover_state_from(None, Some(modified), now),
        CoverState::KnownMissing
    );
    assert_eq!(
        cover_state_from(
            None,
            Some(modified),
            modified + NEGATIVE_MARKER_MAX_AGE + Duration::from_secs(1),
        ),
        CoverState::Unknown
    );
}

#[test]
fn negative_marker_blocks_when_fresh() {
    let now = SystemTime::now();
    assert!(negative_marker_blocks(Some(now), now));
}

#[test]
fn negative_marker_does_not_block_when_stale() {
    let now = SystemTime::now();
    let eight_days_ago = now - Duration::from_secs(8 * 24 * 60 * 60);
    assert!(!negative_marker_blocks(Some(eight_days_ago), now));
}

#[test]
fn negative_marker_does_not_block_when_absent() {
    assert!(!negative_marker_blocks(None, SystemTime::now()));
}

#[test]
fn negative_marker_blocks_when_mtime_is_in_the_future() {
    // Clock skew: a marker that appears newer than "now" is treated as fresh.
    let now = SystemTime::now();
    let future = now + Duration::from_secs(60);
    assert!(negative_marker_blocks(Some(future), now));
}

#[test]
fn release_group_fetch_short_circuits_on_fresh_negative_marker_without_network() {
    let mbid = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
    let key = release_group_key(mbid);
    std::fs::create_dir_all(downloaded_dir()).unwrap();
    let marker = negative_marker_path(&key);
    std::fs::write(&marker, b"").unwrap();

    let mut fetch = |_url: &str| -> CaaFetchResult { panic!("must not hit the network") };
    let result = fetch_release_group_cover_with(mbid, &mut fetch);

    assert_eq!(result, ReleaseGroupCover::Fallback);
    std::fs::remove_file(&marker).ok();
}

#[test]
fn release_group_not_found_writes_a_negative_marker() {
    let mbid = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
    let key = release_group_key(mbid);
    let marker = negative_marker_path(&key);
    std::fs::remove_file(&marker).ok();

    let mut fetch = |_url: &str| CaaFetchResult::NotFound;
    let result = fetch_release_group_cover_with(mbid, &mut fetch);

    assert_eq!(result, ReleaseGroupCover::Fallback);
    assert!(marker.exists());
    std::fs::remove_file(&marker).ok();
}

#[test]
fn release_group_transient_failure_does_not_write_a_marker() {
    let mbid = "cccccccc-cccc-cccc-cccc-cccccccccccc";
    let key = release_group_key(mbid);
    let marker = negative_marker_path(&key);
    std::fs::remove_file(&marker).ok();

    let mut fetch = |_url: &str| CaaFetchResult::TransientFailure;
    let result = fetch_release_group_cover_with(mbid, &mut fetch);

    assert_eq!(result, ReleaseGroupCover::Fallback);
    assert!(!marker.exists());
}

#[test]
fn downloaded_bytes_must_decode_as_a_supported_image() {
    assert_eq!(validated_image_extension(b"not an image"), None);

    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(1, 1)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    assert_eq!(validated_image_extension(png.get_ref()), Some("png"));
}

#[test]
fn rock_antenne_favicon_is_accepted_as_ico() {
    let favicon = include_bytes!("../tests/fixtures/rock-antenne-favicon.ico");

    assert_eq!(validated_image_extension(favicon), Some("ico"));
}

#[test]
fn validated_ico_download_is_found_again() {
    let favicon = include_bytes!("../tests/fixtures/rock-antenne-favicon.ico");
    let extension = validated_image_extension(favicon).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(format!("station.{extension}"));
    std::fs::write(&path, favicon).unwrap();

    assert_eq!(
        downloaded_cover_path_from_dir(directory.path(), "station"),
        Some(path)
    );
}

#[test]
fn cover_1_publishing_an_album_download_writes_cache_and_album_folder() {
    let album = tempfile::tempdir().unwrap();
    let key = format!("writeback-success-{:016x}", fastrand::u64(..));
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(1, 1)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();

    let cached =
        store_album_downloaded(&key, png.get_ref(), "png", &[album.path().to_path_buf()]).unwrap();

    assert_eq!(std::fs::read(&cached).unwrap(), *png.get_ref());
    assert_eq!(
        std::fs::read(album.path().join("cover.png")).unwrap(),
        *png.get_ref()
    );
    std::fs::remove_file(cached).ok();
}

#[test]
fn cover_1_album_write_failure_does_not_fail_the_cached_download() {
    let album = tempfile::tempdir().unwrap();
    let key = format!("writeback-failure-{:016x}", fastrand::u64(..));
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(1, 1)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let mut writeback_called = false;

    let cached = store_album_downloaded_with(
        &key,
        png.get_ref(),
        "png",
        &[album.path().to_path_buf()],
        |_, _, _| {
            writeback_called = true;
            vec![crate::cover_writeback::CoverWrite::Failed]
        },
    )
    .unwrap();

    assert!(writeback_called);
    assert_eq!(std::fs::read(&cached).unwrap(), *png.get_ref());
    assert!(!album.path().join("cover.png").exists());
    std::fs::remove_file(cached).ok();
}
