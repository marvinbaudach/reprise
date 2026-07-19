use super::*;

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

#[test]
fn net_2_migration_preserves_existing_cover_usage() {
    let conn = open_database_at(12);
    let cover_cache = tempfile::tempdir().unwrap();
    let portrait_cache = tempfile::tempdir().unwrap();
    std::fs::write(cover_cache.path().join("used.jpg"), b"cached").unwrap();

    migrate_with_cache_dirs(&conn, cover_cache.path(), portrait_cache.path()).unwrap();

    assert!(crate::modules::is_enabled(&conn, &crate::modules::COVER_DOWNLOAD_MODULE).unwrap());
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 21);
}

#[test]
fn net_2_migration_preserves_existing_portrait_usage() {
    let conn = open_database_at(12);
    let cover_cache = tempfile::tempdir().unwrap();
    let portrait_cache = tempfile::tempdir().unwrap();
    std::fs::write(portrait_cache.path().join("used.png"), b"cached").unwrap();

    migrate_with_cache_dirs(&conn, cover_cache.path(), portrait_cache.path()).unwrap();

    assert!(crate::modules::is_enabled(&conn, &crate::modules::ARTIST_PORTRAITS_MODULE).unwrap());
}

#[test]
fn net_2_migration_preserves_online_lyrics_for_existing_databases() {
    let conn = open_database_at(12);

    migrate_with_empty_caches(&conn);

    assert!(crate::modules::is_enabled(&conn, &crate::modules::ONLINE_LYRICS_MODULE).unwrap());
}

#[test]
fn net_2_migration_carries_artist_news_opt_in_to_new_releases() {
    let conn = open_database_at(12);
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('module.artist_news.enabled', '1')",
        [],
    )
    .unwrap();

    migrate_with_empty_caches(&conn);

    assert!(crate::modules::is_enabled(&conn, &crate::modules::NEW_RELEASES_MODULE).unwrap());
}

#[test]
fn net_2_migration_ignores_negative_cache_markers() {
    let conn = open_database_at(12);
    let cover_cache = tempfile::tempdir().unwrap();
    let portrait_cache = tempfile::tempdir().unwrap();
    std::fs::write(cover_cache.path().join("miss.notfound"), b"").unwrap();
    std::fs::write(portrait_cache.path().join("miss.notfound"), b"").unwrap();

    migrate_with_cache_dirs(&conn, cover_cache.path(), portrait_cache.path()).unwrap();

    assert!(!crate::modules::is_enabled(&conn, &crate::modules::COVER_DOWNLOAD_MODULE).unwrap());
    assert!(!crate::modules::is_enabled(&conn, &crate::modules::ARTIST_PORTRAITS_MODULE).unwrap());
}

#[test]
fn net_2_migration_preserves_explicit_opt_outs() {
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
        assert!(!crate::modules::is_enabled(&conn, module).unwrap());
    }
}

#[test]
fn net_2_v15_database_runs_network_grandfathering_at_v16() {
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
    assert_eq!(version, 21);
    assert!(!crate::modules::is_enabled(&conn, &crate::modules::COVER_DOWNLOAD_MODULE).unwrap());
    assert!(crate::modules::is_enabled(&conn, &crate::modules::ONLINE_LYRICS_MODULE).unwrap());
}
