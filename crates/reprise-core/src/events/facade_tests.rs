//! T0.3 — every mutating core facade appends exactly one correct `change_log`
//! row in the same transaction as its mutation. These tests drive the public
//! facades and read back the change log through [`read_since`]; they are the
//! contract the GTK/CLI/MCP consumers rely on to see foreign writes.

use super::*;

use crate::library::{playlists, scanner, settings};
use crate::modules;

fn conn() -> crate::db::Db {
    crate::db::Db::open_in_memory().unwrap()
}

fn all_changes(conn: &crate::db::Db) -> Vec<Change> {
    read_since(conn, 0, None).unwrap()
}

fn seed_track(conn: &rusqlite::Connection, id: i64) {
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, added_at) \
         VALUES (?1, ?2, 'T', 'A', 1)",
        rusqlite::params![id, format!("/music/track-{id}.flac")],
    )
    .unwrap();
}

#[test]
fn create_playlist_emits_one_create_event() {
    let conn = conn();
    let id = playlists::create(&conn, "Mix").unwrap();

    let changes = all_changes(&conn);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].entity, "playlist");
    assert_eq!(changes[0].entity_id, id.to_string());
    assert_eq!(changes[0].operation, "create");
}

#[test]
fn rename_playlist_emits_one_rename_event() {
    let conn = conn();
    let id = playlists::create(&conn, "Mix").unwrap();
    let baseline = all_changes(&conn).last().unwrap().id;

    playlists::rename(&conn, id, "Renamed").unwrap();

    let changes = read_since(&conn, baseline, None).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].entity, "playlist");
    assert_eq!(changes[0].entity_id, id.to_string());
    assert_eq!(changes[0].operation, "rename");
}

#[test]
fn rename_missing_playlist_emits_no_event() {
    let conn = conn();
    // No playlist with this id: the rename matches nothing, so it must change
    // zero rows and log nothing (no phantom rename event).
    assert_eq!(playlists::rename(&conn, 999, "Ghost").unwrap(), 0);
    assert!(all_changes(&conn).is_empty());
}

#[test]
fn delete_playlist_emits_one_delete_event() {
    let conn = conn();
    let id = playlists::create(&conn, "Doomed").unwrap();
    let baseline = all_changes(&conn).last().unwrap().id;

    assert!(playlists::delete(&conn, id, "Doomed").unwrap());

    let changes = read_since(&conn, baseline, None).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].entity, "playlist");
    assert_eq!(changes[0].entity_id, id.to_string());
    assert_eq!(changes[0].operation, "delete");
}

#[test]
fn stale_delete_emits_no_event() {
    let conn = conn();
    let id = playlists::create(&conn, "Renamed").unwrap();
    let baseline = all_changes(&conn).last().unwrap().id;

    // The (id, name) identity no longer matches: a stale request deletes
    // nothing and must therefore log nothing.
    assert!(!playlists::delete(&conn, id, "Old name").unwrap());

    assert!(read_since(&conn, baseline, None).unwrap().is_empty());
}

#[test]
fn add_tracks_emits_one_add_event() {
    let conn = conn();
    seed_track(conn.conn(), 1);
    seed_track(conn.conn(), 2);
    let id = playlists::create(&conn, "Mix").unwrap();
    let baseline = all_changes(&conn).last().unwrap().id;

    playlists::add_tracks(&conn, id, &[1, 2]).unwrap();

    let changes = read_since(&conn, baseline, None).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].entity, "playlist");
    assert_eq!(changes[0].entity_id, id.to_string());
    assert_eq!(changes[0].operation, "add");
}

#[test]
fn add_no_tracks_emits_no_event() {
    let conn = conn();
    let id = playlists::create(&conn, "Mix").unwrap();
    let baseline = all_changes(&conn).last().unwrap().id;

    assert_eq!(playlists::add_tracks(&conn, id, &[]).unwrap(), 0);

    assert!(read_since(&conn, baseline, None).unwrap().is_empty());
}

