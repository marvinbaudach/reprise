use super::{counts_as_play, stored_play_count_total};

#[test]
fn counts_as_play_matches_the_scrobble_threshold() {
    assert!(!counts_as_play(89_999, 180_000));
    assert!(counts_as_play(90_000, 180_000));
    assert!(!counts_as_play(239_999, 600_000));
    assert!(counts_as_play(240_000, 600_000));
    assert!(!counts_as_play(1_000, 0));
    assert!(!counts_as_play(1_000, -1));
}

#[test]
fn stored_play_count_total_reads_counters_without_creating_events() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, play_count, added_at) \
         VALUES (1, '/music/one.flac', 'One', 'Artist', 190, 0), \
                (2, '/music/two.flac', 'Two', 'Artist', 4, 0)",
        [],
    )
    .unwrap();

    assert_eq!(stored_play_count_total(&conn).unwrap(), 194);
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM listen_events", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap(),
        0
    );
}
