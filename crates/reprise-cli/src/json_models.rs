//! Stable JSON shapes for `--json` output.
//!
//! The CLI owns its machine-readable contract rather than serializing core
//! types directly: core structs are free to grow fields without silently
//! changing the CLI's output, and the shapes here are the ones tests pin. All
//! values are built with `serde_json::json!` so the crate needs no `serde`
//! derive dependency.

use reprise_core::ai_jobs::{AiJob, BatchProgress, EnqueueOutcome};
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

/// One AI job, as emitted by `jobs status` and `instrumental` commands. Every
/// surface (GTK conversion view, CLI, MCP) reads these same fields; `state` is
/// the lowercase status string and `progress_permille` is 0..=1000 — the same
/// number the GTK bar shows (plan 2.2). `result_track_id` is set only once a job
/// has been saved (promoted) into the library.
pub fn job(job: &AiJob) -> Value {
    json!({
        "id": job.id,
        "kind": job.kind,
        "batch_id": job.batch_id,
        "source_track_id": job.source_track_id,
        "state": job.state.as_str(),
        "progress_permille": job.progress_permille,
        "cancel_requested": job.cancel_requested,
        "error_kind": job.error_kind,
        "result_track_id": job.result_track_id,
        "created_at": job.created_at,
        "finished_at": job.finished_at,
    })
}

/// Aggregate progress for one batch, as emitted by `jobs status --batch` — the
/// same numbers that feed the conversion view's single progress bar.
pub fn batch_progress(batch_id: &str, progress: &BatchProgress) -> Value {
    json!({
        "batch_id": batch_id,
        "total": progress.total,
        "done": progress.done,
        "failed": progress.failed,
        "cancelled": progress.cancelled,
        "running": progress.running,
        "queued": progress.queued,
        "progress_permille": progress.permille,
    })
}

/// One track's enqueue outcome, as emitted by `instrumental create`. Either a
/// fresh `queued` job, or a `deduplicated` reference to pre-existing work
/// (Beschluss 16), with the saved result track id when one already exists.
pub fn enqueue_outcome(source_track_id: i64, outcome: EnqueueOutcome) -> Value {
    match outcome {
        EnqueueOutcome::Created { job_id } => json!({
            "source_track_id": source_track_id,
            "job_id": job_id,
            "outcome": "created",
        }),
        EnqueueOutcome::Deduplicated {
            job_id,
            result_track_id,
        } => json!({
            "source_track_id": source_track_id,
            "job_id": job_id,
            "outcome": "deduplicated",
            "result_track_id": result_track_id,
        }),
    }
}

/// One change-log row, as emitted by `events tail`. `writer` is the per-process
/// token of the connection that authored the change — useful for telling apart
/// which frontend wrote a row when several drive the same database; it is only
/// meaningful within one database's lifetime, not across separate processes.
pub fn change(change: &Change) -> Value {
    json!({
        "id": change.id,
        "entity": change.entity,
        "entity_id": change.entity_id,
        "op": change.operation,
        "writer": change.writer.value(),
        "at": change.at,
    })
}
