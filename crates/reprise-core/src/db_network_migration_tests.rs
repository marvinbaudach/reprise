use super::*;
use rusqlite::OptionalExtension;

fn open_database_at(version: i64) -> Connection {
    let conn = open(None).unwrap();
    for (schema_version, schema) in [
        (1, SCHEMA_V1),
        (2, SCHEMA_V2),
        (3, SCHEMA_V3),
        (4, SCHEMA_V4),
        (5, SCHEMA_V5),
        (6, SCHEMA_V6),
        (7, SCHEMA_V7),
        (8, SCHEMA_V8),
        (9, SCHEMA_V9),
        (10, SCHEMA_V10),
        (11, SCHEMA_V11),
        (12, SCHEMA_V12),
        (13, SCHEMA_V13),
        (14, SCHEMA_V14),
        (15, SCHEMA_V15),
        (17, SCHEMA_V17),
        (18, SCHEMA_V18),
    ] {
        if schema_version > version {
            break;
        }
        conn.execute_batch(schema).unwrap();
    }
    conn.pragma_update(None, "user_version", version).unwrap();
    conn
}

fn migrate_with_empty_caches(conn: &Connection) {
    let cover_cache = tempfile::tempdir().unwrap();
    let portrait_cache = tempfile::tempdir().unwrap();
    migrate_with_cache_dirs(conn, cover_cache.path(), portrait_cache.path()).unwrap();
}

fn open_pre_online_gate_database() -> Connection {
    let conn = open(None).unwrap();
    migrate_with_empty_caches(&conn);
    conn.execute(
        "DELETE FROM settings WHERE key = ?1",
        [crate::online_sources::ENABLED_KEY],
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 48).unwrap();
    conn
}

fn stored_online_gate(conn: &Connection) -> Option<String> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        [crate::online_sources::ENABLED_KEY],
        |row| row.get(0),
    )
    .optional()
    .unwrap()
}

fn first_enable_completed(conn: &Connection) -> bool {
    crate::library::settings::get_bool_in(
        conn,
        crate::library::settings::ONLINE_SOURCES_FIRST_ENABLE_COMPLETED_KEY,
        false,
    )
    .unwrap()
}

#[test]
fn net_2a_fresh_database_migration_stores_the_master_off() {
    let conn = open(None).unwrap();

    migrate_with_empty_caches(&conn);

    assert_eq!(stored_online_gate(&conn).as_deref(), Some("0"));
    assert!(!first_enable_completed(&conn));
}

#[test]
fn online_gate_existing_subscription_migration_stores_the_master_on() {
    let conn = open_pre_online_gate_database();
    conn.execute(
        "INSERT INTO podcast_subscriptions \
         (kind, feed_url, title, added_at) VALUES ('podcast', 'https://example.test/feed', 'Show', 1)",
        [],
    )
    .unwrap();

    migrate_with_empty_caches(&conn);

    assert_eq!(stored_online_gate(&conn).as_deref(), Some("1"));
    assert!(first_enable_completed(&conn));
}

#[test]
fn online_gate_existing_database_without_use_stores_the_master_off() {
    let conn = open_pre_online_gate_database();

    migrate_with_empty_caches(&conn);

    assert_eq!(stored_online_gate(&conn).as_deref(), Some("0"));
    assert!(!first_enable_completed(&conn));
}

#[test]
fn online_gate_explicit_master_value_survives_migration_untouched() {
    for value in ["0", "1"] {
        let conn = open_pre_online_gate_database();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)",
            rusqlite::params![crate::online_sources::ENABLED_KEY, value],
        )
        .unwrap();

        migrate_with_empty_caches(&conn);

        assert_eq!(stored_online_gate(&conn).as_deref(), Some(value));
        assert!(first_enable_completed(&conn));
    }
}

