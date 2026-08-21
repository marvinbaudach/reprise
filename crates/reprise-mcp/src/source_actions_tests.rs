use super::*;

#[test]
fn malformed_radio_browser_server_returns_without_panicking() {
    let outcome = std::panic::catch_unwind(|| radio_uuid_url("https://[", "station-one"));

    assert!(outcome.is_ok(), "third-party server data must not unwind");
    assert!(matches!(
        outcome.unwrap(),
        Err(reprise_core::radio::RadioError::Parse(_))
    ));
}

#[test]
fn podcast_refresh_json_omits_counters_the_refresh_no_longer_measures() {
    let result = PodcastRefreshResult {
        action: "refresh",
        attempted: 4,
        refreshed: 2,
        not_modified: 1,
        failed: 1,
        episodes_inserted: 3,
        episodes_updated: 5,
    };

    assert_eq!(
        serde_json::to_value(result).unwrap(),
        serde_json::json!({
            "action": "refresh",
            "attempted": 4,
            "refreshed": 2,
            "not_modified": 1,
            "failed": 1,
            "episodes_inserted": 3,
            "episodes_updated": 5
        })
    );
}
