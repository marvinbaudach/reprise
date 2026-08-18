//! Schema v77: the one-time cleanup of YouTube channel images.

use rusqlite::Connection;

/// The columns this migration touches, in the shape the real schema gives them.
fn table(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE podcast_subscriptions (
           id INTEGER PRIMARY KEY,
           kind TEXT NOT NULL,
           feed_url TEXT NOT NULL UNIQUE,
           title TEXT NOT NULL,
           author TEXT,
           image_url TEXT,
           auto_download INTEGER NOT NULL DEFAULT 0,
           added_at INTEGER NOT NULL,
           removed_at INTEGER,
           last_fetch_at INTEGER
         );",
    )
    .unwrap();
}

fn subscription(
    conn: &Connection,
    id: i64,
    kind: &str,
    image_url: Option<&str>,
    last_fetch_at: Option<i64>,
) {
    conn.execute(
        "INSERT INTO podcast_subscriptions
           (id, kind, feed_url, title, author, image_url, auto_download, added_at,
            removed_at, last_fetch_at)
         VALUES (?1, ?2, ?3, ?4, NULL, ?5, 0, 1, NULL, ?6)",
        rusqlite::params![
            id,
            kind,
            format!("https://example.test/{id}"),
            format!("Source {id}"),
            image_url,
            last_fetch_at
        ],
    )
    .unwrap();
}

fn row(conn: &Connection, id: i64) -> (Option<String>, Option<i64>) {
    conn.query_row(
        "SELECT image_url, last_fetch_at FROM podcast_subscriptions WHERE id = ?1",
        [id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .unwrap()
}

#[test]
fn v77_clears_video_thumbnails_and_leaves_real_channel_images_alone() {
    let conn = Connection::open_in_memory().unwrap();
    table(&conn);

    subscription(
        &conn,
        1,
        "youtube",
        Some("https://i.ytimg.com/vi/ksu_4tR47F0/hq720.jpg?sqp=-oaymwE"),
        Some(1_000),
    );
    subscription(&conn, 2, "youtube", None, Some(1_000));
    subscription(
        &conn,
        3,
        "youtube",
        Some("https://yt3.googleusercontent.com/ytc/AIdro=s900"),
        Some(1_000),
    );
    subscription(
        &conn,
        4,
        "rss",
        Some("https://i.ytimg.com/vi/other/hq720.jpg"),
        Some(1_000),
    );

    conn.pragma_update(None, "user_version", 76).unwrap();
    crate::db_podcast_channel_image::migrate_v77(&conn).unwrap();

    // The video thumbnail is gone and the subscription is due again.
    assert_eq!(row(&conn, 1), (None, None));
    // A subscription that never had an image is made due too, so it gets one.
    assert_eq!(row(&conn, 2), (None, None));
    // A real channel avatar survives untouched, timestamp included.
    assert_eq!(
        row(&conn, 3),
        (
            Some("https://yt3.googleusercontent.com/ytc/AIdro=s900".to_string()),
            Some(1_000)
        )
    );
    // RSS is not part of this at all, whatever its image happens to look like.
    assert_eq!(
        row(&conn, 4),
        (
            Some("https://i.ytimg.com/vi/other/hq720.jpg".to_string()),
            Some(1_000)
        )
    );

    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 77);
}

#[test]
fn v77_does_not_run_twice() {
    let conn = Connection::open_in_memory().unwrap();
    table(&conn);

    conn.pragma_update(None, "user_version", 77).unwrap();
    subscription(
        &conn,
        1,
        "youtube",
        Some("https://i.ytimg.com/vi/late/hq720.jpg"),
        Some(2_000),
    );

    crate::db_podcast_channel_image::migrate_v77(&conn).unwrap();

    // A row that appeared after the migration already ran is none of its
    // business — otherwise every launch would re-clear images the app just
    // stored.
    assert_eq!(
        row(&conn, 1),
        (
            Some("https://i.ytimg.com/vi/late/hq720.jpg".to_string()),
            Some(2_000)
        )
    );
}
