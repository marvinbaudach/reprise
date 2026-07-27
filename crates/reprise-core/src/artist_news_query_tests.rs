//! Tests for the query layer and its library-presence annotation in
//! `artist_news_query.rs`. Split out of `artist_news_tests.rs` purely to
//! keep both files under the project's 800-line rule — a pure move, not a
//! rewrite.

use chrono::NaiveDate;

use crate::artist_news::{
    hidden_release_count, mark_releases_seen, query_releases, set_release_hidden,
    unseen_release_count,
};

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 7, 13).unwrap()
}

fn migrated_conn() -> rusqlite::Connection {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn
}

fn insert_release(conn: &rusqlite::Connection, mbid: &str, seen_at: Option<i64>) {
    conn.execute(
        "INSERT INTO new_releases (
           release_group_mbid, artist_name, artist_mbid, title, release_type,
           first_release_date, fetched_at, seen_at, fallback_accent
         ) VALUES (?1, 'Artist', 'artist-id', 'Release', 'Album', '2026-08-01', 1, ?2, '#123456')",
        rusqlite::params![mbid, seen_at],
    )
    .unwrap();
}

fn insert_named_release(
    conn: &rusqlite::Connection,
    mbid: &str,
    title: &str,
    seen_at: Option<i64>,
) {
    conn.execute(
        "INSERT INTO new_releases (
           release_group_mbid, artist_name, artist_mbid, title, release_type,
           first_release_date, fetched_at, seen_at, fallback_accent
         ) VALUES (?1, 'Artist', 'artist-id', ?2, 'Album', '2026-08-01', 1, ?3, '#123456')",
        rusqlite::params![mbid, title, seen_at],
    )
    .unwrap();
}

#[test]
fn nr_9a_unseen_badge_excludes_complete_albums_but_keeps_partial_ones() {
    let conn = migrated_conn();
    insert_named_release(&conn, "owned", "Owned Album", None);
    insert_named_release(&conn, "partial", "Partial Album", None);
    insert_named_release(&conn, "absent", "Absent Album", None);
    insert_named_release(&conn, "seen", "Seen Album", Some(20));
    for (path, title, album) in [
        ("/music/owned-one.flac", "Owned One", "Owned Album"),
        ("/music/owned-two.flac", "Owned Two", "Owned Album"),
        ("/music/partial.flac", "Partial", "Partial Album"),
    ] {
        conn.execute(
            "INSERT INTO tracks (path, title, artist, album, play_count, added_at)
             VALUES (?1, ?2, 'Artist', ?3, 0, 0)",
            rusqlite::params![path, title, album],
        )
        .unwrap();
    }

    assert_eq!(
        unseen_release_count(&conn).unwrap(),
        2,
        "only absent and partial unseen releases contribute to the badge"
    );
}

#[test]
fn nr_3a_opening_marks_seen_clears_badge() {
    let conn = migrated_conn();
    insert_release(&conn, "one", None);
    insert_release(&conn, "two", None);
    insert_release(&conn, "already-seen", Some(50));
    assert_eq!(unseen_release_count(&conn).unwrap(), 2);

    mark_releases_seen(&conn, &["one".into(), "two".into()], 100).unwrap();

    assert_eq!(unseen_release_count(&conn).unwrap(), 0);
    let seen_at: Vec<Option<i64>> = ["one", "two", "already-seen"]
        .into_iter()
        .map(|id| {
            conn.query_row(
                "SELECT seen_at FROM new_releases WHERE release_group_mbid = ?1",
                [id],
                |row| row.get(0),
            )
            .unwrap()
        })
        .collect();
    assert_eq!(seen_at, [Some(100), Some(100), Some(50)]);
}