#[test]
fn online_gate_existing_radio_favourite_counts_as_demonstrable_use() {
    let conn = open_pre_online_gate_database();
    conn.execute(
        "INSERT INTO radio_stations (name, stream_url, added_at) \
         VALUES ('Station', 'https://example.test/radio', 1)",
        [],
    )
    .unwrap();

    migrate_with_empty_caches(&conn);

    assert_eq!(stored_online_gate(&conn).as_deref(), Some("1"));
}

#[test]
fn online_gate_existing_positive_image_caches_count_as_demonstrable_use() {
    for cache_kind in ["cover", "portrait"] {
        let conn = open_pre_online_gate_database();
        let cover_cache = tempfile::tempdir().unwrap();
        let portrait_cache = tempfile::tempdir().unwrap();
        let cache = if cache_kind == "cover" {
            cover_cache.path()
        } else {
            portrait_cache.path()
        };
        std::fs::write(cache.join("used.jpg"), b"cached").unwrap();

        migrate_with_cache_dirs(&conn, cover_cache.path(), portrait_cache.path()).unwrap();

        assert_eq!(stored_online_gate(&conn).as_deref(), Some("1"));
    }
}

#[test]
fn online_gate_negative_cache_markers_do_not_count_as_demonstrable_use() {
    let conn = open_pre_online_gate_database();
    let cover_cache = tempfile::tempdir().unwrap();
    let portrait_cache = tempfile::tempdir().unwrap();
    std::fs::write(cover_cache.path().join("miss.notfound"), b"").unwrap();
    std::fs::write(portrait_cache.path().join("miss.notfound"), b"").unwrap();

    migrate_with_cache_dirs(&conn, cover_cache.path(), portrait_cache.path()).unwrap();

    assert_eq!(stored_online_gate(&conn).as_deref(), Some("0"));
}

#[test]
fn net_2a_migration_preserves_existing_cover_usage() {
    let conn = open_database_at(12);
    let cover_cache = tempfile::tempdir().unwrap();
    let portrait_cache = tempfile::tempdir().unwrap();
    std::fs::write(cover_cache.path().join("used.jpg"), b"cached").unwrap();

    migrate_with_cache_dirs(&conn, cover_cache.path(), portrait_cache.path()).unwrap();

    assert!(crate::modules::is_enabled_in(&conn, &crate::modules::COVER_DOWNLOAD_MODULE).unwrap());
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, SUPPORTED_SCHEMA_VERSION);
}

#[test]
fn net_2a_migration_preserves_existing_portrait_usage() {
    let conn = open_database_at(12);
    let cover_cache = tempfile::tempdir().unwrap();
    let portrait_cache = tempfile::tempdir().unwrap();
    std::fs::write(portrait_cache.path().join("used.png"), b"cached").unwrap();

    migrate_with_cache_dirs(&conn, cover_cache.path(), portrait_cache.path()).unwrap();

    assert!(
        crate::modules::is_enabled_in(&conn, &crate::modules::ARTIST_PORTRAITS_MODULE).unwrap()
    );
}

#[test]
fn net_2a_migration_preserves_online_lyrics_for_existing_databases() {
    let conn = open_database_at(12);

    migrate_with_empty_caches(&conn);

    assert!(crate::modules::is_enabled_in(&conn, &crate::modules::ONLINE_LYRICS_MODULE).unwrap());
}

#[test]
fn net_2a_migration_carries_artist_news_opt_in_to_new_releases() {
    let conn = open_database_at(12);
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('module.artist_news.enabled', '1')",
        [],
    )
    .unwrap();

    migrate_with_empty_caches(&conn);

    assert!(crate::modules::is_enabled_in(&conn, &crate::modules::NEW_RELEASES_MODULE).unwrap());
}

