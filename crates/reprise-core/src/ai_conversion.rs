//! The conversion playlist — the drop-target semantics of plan 2.4/1.
//!
//! Smart playlists in this codebase are rule queries with no drop semantics
//! (plan 1.2/8), so the "conversion playlist" is modelled as a **system
//! playlist with a role** (`role = 'conversion'`) whose *insertions create
//! jobs* rather than membership rows: dragging a song onto it enqueues an
//! instrumental job (or, if one already covers that track, references the
//! existing work — the "hint, not a second render" of Beschluss 15/16). The
//! rendered results live in `ai_jobs` + the staging store, not in
//! `playlist_tracks`; the playlist row exists only as the named, role-marked
//! drop target the sidebar shows.
//!
//! This module is the one place the removable AI feature couples the generic
//! role-playlist primitives (`library::playlists`) to the job queue
//! (`ai_jobs`), keeping `playlists` itself feature-agnostic.

use rusqlite::Connection;

use crate::ai_jobs::{self, BatchOutcome, EnqueueOutcome};
use crate::ai_staging::StagingStore;
use crate::library::playlists;

/// The `playlists.role` value marking the conversion drop playlist.
pub const CONVERSION_ROLE: &str = "conversion";

/// The display name of the conversion playlist (English per AGENTS.md; user-
/// facing translation comes later via gettext).
pub const CONVERSION_PLAYLIST_NAME: &str = "Conversion";

/// Ensures the conversion drop playlist exists and returns its id. Idempotent
/// — safe to call on every startup and every drop.
pub fn ensure_conversion_playlist(conn: &Connection) -> Result<i64, rusqlite::Error> {
    playlists::ensure_role_playlist(conn, CONVERSION_PLAYLIST_NAME, CONVERSION_ROLE)
}

/// The conversion playlist's id, or `None` if it has never been created.
pub fn conversion_playlist(conn: &Connection) -> Result<Option<i64>, rusqlite::Error> {
    playlists::find_role_playlist(conn, CONVERSION_ROLE)
}

/// Adds one track to the conversion playlist: ensures the playlist exists, then
/// enqueues an instrumental job for the track. A track already covered by an
/// open/staged/saved job returns [`EnqueueOutcome::Deduplicated`] — the
/// caller surfaces that as a hint rather than a second render (Beschluss 16).
pub fn add_to_conversion(
    conn: &Connection,
    staging: &StagingStore,
    track_id: i64,
    model_id: &str,
    now: i64,
) -> Result<EnqueueOutcome, rusqlite::Error> {
    ensure_conversion_playlist(conn)?;
    ai_jobs::enqueue_instrumental(conn, staging, track_id, model_id, now)
}

/// Adds several tracks (a multi-select drop) to the conversion playlist under
/// one batch, deduping each. Ensures the playlist exists first. `auto_promote`
/// carries the caller's save-intent (decision 15: the MCP/CLI batch path saves
/// by default, the GTK drop stages for a manual decision) onto every fresh job,
/// where the completion path honors it.
pub fn add_batch_to_conversion(
    conn: &Connection,
    staging: &StagingStore,
    track_ids: &[i64],
    model_id: &str,
    auto_promote: bool,
    now: i64,
) -> Result<BatchOutcome, rusqlite::Error> {
    ensure_conversion_playlist(conn)?;
    ai_jobs::enqueue_instrumental_batch(conn, staging, track_ids, model_id, auto_promote, now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::playlists;

    fn migrated() -> Connection {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn
    }

    fn seed_track(conn: &Connection, id: i64) {
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at, file_mtime, file_size) \
             VALUES (?1, ?2, 'T', 'A', 1, 1, 1)",
            rusqlite::params![id, format!("/music/{id}.flac")],
        )
        .unwrap();
    }

    #[test]
    fn ensure_conversion_playlist_is_idempotent_and_role_marked() {
        let conn = migrated();
        let first = ensure_conversion_playlist(&conn).unwrap();
        let second = ensure_conversion_playlist(&conn).unwrap();
        assert_eq!(first, second, "the conversion playlist is a singleton");

        assert_eq!(
            playlists::playlist_role(&conn, first).unwrap().as_deref(),
            Some(CONVERSION_ROLE)
        );
        assert_eq!(conversion_playlist(&conn).unwrap(), Some(first));
        // Exactly one playlist row exists.
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM playlists", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn the_conversion_playlist_is_hidden_from_the_user_list() {
        let conn = migrated();
        ensure_conversion_playlist(&conn).unwrap();
        playlists::create(&conn, "My Mix").unwrap();

        let names: Vec<String> = playlists::list(&conn)
            .unwrap()
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(
            names,
            ["My Mix"],
            "role playlists never appear in the user list"
        );
    }

    #[test]
    fn dropping_a_track_enqueues_a_job_and_creates_the_playlist() {
        let conn = migrated();
        let staging = StagingStore::new("/unused");
        seed_track(&conn, 1);

        let outcome = add_to_conversion(&conn, &staging, 1, "m@1", 0).unwrap();

        assert!(matches!(outcome, EnqueueOutcome::Created { .. }));
        assert!(conversion_playlist(&conn).unwrap().is_some());
        let job = ai_jobs::get_job(&conn, outcome.job_id()).unwrap().unwrap();
        assert_eq!(job.source_track_id, Some(1));
    }

    #[test]
    fn dropping_an_already_converting_track_is_a_hint_not_a_second_job() {
        let conn = migrated();
        let staging = StagingStore::new("/unused");
        seed_track(&conn, 1);
        let first = add_to_conversion(&conn, &staging, 1, "m@1", 0).unwrap();

        let second = add_to_conversion(&conn, &staging, 1, "m@1", 10).unwrap();

        assert_eq!(
            second,
            EnqueueOutcome::Deduplicated {
                job_id: first.job_id(),
                result_track_id: None,
            }
        );
        let jobs: i64 = conn
            .query_row("SELECT COUNT(*) FROM ai_jobs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(jobs, 1);
    }

    #[test]
    fn a_multi_select_drop_enqueues_a_batch() {
        let conn = migrated();
        let staging = StagingStore::new("/unused");
        for id in [1, 2, 3] {
            seed_track(&conn, id);
        }

        let batch = add_batch_to_conversion(&conn, &staging, &[1, 2, 3], "m@1", false, 0).unwrap();

        assert_eq!(batch.jobs.len(), 3);
        assert!(conversion_playlist(&conn).unwrap().is_some());
        assert_eq!(
            ai_jobs::list_jobs_in_batch(&conn, &batch.batch_id)
                .unwrap()
                .len(),
            3
        );
    }
}
