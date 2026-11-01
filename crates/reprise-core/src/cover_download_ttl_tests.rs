use super::*;

#[test]
fn net_7e_album_negative_ttl_is_deterministic_and_spread_from_thirty_to_thirty_seven_days() {
    let first = album_key("Band", "First");
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
}

#[test]
fn net_7e_album_negative_ttl_spreads_over_the_week_without_leaving_it() {
    let ttls: std::collections::HashSet<Duration> = (0..64)
        .map(|index| negative_marker_ttl(&album_key("Band", &format!("Album {index}"))))
        .collect();

    assert!(ttls.iter().all(|ttl| {
        (NEGATIVE_MARKER_MAX_AGE..=NEGATIVE_MARKER_MAX_AGE + NEGATIVE_MARKER_MAX_SPREAD)
            .contains(ttl)
    }));
    assert!(ttls.len() > 1, "all 64 albums share one expiry day");
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

#[test]
fn net_7e_album_negative_marker_expires_at_exactly_its_keyed_ttl() {
    let key = album_key("Band", "Exact");
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
        modified + ttl
    ));
}

#[test]
fn net_7e_an_album_marker_dated_more_than_a_day_ahead_is_expired() {
    let key = album_key("Band", "Skewed");
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000_000);

    assert!(negative_marker_blocks(
        Some(now + Duration::from_secs(60 * 60)),
        &key,
        now,
    ));
    assert!(!negative_marker_blocks(
        Some(now + Duration::from_secs(2 * 24 * 60 * 60)),
        &key,
        now,
    ));
}

#[test]
fn net_7e_a_local_marker_dated_more_than_a_day_ahead_is_expired() {
    let cache = tempfile::tempdir().unwrap();
    write_local_marker_in(cache.path(), "Band", "Skewed local");
    let marker = downloaded_dir_in(cache.path())
        .join(format!("{}.local", album_key("Band", "Skewed local")));
    std::fs::File::options()
        .write(true)
        .open(&marker)
        .unwrap()
        .set_times(
            std::fs::FileTimes::new()
                .set_modified(SystemTime::now() + Duration::from_secs(2 * 24 * 60 * 60)),
        )
        .unwrap();

    assert!(!local_marker_is_fresh_in(
        cache.path(),
        "Band",
        "Skewed local"
    ));
}
