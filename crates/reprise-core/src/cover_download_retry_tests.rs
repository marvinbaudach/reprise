use super::*;

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
fn malformed_musicbrainz_search_does_not_write_a_negative_marker() {
    for (case, body) in [
        ("invalid-json", "not json"),
        ("missing-releases", r#"{"unexpected":[]}"#),
    ] {
        let album = format!("Retry malformed search {case}");
        let key = album_key("Retry Band", &album);
        let marker = negative_marker_path(&key);
        std::fs::remove_file(&marker).ok();

        let outcome = fetch_and_cache_with(
            "Retry Band",
            &album,
            None,
            &[],
            &mut |_| Some(body.to_owned()),
            &mut |_| panic!("a malformed search must not reach Cover Art Archive"),
        );

        assert_eq!(outcome, CoverFetchOutcome::TransientFailure);
        assert!(!marker.exists());
    }
}

#[test]
fn failed_musicbrainz_search_does_not_write_a_negative_marker() {
    let key = album_key("Retry Search Band", "Retry Search Album");
    let marker = negative_marker_path(&key);
    std::fs::remove_file(&marker).ok();

    let outcome = fetch_and_cache_with(
        "Retry Search Band",
        "Retry Search Album",
        None,
        &[],
        &mut |_| None,
        &mut |_| panic!("a failed search must not reach Cover Art Archive"),
    );

    assert_eq!(outcome, CoverFetchOutcome::TransientFailure);
    assert!(!marker.exists());
}

#[test]
fn failed_stripped_musicbrainz_search_does_not_write_a_negative_marker() {
    let album = format!("Retry stripped search {:016x} - Single", fastrand::u64(..));
    let key = album_key("Retry Search Band", &album);
    let marker = negative_marker_path(&key);
    std::fs::remove_file(&marker).ok();
    let mut mb_calls = 0;

    let outcome = fetch_and_cache_with(
        "Retry Search Band",
        &album,
        None,
        &[],
        &mut |_| {
            mb_calls += 1;
            (mb_calls == 1).then(|| r#"{"releases":[]}"#.to_owned())
        },
        &mut |_| panic!("a failed stripped search must not reach Cover Art Archive"),
    );

    assert_eq!(outcome, CoverFetchOutcome::TransientFailure);
    assert_eq!(mb_calls, 2);
    assert!(!marker.exists());
}

#[test]
fn malformed_stripped_musicbrainz_search_does_not_write_a_negative_marker() {
    let album = format!(
        "Retry malformed fallback {:016x} [Explicit]",
        fastrand::u64(..)
    );
    let key = album_key("Retry Search Band", &album);
    let marker = negative_marker_path(&key);
    std::fs::remove_file(&marker).ok();
    let mut mb_calls = 0;

    let outcome = fetch_and_cache_with(
        "Retry Search Band",
        &album,
        None,
        &[],
        &mut |_| {
            mb_calls += 1;
            Some(if mb_calls == 1 {
                r#"{"releases":[]}"#.to_owned()
            } else {
                "not json".to_owned()
            })
        },
        &mut |_| panic!("a malformed stripped search must not reach Cover Art Archive"),
    );

    assert_eq!(outcome, CoverFetchOutcome::TransientFailure);
    assert_eq!(mb_calls, 2);
    assert!(!marker.exists());
}

#[test]
fn well_formed_musicbrainz_miss_writes_a_negative_marker() {
    let key = album_key("Missing Search Band", "Missing Search Album");
    let marker = negative_marker_path(&key);
    std::fs::remove_file(&marker).ok();

    let outcome = fetch_and_cache_with(
        "Missing Search Band",
        "Missing Search Album",
        None,
        &[],
        &mut |_| Some(MB_WEAK.to_owned()),
        &mut |_| panic!("a definitive search miss must not reach Cover Art Archive"),
    );

    assert_eq!(outcome, CoverFetchOutcome::NotFound);
    assert!(marker.exists());
    std::fs::remove_file(marker).ok();
}

#[test]
fn cache_write_failure_is_classified_as_retryable() {
    let album = format!("Unwritable cache {:016x}", fastrand::u64(..));
    let key = album_key("Retry Cache Band", &album);
    let marker = negative_marker_path(&key);
    std::fs::remove_file(&marker).ok();

    let outcome = fetch_and_cache_with(
        "Retry Cache Band",
        &album,
        Some("cccccccc-cccc-cccc-cccc-cccccccccccc"),
        &[],
        &mut |_| panic!("an embedded release id must skip MusicBrainz search"),
        &mut |_| CaaFetchResult::Found(vec![1, 2, 3], "missing/subdirectory"),
    );

    assert_eq!(outcome, CoverFetchOutcome::TransientFailure);
    assert!(!marker.exists());
}

#[test]
fn an_oversized_caa_body_is_a_definitive_candidate_miss() {
    let bytes = vec![0; MAX_IMAGE_BYTES as usize + 1];

    assert!(matches!(
        classify_caa_body(bytes, Some("text/html")),
        CaaFetchResult::UnusableBody
    ));
}

#[test]
fn an_unreadable_caa_image_is_a_definitive_candidate_miss() {
    assert!(matches!(
        classify_caa_body(b"not an image".to_vec(), Some("image/jpeg")),
        CaaFetchResult::UnusableBody
    ));
}

#[test]
fn an_html_success_body_is_retryable_and_writes_no_negative_marker() {
    let album = format!("Retry HTML body {:016x}", fastrand::u64(..));
    let key = album_key("Retry Band", &album);
    let marker = negative_marker_path(&key);
    std::fs::remove_file(&marker).ok();

    let outcome = fetch_and_cache_with(
        "Retry Band",
        &album,
        Some("dddddddd-dddd-dddd-dddd-dddddddddddd"),
        &[],
        &mut |_| panic!("an embedded release id must skip MusicBrainz search"),
        &mut |_| classify_caa_body(b"temporarily unavailable".to_vec(), Some("text/html")),
    );

    assert_eq!(outcome, CoverFetchOutcome::TransientFailure);
    assert!(!marker.exists());
}

#[test]
fn deterministic_client_errors_exhaust_the_candidate_walk_and_write_a_marker() {
    let album = format!("Client errors {:016x}", fastrand::u64(..));
    let key = album_key("Client Error Band", &album);
    let marker = negative_marker_path(&key);
    let body = matching_releases("Client Error Band", &album, &["bad-1", "bad-2"]);
    let mut caa_calls = 0;

    let outcome = fetch_and_cache_with(
        "Client Error Band",
        &album,
        None,
        &[],
        &mut |_| Some(body.clone()),
        &mut |_| {
            caa_calls += 1;
            classify_caa_status(400)
        },
    );

    assert_eq!(outcome, CoverFetchOutcome::NotFound);
    assert_eq!(caa_calls, 2);
    assert!(marker.exists());
    std::fs::remove_file(marker).ok();
}

#[test]
fn a_throttled_client_error_keeps_walking_and_leaves_no_marker() {
    // CDNs in front of the Cover Art Archive answer 401/403 while throttling,
    // so neither may end the walk early or cache "no cover" for a week.
    for status in [401, 403] {
        let album = format!("Throttled {status} {:016x}", fastrand::u64(..));
        let key = album_key("Throttled Band", &album);
        let marker = negative_marker_path(&key);
        let body = matching_releases("Throttled Band", &album, &["bad-1", "bad-2"]);
        let mut caa_calls = 0;

        let outcome = fetch_and_cache_with(
            "Throttled Band",
            &album,
            None,
            &[],
            &mut |_| Some(body.clone()),
            &mut |_| {
                caa_calls += 1;
                classify_caa_status(status)
            },
        );

        assert_eq!(outcome, CoverFetchOutcome::TransientFailure);
        assert_eq!(caa_calls, 2, "status {status} cut the candidate walk short");
        assert!(!marker.exists(), "status {status} wrote a negative marker");
    }
}

#[test]
fn server_and_rate_limit_statuses_remain_retryable_without_a_marker() {
    for status in [503, 429, 408] {
        let album = format!("Retry status {status} {:016x}", fastrand::u64(..));
        let key = album_key("Retry Status Band", &album);
        let marker = negative_marker_path(&key);

        let outcome = fetch_and_cache_with(
            "Retry Status Band",
            &album,
            Some("eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee"),
            &[],
            &mut |_| panic!("an embedded release id must skip MusicBrainz search"),
            &mut |_| classify_caa_status(status),
        );

        assert_eq!(outcome, CoverFetchOutcome::TransientFailure);
        assert!(!marker.exists(), "status {status} wrote a negative marker");
    }
}

#[test]
fn invalid_embedded_release_mbid_is_rejected_before_the_caa_request() {
    let album = format!("Invalid embedded MBID {:016x}", fastrand::u64(..));
    let key = album_key("Invalid ID Band", &album);
    let marker = negative_marker_path(&key);
    let mut caa_calls = 0;

    let outcome = fetch_and_cache_with(
        "Invalid ID Band",
        &album,
        Some("not-a-uuid"),
        &[],
        &mut |_| Some(r#"{"releases":[]}"#.to_owned()),
        &mut |_| {
            caa_calls += 1;
            CaaFetchResult::NotFound
        },
    );

    assert_eq!(outcome, CoverFetchOutcome::NotFound);
    assert_eq!(caa_calls, 0);
    assert!(marker.exists());
    std::fs::remove_file(marker).ok();
}
