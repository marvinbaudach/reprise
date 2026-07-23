//! `search` — paginated track search over the library.

use reprise_core::queries;
use reprise_core::view_source::ViewSource;
use rusqlite::Connection;
use serde_json::{json, Value};

use crate::error::CliError;
use crate::json_models;
use crate::output::{format_duration_ms, print_json, sanitize_for_terminal};

/// Searches the library for `query` (matched against title, artist, album and
/// genre) and prints the requested window plus the total match count.
pub fn run(
    conn: &mut Connection,
    query: &str,
    limit: i64,
    offset: i64,
    json_output: bool,
) -> Result<(), CliError> {
    if limit < 0 {
        return Err(CliError::InvalidInput(
            "--limit must not be negative".to_string(),
        ));
    }
    if offset < 0 {
        return Err(CliError::InvalidInput(
            "--offset must not be negative".to_string(),
        ));
    }

    let source = ViewSource::Library;
    let total = queries::query_track_count(conn, &source, query, &[])?;
    let tracks =
        queries::query_track_window(conn, &source, "title", "asc", query, offset, limit, &[])?;

    if json_output {
        let rows: Vec<Value> = tracks.iter().map(json_models::track).collect();
        print_json(&json!({
            "query": query,
            "total": total,
            "offset": offset,
            "limit": limit,
            "tracks": rows,
        }));
    } else if tracks.is_empty() {
        println!("no matches for {query:?} ({total} total)");
    } else {
        println!("{total} matches for {query:?}:");
        for track in &tracks {
            println!(
                "  {}\t{} - {} [{}]",
                track.id,
                sanitize_for_terminal(&track.artist),
                sanitize_for_terminal(&track.title),
                format_duration_ms(track.duration_ms)
            );
        }
    }
    Ok(())
}
