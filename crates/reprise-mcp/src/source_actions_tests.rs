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
