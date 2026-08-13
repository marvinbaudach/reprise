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
fn an_oversized_caa_body_is_retryable() {
    let bytes = vec![0; MAX_IMAGE_BYTES as usize + 1];

    assert!(matches!(
        classify_caa_body(bytes),
        CaaFetchResult::TransientFailure
    ));
}

#[test]
fn an_unreadable_caa_image_is_retryable() {
    assert!(matches!(
        classify_caa_body(b"not an image".to_vec()),
        CaaFetchResult::TransientFailure
    ));
}
