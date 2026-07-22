//! `events tail` — dump the cross-process change log (debugging aid).

use reprise_core::events;
use rusqlite::Connection;
use serde_json::Value;

use crate::error::CliError;
use crate::json_models;
use crate::output::print_json;

/// Prints change-log rows with an id greater than `since`, oldest first. This
/// is a debug window onto the same outbox the running app consumes for live
/// refresh. `--json` includes each row's per-process writer token (see
/// `json_models`); the human-readable form stays a compact one line per row.
pub fn tail(conn: &Connection, since: i64, json_output: bool) -> Result<(), CliError> {
    let changes = events::read_since(conn, since, None)?;
    if json_output {
        let rows: Vec<Value> = changes.iter().map(json_models::change).collect();
        print_json(&Value::Array(rows));
    } else if changes.is_empty() {
        println!("no changes since {since}");
    } else {
        for change in &changes {
            println!(
                "{}\t{}\t{}\t{}\t{}",
                change.id, change.entity, change.entity_id, change.operation, change.at
            );
        }
    }
    Ok(())
}
