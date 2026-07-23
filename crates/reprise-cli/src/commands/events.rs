//! `events tail` — dump the cross-process change log (debugging aid).

use reprise_core::events;
use rusqlite::Connection;
use serde_json::Value;

use crate::error::CliError;
use crate::json_models;
use crate::output::{print_json, sanitize_for_terminal};

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
            // entity/entity_id/operation are core-generated today, but a
            // settings entity_id echoes a key that can carry hand-edited text —
            // sanitize every untrusted field before it reaches the terminal.
            println!(
                "{}\t{}\t{}\t{}\t{}",
                change.id,
                sanitize_for_terminal(&change.entity),
                sanitize_for_terminal(&change.entity_id),
                sanitize_for_terminal(&change.operation),
                change.at
            );
        }
    }
    Ok(())
}