#[test]
fn remove_positions_emits_one_remove_event() {
    let conn = conn();
    seed_track(conn.conn(), 1);
    seed_track(conn.conn(), 2);
    let id = playlists::create(&conn, "Mix").unwrap();
    playlists::add_tracks(&conn, id, &[1, 2]).unwrap();
    let baseline = all_changes(&conn).last().unwrap().id;

    playlists::remove_positions(&conn, id, &[0]).unwrap();

    let changes = read_since(&conn, baseline, None).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].entity, "playlist");
    assert_eq!(changes[0].entity_id, id.to_string());
    assert_eq!(changes[0].operation, "remove");
}

#[test]
fn move_position_emits_one_move_event() {
    let conn = conn();
    seed_track(conn.conn(), 1);
    seed_track(conn.conn(), 2);
    let id = playlists::create(&conn, "Mix").unwrap();
    playlists::add_tracks(&conn, id, &[1, 2]).unwrap();
    let baseline = all_changes(&conn).last().unwrap().id;

    playlists::move_position(&conn, id, 0, 1).unwrap();

    let changes = read_since(&conn, baseline, None).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].entity, "playlist");
    assert_eq!(changes[0].entity_id, id.to_string());
    assert_eq!(changes[0].operation, "move");
}

#[test]
fn create_with_tracks_emits_one_create_event() {
    let conn = conn();
    seed_track(conn.conn(), 1);

    let id = playlists::create_with_tracks(&conn, "Mix", &[1]).unwrap();

    let changes = all_changes(&conn);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].entity, "playlist");
    assert_eq!(changes[0].entity_id, id.to_string());
    assert_eq!(changes[0].operation, "create");
}

#[test]
fn create_smart_emits_one_create_event() {
    let conn = conn();

    let id = playlists::create_smart(&conn, "Top", "[]", "rating", "desc", None).unwrap();

    let changes = all_changes(&conn);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].entity, "smart_playlist");
    assert_eq!(changes[0].entity_id, id.to_string());
    assert_eq!(changes[0].operation, "create");
}

#[test]
fn create_smart_dedup_hit_emits_no_event() {
    let conn = conn();
    let first = playlists::create_smart(&conn, "Top", "[]", "rating", "desc", None).unwrap();
    let baseline = all_changes(&conn).last().unwrap().id;

    // An identical definition returns the existing row instead of inserting;
    // nothing changed, so nothing is logged.
    let again = playlists::create_smart(&conn, "Top", "[]", "rating", "desc", None).unwrap();

    assert_eq!(again, first);
    assert!(read_since(&conn, baseline, None).unwrap().is_empty());
}

#[test]
fn set_setting_emits_one_settings_event() {
    let conn = conn();

    settings::set_setting(&conn, "color_scheme", "dark").unwrap();

    let changes = all_changes(&conn);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].entity, "settings");
    assert_eq!(changes[0].entity_id, "color_scheme");
    assert_eq!(changes[0].operation, "set");
}

#[test]
fn module_toggle_emits_one_settings_event_keyed_by_module() {
    let conn = conn();

    modules::set_enabled(&conn, &modules::LISTENBRAINZ_MODULE, true).unwrap();

    let changes = all_changes(&conn);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].entity, "settings");
    assert_eq!(changes[0].entity_id, "module.listenbrainz.enabled");
    assert_eq!(changes[0].operation, "set");
}

#[test]
fn scan_completion_emits_one_collective_library_event() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("music");
    std::fs::create_dir(&root).unwrap();
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    std::fs::copy(&fixture, root.join("song.flac")).unwrap();
    let conn = conn();

    let outcome = scanner::scan_folder(&conn, &root).unwrap();
    assert!(matches!(outcome, scanner::ScanOutcome::Completed(_)));

    let changes = all_changes(&conn);
    assert_eq!(
        changes.len(),
        1,
        "a scan that added a track logs exactly once"
    );
    assert_eq!(changes[0].entity, "library");
    assert_eq!(changes[0].operation, "scan");
}

#[test]
fn scan_with_no_catalog_change_emits_no_event() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("music");
    std::fs::create_dir(&root).unwrap();
    let conn = conn();

    let outcome = scanner::scan_folder(&conn, &root).unwrap();
    assert!(matches!(outcome, scanner::ScanOutcome::Completed(_)));

    assert!(
        all_changes(&conn).is_empty(),
        "an empty scan that changes nothing logs nothing"
    );
}
