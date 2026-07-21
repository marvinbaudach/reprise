//! Stable JSON shapes for `--json` output.
//!
//! The CLI owns its machine-readable contract rather than serializing core
//! types directly: core structs are free to grow fields without silently
//! changing the CLI's output, and the shapes here are the ones tests pin. All
//! values are built with `serde_json::json!` so the crate needs no `serde`
//! derive dependency.

use reprise_core::events::Change;
use reprise_core::library::playlists::PlaylistSummary;
use reprise_core::models::Track;
use serde_json::{json, Value};

/// One track, as emitted by `search` and `playlist show`.
pub fn track(track: &Track) -> Value {
    json!({
        "id": track.id,
        "title": track.title,
        "artist": track.artist,
        "album": track.album,
        "album_artist": track.album_artist,
        "genre": track.genre,
        "year": track.year,
        "track_no": track.track_no,
        "duration_ms": track.duration_ms,
        "rating": track.rating,
        "play_count": track.play_count,
        "path": track.path,
        "missing": track.is_missing(),
    })
}

/// One playlist row, as emitted by `playlist list`.
pub fn playlist_summary(summary: &PlaylistSummary) -> Value {
    json!({
        "id": summary.id,
        "name": summary.name,
        "track_count": summary.track_count,
    })
}

/// One change-log row, as emitted by `events tail`. The per-process writer
/// token is deliberately omitted: it carries no value across separate CLI
/// invocations (each process has its own) and core exposes no accessor for it.
pub fn change(change: &Change) -> Value {
    json!({
        "id": change.id,
        "entity": change.entity,
        "entity_id": change.entity_id,
        "op": change.operation,
        "at": change.at,
    })
}