#[test]
fn net_2a_migration_ignores_negative_cache_markers() {
    let conn = open_database_at(12);
    let cover_cache = tempfile::tempdir().unwrap();
    let portrait_cache = tempfile::tempdir().unwrap();
    std::fs::write(cover_cache.path().join("miss.notfound"), b"").unwrap();
    std::fs::write(portrait_cache.path().join("miss.notfound"), b"").unwrap();

    migrate_with_cache_dirs(&conn, cover_cache.path(), portrait_cache.path()).unwrap();

    assert!(!crate::modules::is_enabled_in(&conn, &crate::modules::COVER_DOWNLOAD_MODULE).unwrap());
    assert!(
        !crate::modules::is_enabled_in(&conn, &crate::modules::ARTIST_PORTRAITS_MODULE).unwrap()
    );
}

#[test]
fn net_2a_migration_preserves_explicit_opt_outs() {
    let conn = open_database_at(12);
    for key in [
        "module.cover_download.enabled",
        "module.artist_portraits.enabled",
        "module.online_lyrics.enabled",
        "module.new_releases.enabled",
    ] {
        conn.execute("INSERT INTO settings (key, value) VALUES (?1, '0')", [key])
            .unwrap();
    }
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('module.artist_news.enabled', '1')",
        [],
    )
    .unwrap();
    let cover_cache = tempfile::tempdir().unwrap();
    let portrait_cache = tempfile::tempdir().unwrap();
    std::fs::write(cover_cache.path().join("used.jpg"), b"cached").unwrap();
    std::fs::write(portrait_cache.path().join("used.png"), b"cached").unwrap();

    migrate_with_cache_dirs(&conn, cover_cache.path(), portrait_cache.path()).unwrap();

    for module in [
        &crate::modules::COVER_DOWNLOAD_MODULE,
        &crate::modules::ARTIST_PORTRAITS_MODULE,
        &crate::modules::ONLINE_LYRICS_MODULE,
        &crate::modules::NEW_RELEASES_MODULE,
    ] {
        assert!(!crate::modules::is_enabled_in(&conn, module).unwrap());
    }
}

#[test]
fn net_2a_an_explicitly_enabled_online_module_is_demonstrable_use() {
    // Concerts and New Releases fetch on demand and cache nothing, so none of
    // the data-trace signals can see them. Without this, someone who follows
    // only concerts loses the feature on update: the module stays on while the
    // gate above it goes off.
    for key in [
        "module.concerts.enabled",
        "module.new_releases.enabled",
        "module.listenbrainz.enabled",
    ] {
        let conn = open_pre_online_gate_database();
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, '1')",
            [key],
        )
        .unwrap();

        migrate_with_empty_caches(&conn);

        assert_eq!(
            stored_online_gate(&conn).as_deref(),
            Some("1"),
            "{key} on must count as prior online use"
        );
    }
}

#[test]
fn net_2a_an_enabled_local_module_is_not_demonstrable_use() {
    // Song Visuals and Library Doctor both default to on and never make a
    // request, so a pattern match over `module.%.enabled` would report every
    // database as having used online features.
    let conn = open_pre_online_gate_database();
    for key in [
        "module.song_visuals.enabled",
        "module.library_doctor.enabled",
    ] {
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, '1')",
            [key],
        )
        .unwrap();
    }

    migrate_with_empty_caches(&conn);

    assert_eq!(stored_online_gate(&conn).as_deref(), Some("0"));
}

#[test]
fn net_2a_v15_database_runs_network_grandfathering_at_v16() {
    let conn = open_database_at(15);
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('module.cover_download.enabled', '0')",
        [],
    )
    .unwrap();
    let cover_cache = tempfile::tempdir().unwrap();
    let portrait_cache = tempfile::tempdir().unwrap();
    std::fs::write(cover_cache.path().join("used.jpg"), b"cached").unwrap();

    migrate_with_cache_dirs(&conn, cover_cache.path(), portrait_cache.path()).unwrap();

    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, SUPPORTED_SCHEMA_VERSION);
    assert!(!crate::modules::is_enabled_in(&conn, &crate::modules::COVER_DOWNLOAD_MODULE).unwrap());
    assert!(crate::modules::is_enabled_in(&conn, &crate::modules::ONLINE_LYRICS_MODULE).unwrap());
}
