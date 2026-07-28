//! Schema v32 regressions for podcasts and radio.

use super::*;
use crate::db::{self, DbError, SUPPORTED_SCHEMA_VERSION};

fn object_schema(conn: &Connection, table: &str) -> Vec<(String, String)> {
    let mut statement = conn
        .prepare(
            "SELECT name, COALESCE(sql, '') FROM sqlite_schema \
             WHERE (type = 'table' AND name = ?1) \
                OR (type = 'index' AND tbl_name = ?1) \
             ORDER BY type, name",
        )
        .unwrap();
    statement
        .query_map([table], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

fn reset_to_v31(conn: &Connection) {
    conn.execute_batch(
        "DROP TABLE podcast_episode_dismissals;
         DROP TABLE podcast_subscription_baselines;
         DROP TABLE podcast_episodes;
         DROP TABLE podcast_subscriptions;
         DROP TABLE radio_stations;
         PRAGMA user_version = 31;",
    )
    .unwrap();
}

#[test]
fn fresh_and_v31_upgrade_have_identical_source_schema() {
    let fresh = db::open(None).unwrap();
    db::migrate(&fresh).unwrap();
    let upgraded = db::open(None).unwrap();
    db::migrate(&upgraded).unwrap();
    reset_to_v31(&upgraded);

    db::migrate(&upgraded).unwrap();

    for table in [
        "podcast_subscriptions",
        "podcast_subscription_baselines",
        "podcast_episode_dismissals",
        "podcast_episodes",
        "radio_stations",
    ] {
        assert_eq!(
            object_schema(&upgraded, table),
            object_schema(&fresh, table),
            "{table} differs between fresh and upgraded databases"
        );
    }
    assert_eq!(
        upgraded
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        SUPPORTED_SCHEMA_VERSION
    );
}

#[test]
fn v33_adds_future_only_guid_baselines_with_subscription_cascade() {
    let conn = db::open(None).unwrap();
    db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO podcast_subscriptions
         (id, kind, feed_url, title, added_at)
         VALUES (1, 'rss', 'https://example.test/feed', 'Show', 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO podcast_subscription_baselines (subscription_id, guid)
         VALUES (1, 'known-guid')",
        [],
    )
    .unwrap();

    conn.execute("DELETE FROM podcast_subscriptions WHERE id = 1", [])
        .unwrap();

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM podcast_subscription_baselines",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn v34_adds_episode_tombstones_and_dismissals_with_subscription_cascade() {
    let conn = db::open(None).unwrap();
    db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO podcast_subscriptions
         (id, kind, feed_url, title, added_at)
         VALUES (1, 'youtube', 'https://example.test/channel', 'Channel', 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO podcast_episodes
         (subscription_id, guid, title, audio_url, first_seen_at, removed_at)
         VALUES (1, 'video-guid', 'Video', 'https://example.test/video', 1, 2)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO podcast_episode_dismissals (subscription_id, guid, removed_at)
         VALUES (1, 'video-guid', 2)",
        [],
    )
    .unwrap();

    conn.execute("DELETE FROM podcast_subscriptions WHERE id = 1", [])
        .unwrap();

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM podcast_episode_dismissals",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn v36_adds_download_sizes_and_rss_phone_sync_defaults() {
    let conn = db::open(None).unwrap();
    db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO podcast_subscriptions
         (id, kind, feed_url, title, added_at)
         VALUES (1, 'rss', 'https://example.test/feed', 'Show', 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO podcast_episodes
         (id, subscription_id, guid, title, audio_url, first_seen_at)
         VALUES (1, 1, 'episode', 'Episode', 'https://example.test/e.mp3', 1)",
        [],
    )
    .unwrap();

    let sync_to_phone = conn
        .query_row(
            "SELECT sync_to_phone FROM podcast_subscriptions WHERE id = 1",
            [],
            |row| row.get::<_, bool>(0),
        )
        .unwrap();
    let downloaded_bytes = conn
        .query_row(
            "SELECT downloaded_bytes FROM podcast_episodes WHERE id = 1",
            [],
            |row| row.get::<_, Option<i64>>(0),
        )
        .unwrap();

    assert!(!sync_to_phone);
    assert_eq!(downloaded_bytes, None);
}

#[test]
fn v37_adds_stable_per_device_podcast_selections() {
    let conn = db::open(None).unwrap();
    db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO podcast_subscriptions
         (id, kind, feed_url, title, added_at)
         VALUES (1, 'rss', 'https://example.test/feed', 'Show', 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO podcast_subscription_devices (subscription_id, device_id)
         VALUES (1, 'mtp:pixel-serial')",
        [],
    )
    .unwrap();

    let selected = conn
        .query_row(
            "SELECT device_id FROM podcast_subscription_devices
             WHERE subscription_id = 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap();

    assert_eq!(selected, "mtp:pixel-serial");
    assert!(conn
        .execute(
            "INSERT INTO podcast_subscription_devices (subscription_id, device_id)
             VALUES (1, '')",
            [],
        )
        .is_err());
}

#[test]
fn migration_is_idempotent() {
    let conn = db::open(None).unwrap();
    db::migrate(&conn).unwrap();
    let before = object_schema(&conn, "podcast_episodes");

    migrate_v32(&conn).unwrap();
    migrate_v33(&conn).unwrap();
    migrate_v34(&conn).unwrap();
    migrate_v36(&conn).unwrap();
    migrate_v37(&conn).unwrap();

    assert_eq!(object_schema(&conn, "podcast_episodes"), before);
}

#[test]
fn podcast_episode_delete_cascades_from_subscription() {
    let conn = db::open(None).unwrap();
    db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO podcast_subscriptions \
         (id, kind, feed_url, title, added_at) VALUES (1, 'rss', 'https://example.test/feed', 'Show', 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO podcast_episodes \
         (subscription_id, guid, title, audio_url, first_seen_at) \
         VALUES (1, 'episode-guid', 'Episode', 'https://example.test/episode.mp3', 1)",
        [],
    )
    .unwrap();

    conn.execute("DELETE FROM podcast_subscriptions WHERE id = 1", [])
        .unwrap();

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM podcast_episodes", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn newer_schema_is_refused_before_migration() {
    let conn = db::open(None).unwrap();
    conn.pragma_update(None, "user_version", SUPPORTED_SCHEMA_VERSION + 1)
        .unwrap();

    let error = db::migrate(&conn).unwrap_err();

    assert!(matches!(
        error,
        DbError::SchemaTooNew {
            found,
            supported
        } if found == SUPPORTED_SCHEMA_VERSION + 1 && supported == SUPPORTED_SCHEMA_VERSION
    ));
}
