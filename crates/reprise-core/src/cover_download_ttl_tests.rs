use super::*;

#[test]
fn net_7e_album_negative_ttl_is_deterministic_and_spread_from_thirty_to_thirty_seven_days() {
    let first = album_key("Band", "First");
    let second = album_key("Band", "Second");
    let first_ttl = negative_marker_ttl(&first);

    assert_eq!(
        NEGATIVE_MARKER_MAX_AGE,
        Duration::from_secs(30 * 24 * 60 * 60)
    );
    assert_eq!(first_ttl, negative_marker_ttl(&first));
    assert!(
        (NEGATIVE_MARKER_MAX_AGE..=NEGATIVE_MARKER_MAX_AGE + NEGATIVE_MARKER_MAX_SPREAD)
            .contains(&first_ttl)
    );
    assert_ne!(first_ttl, negative_marker_ttl(&second));
}

#[test]
fn net_7e_album_negative_marker_expires_on_its_keyed_boundary() {
    let key = album_key("Band", "Boundary");
    let modified = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
    let ttl = negative_marker_ttl(&key);

    assert!(negative_marker_blocks(
        Some(modified),
        &key,
        modified + ttl - Duration::from_secs(1),
    ));
    assert!(!negative_marker_blocks(
        Some(modified),
        &key,
        modified + ttl + Duration::from_secs(1),
    ));
}
