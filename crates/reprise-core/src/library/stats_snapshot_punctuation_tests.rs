use chrono::Utc;

use super::*;

/// Typographic dashes and apostrophes are a tagger's habit, not a different
/// name: the artist and genre rows each count them once. A bare space
/// where the dash was is a different spelling and stays its own row.
#[test]
fn stats_9_typographic_punctuation_folds_in_artists_and_genres() {
    let conn = migrated_conn();
    let rows = [
        (1, "Guns N' Roses", "Post-Rock", 2),
        (2, "Guns N\u{2019} Roses", "Post\u{2013}Rock", 1),
        (3, "Guns N Roses", "Post Rock", 1),
    ];
    for (id, artist, genre, plays) in rows {
        insert_track(
            &conn,
            id,
            &format!("Track {id}"),
            artist,
            "",
            genre,
            100_000,
            0,
            None,
        );
        for play in 0..plays {
            insert_event(&conn, id, timestamp(2026, 6, id as u32, 12, play), 100_000);
        }
    }

    let snapshot = compute(&conn, StatsPeriod::Year(2026), NOW_2026_07_19, &Utc).unwrap();

    assert_eq!(snapshot.top_artists.len(), 2);
    assert_eq!(snapshot.top_artists[0].group.label, "Guns N' Roses");
    assert_eq!(snapshot.top_artists[0].group.plays, 3);
    assert_eq!(snapshot.top_artists[0].group.variant_count, 2);
    assert_eq!(snapshot.top_artists[1].group.label, "Guns N Roses");
    assert_eq!(
        snapshot
            .genres
            .segments
            .iter()
            .map(|segment| segment.label.as_str())
            .collect::<Vec<_>>(),
        ["Post-Rock", "Post Rock"]
    );
    assert_eq!(
        group_track_ids(&conn, GroupKind::Artist, &snapshot.top_artists[0].group.key).unwrap(),
        vec![1, 2]
    );
}

/// The album title half of an album row folds typographic dashes too.
#[test]
fn stats_9_album_titles_fold_typographic_dashes() {
    let conn = migrated_conn();
    insert_album_track(&conn, 1, "One", "Rock - Live", "Artist", 2);
    insert_album_track(&conn, 2, "Two", "Rock \u{2013} Live", "Artist", 1);
    insert_album_track(&conn, 3, "Three", "Rock Live", "Artist", 1);

    let snapshot = compute(&conn, StatsPeriod::Year(2026), NOW_2026_07_19, &Utc).unwrap();

    assert_eq!(snapshot.top_albums.len(), 2);
    assert_eq!(snapshot.top_albums[0].album, "Rock - Live");
    assert_eq!(snapshot.top_albums[0].plays, 3);
    assert_eq!(snapshot.top_albums[1].album, "Rock Live");
}
