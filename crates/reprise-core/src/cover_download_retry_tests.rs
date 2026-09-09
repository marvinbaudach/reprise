use super::*;

const MB_WEAK: &str = r#"{"releases":[
  {"id":"22222222-2222-2222-2222-222222222222","score":42,
   "title":"Something Else","artist-credit":[{"name":"Other Band"}]}]}"#;

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
        classify_caa_body(bytes),
        CaaFetchResult::UnusableBody
    ));
}

#[test]
fn an_unreadable_caa_image_is_a_definitive_candidate_miss() {
    assert!(matches!(
        classify_caa_body(b"not an image".to_vec()),
        CaaFetchResult::UnusableBody
    ));
}
