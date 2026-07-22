//! `playlist` subcommands: list, show, create, rename, delete.

use reprise_core::library::playlists::{self, PlaylistSummary};
use reprise_core::models::Track;
use reprise_core::queries;
use reprise_core::view_source::ViewSource;
use rusqlite::Connection;
use serde_json::{json, Value};

use crate::error::CliError;
use crate::json_models;
use crate::output::{format_duration_ms, print_json};
use crate::retry::{rusqlite_is_busy, with_retry};

/// One page of `query_track_window`; core caps a single window at 500 rows, so
/// listing a whole playlist pages in these steps.
const PAGE: i64 = 500;

/// Wraps a mutating facade call in the busy-retry policy and maps its error.
fn retry_write<T>(op: impl FnMut() -> Result<T, rusqlite::Error>) -> Result<T, CliError> {
    with_retry(op, rusqlite_is_busy).map_err(CliError::from)
}

/// Looks a playlist up by id, or returns [`CliError::NotFound`]. Uses the
/// single-row `playlists::get` facade rather than scanning `list()`.
fn require_playlist(conn: &Connection, id: i64) -> Result<PlaylistSummary, CliError> {
    playlists::get(conn, id)?.ok_or_else(|| CliError::NotFound(format!("playlist {id}")))
}

/// Lists every manual playlist with its track count.
pub fn list(conn: &Connection, json_output: bool) -> Result<(), CliError> {
    let playlists = playlists::list(conn)?;
    if json_output {
        let rows: Vec<Value> = playlists
            .iter()
            .map(json_models::playlist_summary)
            .collect();
        print_json(&Value::Array(rows));
    } else if playlists.is_empty() {
        println!("no playlists");
    } else {
        for playlist in &playlists {
            println!(
                "{}\t{} ({} tracks)",
                playlist.id, playlist.name, playlist.track_count
            );
        }
    }
    Ok(())
}

/// Fetches every track of a playlist in playlist order, paging past core's
/// per-window cap.
fn all_playlist_tracks(conn: &mut Connection, id: i64) -> Result<Vec<Track>, CliError> {
    let source = ViewSource::Playlist(id);
    let mut tracks = Vec::new();
    let mut offset = 0i64;
    loop {
        let page = queries::query_track_window(
            conn,
            &source,
            "playlist_order",
            "asc",
            "",
            offset,
            PAGE,
            &[],
        )?;
        let fetched = page.len() as i64;
        tracks.extend(page);
        if fetched < PAGE {
            break;
        }
        offset += PAGE;
    }
    Ok(tracks)
}

/// Shows a playlist header and its ordered tracks.
pub fn show(conn: &mut Connection, id: i64, json_output: bool) -> Result<(), CliError> {
    let summary = require_playlist(conn, id)?;
    let tracks = all_playlist_tracks(conn, id)?;
    if json_output {
        let rows: Vec<Value> = tracks.iter().map(json_models::track).collect();
        print_json(&json!({
            "id": summary.id,
            "name": summary.name,
            "track_count": summary.track_count,
            "tracks": rows,
        }));
    } else {
        println!(
            "{} — {} ({} tracks)",
            summary.id, summary.name, summary.track_count
        );
        for track in &tracks {
            println!(
                "  {}\t{} - {} [{}]",
                track.id,
                track.artist,
                track.title,
                format_duration_ms(track.duration_ms)
            );
        }
    }
    Ok(())
}

/// Creates a playlist, optionally seeding it with `tracks`. Both paths write
/// exactly one `change_log` row through the core facade.
pub fn create(
    conn: &mut Connection,
    name: &str,
    tracks: &[i64],
    json_output: bool,
) -> Result<(), CliError> {
    let id = if tracks.is_empty() {
        retry_write(|| playlists::create(conn, name))?
    } else {
        retry_write(|| playlists::create_with_tracks(conn, name, tracks))?
    };
    if json_output {
        print_json(&json!({ "id": id, "name": name, "track_count": tracks.len() }));
    } else {
        println!("created playlist {id}: {name}");
    }
    Ok(())
}

/// Renames an existing playlist. The core facade reports rows affected and logs
/// no event for a no-op, so a missing id is surfaced from its `0` return rather
/// than a separate pre-check (which was a TOCTOU workaround).
pub fn rename(
    conn: &mut Connection,
    id: i64,
    name: &str,
    json_output: bool,
) -> Result<(), CliError> {
    let renamed = retry_write(|| playlists::rename(conn, id, name))?;
    if renamed == 0 {
        return Err(CliError::NotFound(format!("playlist {id}")));
    }
    if json_output {
        print_json(&json!({ "id": id, "name": name }));
    } else {
        println!("renamed playlist {id} to {name}");
    }
    Ok(())
}

/// Deletes a playlist. Refuses without `--yes`; reports a stale/absent target
/// as not-found so a caller never sees a false "deleted".
pub fn delete(
    conn: &mut Connection,
    id: i64,
    yes: bool,
    json_output: bool,
) -> Result<(), CliError> {
    if !yes {
        return Err(CliError::ConfirmationRequired(format!(
            "refusing to delete playlist {id} without --yes"
        )));
    }
    let summary = require_playlist(conn, id)?;
    let name = summary.name.clone();
    let deleted = retry_write(|| playlists::delete(conn, id, &name))?;
    if !deleted {
        return Err(CliError::NotFound(format!("playlist {id}")));
    }
    if json_output {
        print_json(&json!({ "id": id, "deleted": true }));
    } else {
        println!("deleted playlist {id}");
    }
    Ok(())
}
