use crate::db::Db;

use super::{
    add_tracks_in, create_in, create_smart_in, create_with_tracks_in_db, ensure_role_playlist_in,
    find_role_playlist_in, get_in, list_in, list_smart_in, move_position_in, playlist_role_in,
    remove_positions_in, rename_in, track_ids_in, PlaylistSummary, SmartPlaylist,
};

pub fn create(db: &Db, name: &str) -> Result<i64, rusqlite::Error> {
    let conn = db.conn();
    create_in(conn, name)
}

pub fn rename(db: &Db, id: i64, name: &str) -> Result<usize, rusqlite::Error> {
    let conn = db.conn();
    rename_in(conn, id, name)
}

pub fn list(db: &Db) -> Result<Vec<PlaylistSummary>, rusqlite::Error> {
    let conn = db.conn();
    list_in(conn)
}

pub fn get(db: &Db, id: i64) -> Result<Option<PlaylistSummary>, rusqlite::Error> {
    let conn = db.conn();
    get_in(conn, id)
}

pub fn track_ids(db: &Db, playlist_id: i64) -> Result<Vec<i64>, rusqlite::Error> {
    let conn = db.conn();
    track_ids_in(conn, playlist_id)
}

pub fn add_tracks(db: &Db, playlist_id: i64, track_ids: &[i64]) -> Result<u32, rusqlite::Error> {
    let conn = db.conn();
    add_tracks_in(conn, playlist_id, track_ids)
}

pub fn create_with_tracks(db: &Db, name: &str, track_ids: &[i64]) -> Result<i64, rusqlite::Error> {
    let conn = db.conn();
    create_with_tracks_in_db(conn, name, track_ids)
}

pub fn find_role_playlist(db: &Db, role: &str) -> Result<Option<i64>, rusqlite::Error> {
    let conn = db.conn();
    find_role_playlist_in(conn, role)
}

pub fn ensure_role_playlist(db: &Db, name: &str, role: &str) -> Result<i64, rusqlite::Error> {
    let conn = db.conn();
    ensure_role_playlist_in(conn, name, role)
}

pub fn playlist_role(db: &Db, id: i64) -> Result<Option<String>, rusqlite::Error> {
    let conn = db.conn();
    playlist_role_in(conn, id)
}

pub fn remove_positions(
    db: &Db,
    playlist_id: i64,
    positions: &[u32],
) -> Result<u32, rusqlite::Error> {
    let conn = db.conn();
    remove_positions_in(conn, playlist_id, positions)
}

pub fn move_position(db: &Db, playlist_id: i64, from: u32, to: u32) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    move_position_in(conn, playlist_id, from, to)
}

pub fn create_smart(
    db: &Db,
    name: &str,
    rules_json: &str,
    sort_field: &str,
    sort_dir: &str,
    limit_count: Option<i64>,
) -> Result<i64, rusqlite::Error> {
    let conn = db.conn();
    create_smart_in(conn, name, rules_json, sort_field, sort_dir, limit_count)
}

pub fn list_smart(db: &Db) -> Result<Vec<SmartPlaylist>, rusqlite::Error> {
    let conn = db.conn();
    list_smart_in(conn)
}