#[test]
fn hide_sets_hidden_and_set_release_hidden_false_restores_it() {
    let conn = migrated_conn();
    insert_release(&conn, "one", None);
    insert_release(&conn, "two", None);

    set_release_hidden(&conn, "one", true).unwrap();

    assert_eq!(hidden_release_count(&conn).unwrap(), 1);
    assert_eq!(
        query_releases(&conn, false, date())
            .unwrap()
            .into_iter()
            .map(|release| release.release_group_mbid)
            .collect::<Vec<_>>(),
        ["two"]
    );
    assert!(query_releases(&conn, true, date())
        .unwrap()
        .into_iter()
        .find(|release| release.release_group_mbid == "one")
        .is_some_and(|release| release.hidden));
    let hidden_at: Option<i64> = conn
        .query_row(
            "SELECT hidden_at FROM new_releases WHERE release_group_mbid = 'one'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(hidden_at.is_some(), "hiding must stamp hidden_at");

    set_release_hidden(&conn, "one", false).unwrap();
    let hidden_at_after_unhide: Option<i64> = conn
        .query_row(
            "SELECT hidden_at FROM new_releases WHERE release_group_mbid = 'one'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        hidden_at_after_unhide.is_none(),
        "un-hiding via set_release_hidden clears hidden_at again"
    );

    set_release_hidden(&conn, "one", true).unwrap();

    // The former blanket "un-hide everything" helper (`show_hidden_releases`)
    // is gone — `restore_release` (A2) replaces it for the real UI path
    // (single release, wired in C1), and `set_release_hidden(.., false)`
    // remains the primitive both build on, which this asserts directly.
    set_release_hidden(&conn, "one", false).unwrap();

    assert_eq!(hidden_release_count(&conn).unwrap(), 0);
    assert_eq!(query_releases(&conn, false, date()).unwrap().len(), 2);
}

#[test]
fn nr_13_query_marks_local_albums_instead_of_dropping_them() {
    let conn = migrated_conn();
    conn.execute(
        "INSERT INTO new_releases (
           release_group_mbid, artist_name, artist_mbid, title, release_type,
           first_release_date, fetched_at, fallback_accent
         ) VALUES ('owned', 'Pink Floyd', 'artist-id', 'Local Album', 'Album', '2026-08-01', 1, '#123456')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO new_releases (
           release_group_mbid, artist_name, artist_mbid, title, release_type,
           first_release_date, fetched_at, fallback_accent
         ) VALUES ('new', 'Pink Floyd', 'artist-id', 'Brand New Album', 'Album', '2026-08-01', 1, '#123456')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
         VALUES ('/music/local.flac', 'Track', 'Pink Floyd', 'Local Album', 5, 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
         VALUES ('/music/local2.flac', 'Track Two', 'Pink Floyd', 'Local Album', 5, 0)",
        [],
    )
    .unwrap();

    let releases = query_releases(&conn, true, date()).unwrap();

    assert_eq!(releases.len(), 2, "in-library releases stay in the list");
    let owned = releases
        .iter()
        .find(|release| release.release_group_mbid == "owned")
        .unwrap();
    assert_eq!(
        owned.presence,
        crate::artist_news::LibraryPresence::Complete,
        "matching local album (two tracks) is marked fully owned"
    );
    let brand_new = releases
        .iter()
        .find(|release| release.release_group_mbid == "new")
        .unwrap();
    assert_eq!(
        brand_new.presence,
        crate::artist_news::LibraryPresence::Absent,
        "release with no local match stays absent"
    );
}

#[test]
fn presence_distinguishes_absent_partial_and_complete() {
    use crate::artist_news::{presence_for, LibraryPresence};

    let mut counts = std::collections::HashMap::new();
    counts.insert(("pink floyd".to_string(), "owned album".to_string()), 2);
    counts.insert(("pink floyd".to_string(), "just a single".to_string()), 1);

    assert_eq!(
        presence_for(&counts, "Pink Floyd", "Owned Album"),
        LibraryPresence::Complete
    );
    assert_eq!(
        presence_for(&counts, " PINK   FLOYD ", " just a single "),
        LibraryPresence::Partial,
        "normalization must match query_releases' own"
    );
    assert_eq!(
        presence_for(&counts, "Pink Floyd", "Never Heard Of It"),
        LibraryPresence::Absent
    );
}

#[test]
fn query_releases_reports_partial_ownership_for_a_single_track() {
    use crate::artist_news::LibraryPresence;

    let conn = migrated_conn();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
         VALUES ('/music/lead.flac', 'Lead Single', 'Pink Floyd', 'Eclipse', 1, 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO new_releases (release_group_mbid, artist_name, artist_mbid, title, \
         release_type, first_release_date, fetched_at, fallback_accent, first_seen) \
         VALUES ('rg-1', 'Pink Floyd', 'mbid-1', 'Eclipse', 'Album', '2026-09-01', 100, \
         '#3584E4', 100)",
        [],
    )
    .unwrap();

    let releases = query_releases(&conn, false, date()).unwrap();
    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].presence, LibraryPresence::Partial);
}

#[test]
fn track_counts_survive_internal_whitespace_tagging_drift() {
    use crate::artist_news::{local_album_track_counts, presence_for, LibraryPresence};

    let conn = migrated_conn();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
         VALUES ('/music/one.flac', 'T1', 'Pink Floyd', 'Eclipse', 1, 0)",
        [],
    )
    .unwrap();
    // Same artist tag, but with an extra internal space. SQL's
    // `lower(trim(x))` grouping treats this as a distinct artist, while
    // Rust's `normalize()` collapses both to "pink floyd". If counting
    // happens on the SQL side, this second track lands in its own group of
    // one and the two real tracks are never summed together.
    conn.execute(
        "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
         VALUES ('/music/two.flac', 'T2', 'Pink  Floyd', 'Eclipse', 1, 0)",
        [],
    )
    .unwrap();

    let counts = local_album_track_counts(&conn).unwrap();
    assert_eq!(
        presence_for(&counts, "Pink Floyd", "Eclipse"),
        LibraryPresence::Complete,
        "two tracks tagged with an internal-whitespace-only artist variant must still count as one owned album"
    );
}
