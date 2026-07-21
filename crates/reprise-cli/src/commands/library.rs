//! `library summary` — library-wide totals.

use reprise_core::queries;
use rusqlite::Connection;
use serde_json::json;

use crate::error::CliError;
use crate::output::{format_duration_ms, print_json};

/// Prints the total number of present tracks and their combined duration.
pub fn summary(conn: &Connection, json_output: bool) -> Result<(), CliError> {
    let stats = queries::query_library_stats(conn, "")?;
    if json_output {
        print_json(&json!({
            "track_count": stats.track_count,
            "total_duration_ms": stats.total_duration_ms,
        }));
    } else {
        println!(
            "{} tracks, {} total",
            stats.track_count,
            format_duration_ms(stats.total_duration_ms)
        );
    }
    Ok(())
}
