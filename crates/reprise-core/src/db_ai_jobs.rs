//! Schema migration for the experimental AI-audio feature slice (Track 2 of
//! the multi-frontend-core plan, section 2.4): the generic `ai_jobs` job
//! queue, the `track_provenance` registry for AI-manipulated/-generated
//! tracks, and the `playlists.role` column that turns one manual playlist
//! into the drop-target "conversion" system playlist.
//!
//! This is deliberately one migration covering three shapes: they ship as a
//! single, isolated, removable feature (plan 2.3 "isolated and removable")
//! and no reader of one exists without the others. Kept in its own
//! `db_*.rs` file with a `migrate_v27` entry point, exactly like every
//! post-v18 migration (see `db_change_log::migrate_v26`).

use rusqlite::Connection;

/// The AI-job queue plus the two companion shapes. Design notes:
///
/// * `ai_jobs.kind` (`'instrumental'` today) and `provenance.kind` keep the
///   pipeline generic — a later "generate from prompt" job art reuses the
///   same table with a NULL `source_track_id` (plan 2.4/10).
/// * `source_track_id`/`result_track_id`/`provenance.source_track_id` are
///   `ON DELETE SET NULL` (not `CASCADE`): deleting the *original* must leave
///   the instrumental — and any historical job row — intact, with the source
///   link degrading to the textual provenance the tags carry (Beschluss 16).
///   `track_provenance.track_id` is the one `CASCADE`: a provenance row has no
///   meaning once its own track is gone (deleting the instrumental is a plain
///   delete, re-creatable anytime).
/// * The dedup index enforces Beschluss 16's "skip + reference the existing"
///   at the storage layer: at most one *open* (`queued`/`running`) job, or one
///   *successful* job whose result track still exists, per
///   `(kind, source_track_id, params_fingerprint)`. A `failed`/`cancelled`
///   job — and a `done` job whose instrumental was later deleted
///   (`result_track_id` set back to NULL by the FK) — drop out of the index,
///   so the work becomes re-enqueueable exactly when it should.
/// * `progress_permille` lives in the row (not the change log): the worker
///   rewrites it at a rate the caller throttles (plan 2.2), while the change
///   log only gets lifecycle transitions.
/// * `auto_promote` persists the save-intent (decision 15: MCP/CLI
///   create-instrumental saves by default) so the worker that later completes
///   the render knows to promote it without the enqueuer still being around. It
///   is deliberately **absent** from the dedup index — the intent does not
///   change a job's identity, so re-enqueuing the same work with a different
///   intent still deduplicates to the existing job.
const SCHEMA_V27: &str = r#"
CREATE TABLE ai_jobs (
  id                 INTEGER PRIMARY KEY AUTOINCREMENT,
  kind               TEXT    NOT NULL,
  batch_id           TEXT,
  source_track_id    INTEGER REFERENCES tracks(id) ON DELETE SET NULL,
  params_json        TEXT    NOT NULL,
  params_fingerprint TEXT    NOT NULL,
  status             TEXT    NOT NULL DEFAULT 'queued'
                       CHECK (status IN ('queued','running','done','failed','cancelled')),
  progress_permille  INTEGER NOT NULL DEFAULT 0
                       CHECK (progress_permille BETWEEN 0 AND 1000),
  claimed_by         INTEGER,
  lease_expires_at   INTEGER,
  cancel_requested   INTEGER NOT NULL DEFAULT 0 CHECK (cancel_requested IN (0, 1)),
  auto_promote       INTEGER NOT NULL DEFAULT 0 CHECK (auto_promote IN (0, 1)),
  error_kind         TEXT,
  created_at         INTEGER NOT NULL,
  started_at         INTEGER,
  finished_at        INTEGER,
  result_track_id    INTEGER REFERENCES tracks(id) ON DELETE SET NULL
);
CREATE UNIQUE INDEX idx_ai_jobs_dedup
  ON ai_jobs(kind, source_track_id, params_fingerprint)
  WHERE status IN ('queued', 'running')
     OR (status = 'done' AND result_track_id IS NOT NULL);
CREATE INDEX idx_ai_jobs_status ON ai_jobs(status);
CREATE INDEX idx_ai_jobs_batch ON ai_jobs(batch_id) WHERE batch_id IS NOT NULL;

CREATE TABLE track_provenance (
  track_id        INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
  kind            TEXT    NOT NULL,
  ai              INTEGER NOT NULL DEFAULT 1 CHECK (ai IN (0, 1)),
  source_track_id INTEGER REFERENCES tracks(id) ON DELETE SET NULL,
  source_text     TEXT,
  source_mbid     TEXT,
  model           TEXT,
  created_at      INTEGER NOT NULL
);
CREATE INDEX idx_track_provenance_ai ON track_provenance(ai) WHERE ai = 1;
CREATE INDEX idx_track_provenance_source ON track_provenance(source_track_id);

ALTER TABLE playlists ADD COLUMN role TEXT;
"#;

/// Applies schema v27 — the AI-jobs/provenance/role migration — following the
/// one-transaction, version-gated shape every post-v18 step uses (see
/// `db::migrate`'s doc comment for why schema change and `user_version` bump
/// share a transaction).
pub(crate) fn migrate_v27(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 27 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(SCHEMA_V27)?;
    transaction.pragma_update(None, "user_version", 27)?;
    transaction.commit()
}
