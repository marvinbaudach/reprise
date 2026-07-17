//! `ViewSource::Playlist(id)` window/count/ids queries, plus the unwindowed
//! "whole playlist, in order" query M3U export needs. Split out of the
//! former single-file `queries.rs` (Refactoring & Extensibility Task 1) — a
//! pure move, no behavior change.

use crate::models::Track;

use super::clauses::{
    filter_clause, like_pattern, order_expr_and_dir, row_to_id, row_to_playlist_track, PRESENT,
};
use super::queue::QUEUE_LIMIT;
use super::MAX_WINDOW_LIMIT;
use rusqlite::Connection;

/// Builds the parameterized SELECT for a `Playlist(id)` window — see the
/// module doc's `Playlist(id)` section for the join shape, the `"playlist_
/// order"` sentinel, and the duplicates-as-separate-rows behavior.
/// [`PRESENT`] is applied here too: a track that later vanishes from disk
/// drops out of every playlist's view and resurfaces only in the dedicated
/// `Missing` source, exactly like the library view.
///
/// The trailing `pt.position` column (index 22, read by `row_to_playlist_
/// track`) is the durable fix for the "remove from playlist deletes the
/// wrong row" bug: it surfaces each row's *true* `playlist_tracks.position`
/// regardless of what `ORDER BY` this query used, so `ui::track_actions::
/// remove_selected_from_playlist` can resolve a selected on-screen row back
/// to the position `library::playlists::remove_positions` actually needs —
/// see `Track::playlist_position`'s doc comment.
fn build_playlist_track_query(sort_field: &str, sort_dir: &str, has_filter: bool) -> String {
    let (order_expr, dir) = order_expr_and_dir(sort_field, sort_dir);
    let filter_clause = filter_clause(has_filter, 4);
    format!(
        "SELECT tracks.id, tracks.path, tracks.title, tracks.artist, tracks.album, \
         tracks.album_artist, tracks.year, tracks.track_no, tracks.genre, \
         tracks.duration_ms, tracks.bitrate_kbps, tracks.rating, tracks.play_count, \
         tracks.last_played_at, tracks.added_at, tracks.file_mtime, tracks.missing_since, \
         tracks.missing_reason, tracks.untagged, tracks.file_size, tracks.device, \
         tracks.inode, pt.position \
         FROM tracks JOIN playlist_tracks pt ON pt.track_id = tracks.id \
         WHERE pt.playlist_id = ?3 AND {PRESENT}{filter_clause} \
         ORDER BY {order_expr} {dir} LIMIT ?1 OFFSET ?2"
    )
}

pub(super) fn query_track_window_playlist(
    conn: &mut Connection,
    playlist_id: i64,
    sort_field: &str,
    sort_dir: &str,
    filter: &str,
    offset: i64,
    limit: i64,
) -> Result<Vec<Track>, rusqlite::Error> {
    let limit = limit.clamp(0, MAX_WINDOW_LIMIT);
    let has_filter = !filter.trim().is_empty();
    let sql = build_playlist_track_query(sort_field, sort_dir, has_filter);
    let mut stmt = conn.prepare(&sql)?;
    let like = like_pattern(filter.trim());
    let rows = if has_filter {
        stmt.query_map(
            rusqlite::params![limit, offset, playlist_id, like],
            row_to_playlist_track,
        )?
    } else {
        stmt.query_map(
            rusqlite::params![limit, offset, playlist_id],
            row_to_playlist_track,
        )?
    };
    rows.collect()
}

pub(super) fn query_track_count_playlist(
    conn: &Connection,
    playlist_id: i64,
    filter: &str,
) -> Result<i64, rusqlite::Error> {
    let has_filter = !filter.trim().is_empty();
    let sql = format!(
        "SELECT count(*) FROM tracks JOIN playlist_tracks pt ON pt.track_id = tracks.id \
         WHERE pt.playlist_id = ?1 AND {PRESENT}{}",
        filter_clause(has_filter, 2)
    );
    if has_filter {
        let like = like_pattern(filter.trim());
        conn.query_row(&sql, rusqlite::params![playlist_id, like], |r| r.get(0))
    } else {
        conn.query_row(&sql, rusqlite::params![playlist_id], |r| r.get(0))
    }
}

pub(super) fn query_track_ids_playlist(
    conn: &Connection,
    playlist_id: i64,
    filter: &str,
) -> Result<Vec<i64>, rusqlite::Error> {
    // Deliberately always `pt.position` order, never the caller's current
    // column sort (see the module doc's `Playlist(id)` section): "play this
    // playlist" always follows playlist order, even if the visible window
    // is temporarily sorted by a clicked column header.
    let has_filter = !filter.trim().is_empty();
    let sql = format!(
        "SELECT tracks.id FROM tracks JOIN playlist_tracks pt ON pt.track_id = tracks.id \
         WHERE pt.playlist_id = ?1 AND {PRESENT}{} \
         ORDER BY pt.position ASC LIMIT {QUEUE_LIMIT}",
        filter_clause(has_filter, 2)
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = if has_filter {
        let like = like_pattern(filter.trim());
        stmt.query_map(rusqlite::params![playlist_id, like], row_to_id)?
    } else {
        stmt.query_map(rusqlite::params![playlist_id], row_to_id)?
    };
    rows.collect()
}

/// Loads every track in playlist `playlist_id`, in playlist order
/// (`playlist_tracks.position` ascending), with no window/page limit —
/// distinct from `query_track_window`'s `Playlist` arm, which is capped at
/// `MAX_WINDOW_LIMIT` for one `ColumnView` page. Stage 3 Task 7 (M3U export)
/// needs every track the playlist has, in order, in one call; reusing the
/// windowed query would mean the caller paging through in a loop for no
/// benefit at the scale a single playlist reaches. Missing tracks (`missing
/// = 1`) are excluded, matching every other playlist-facing query in this
/// module (a track that vanished from disk shouldn't be written into an
/// exported M3U with a dead path). Capped at `QUEUE_LIMIT` for defense in
/// depth, same reasoning as `query_track_ids`'s per-source caps.
pub fn query_playlist_tracks_full(
    conn: &Connection,
    playlist_id: i64,
) -> Result<Vec<Track>, rusqlite::Error> {
    let sql = format!(
        "SELECT tracks.id, tracks.path, tracks.title, tracks.artist, tracks.album, \
         tracks.album_artist, tracks.year, tracks.track_no, tracks.genre, \
         tracks.duration_ms, tracks.bitrate_kbps, tracks.rating, tracks.play_count, \
         tracks.last_played_at, tracks.added_at, tracks.file_mtime, tracks.missing_since, \
         tracks.missing_reason, tracks.untagged, tracks.file_size, tracks.device, \
         tracks.inode, pt.position \
         FROM tracks JOIN playlist_tracks pt ON pt.track_id = tracks.id \
         WHERE pt.playlist_id = ?1 AND {PRESENT} \
         ORDER BY pt.position ASC LIMIT {QUEUE_LIMIT}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params![playlist_id], row_to_playlist_track)?;
    rows.collect()
}
