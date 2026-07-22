//! The AI-audio job queue facade — enqueue/dedup, the worker claim/lease/
//! heartbeat/reclaim protocol, progress, cancellation, and typed status
//! transitions (plan 2.4/2). "Instrumental" is the first `kind`, not the
//! system's name; the shape is generic so a later job art reuses it.
//!
//! ## Where each concern lives (plan 2.2)
//!
//! * **change_log** gets **lifecycle transitions only** — `enqueue`, `start`,
//!   `done`, `save`, `fail`, `cancel` — each appended in the *same*
//!   transaction as the mutation via [`crate::events::in_txn`], so a job event
//!   never lands without the state change and vice versa. Progress is **not**
//!   a lifecycle event.
//! * **`progress_permille`** lives in the row and is rewritten in place. The
//!   caller throttles it (≤ 2 writes/s, plan 2.2); this facade just writes.
//! * The **clock is injected** (`now: i64`) everywhere a timestamp or a lease
//!   deadline is computed, so lease expiry/reclaim is testable without sleeps.
//! * Every facade that **reads then writes** in one transaction — the enqueue
//!   dedup probe, the `claim_next` candidate select, and the `finish_owned`
//!   progress read — opens with `BEGIN IMMEDIATE` (via
//!   [`crate::events::in_txn_immediate`], or directly in `claim_next`). Under
//!   real concurrency a DEFERRED read-then-write takes a snapshot and then fails
//!   its write-lock upgrade with a raw `SQLITE_BUSY`/`SQLITE_BUSY_SNAPSHOT` that
//!   `busy_timeout` never retries; taking the write lock upfront makes a loser
//!   wait its turn and see the deduplicated / next-candidate result the API
//!   promises. Write-only transitions (cancel, discard, attach) stay ordinary,
//!   since their first statement already takes the lock.
//!
//! ## Dedup (Beschluss 16) and the staging wrinkle
//!
//! Re-triggering existing work is a *skip that references the existing result*,
//! never a silent double-render. A DB `UNIQUE` index is the concurrency guard
//! for two racing enqueues; [`enqueue_instrumental`] additionally references a
//! finished-but-unsaved render still sitting in the staging store, and lets
//! the work be re-enqueued once that render is gone (discarded, or the
//! promoted instrumental deleted — Beschluss 16).

use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};

use crate::ai_staging::StagingStore;
use crate::events;

/// The first (and, in v1, only) job kind.
pub const INSTRUMENTAL_KIND: &str = "instrumental";

/// The change-log entity every job lifecycle event is recorded under.
const JOB_ENTITY: &str = "ai_job";

/// A job's lifecycle state — the typed mirror of the `ai_jobs.status` CHECK
/// set. `Done` means the render exists (in staging when `result_track_id` is
/// `None`, promoted into the library once it is set); `Cancelled` also covers
/// a user-discarded staging render.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

impl JobState {
    /// The exact string stored in `ai_jobs.status`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    /// Parses a stored status. `None` for an unrecognized value — the row
    /// mapper turns that into a hard error, since the column is CHECK-
    /// constrained and an unknown status is a real corruption, not something
    /// to paper over.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(Self::Queued),
            "running" => Some(Self::Running),
            "done" => Some(Self::Done),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

/// A job row, in the shape every surface (GTK conversion view, CLI, MCP)
/// reads. Internal bookkeeping columns (`claimed_by`, `lease_expires_at`,
/// `params_json`) are deliberately omitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiJob {
    pub id: i64,
    pub kind: String,
    pub batch_id: Option<String>,
    pub source_track_id: Option<i64>,
    pub params_fingerprint: String,
    pub state: JobState,
    pub progress_permille: u16,
    pub cancel_requested: bool,
    pub error_kind: Option<String>,
    pub result_track_id: Option<i64>,
    pub created_at: i64,
    pub finished_at: Option<i64>,
}

/// What a claiming worker receives — enough to run the backend, plus the lease
/// deadline it must heartbeat before.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimedJob {
    pub id: i64,
    pub kind: String,
    pub source_track_id: Option<i64>,
    pub params_json: String,
    pub params_fingerprint: String,
    pub lease_expires_at: i64,
}

/// The result of an enqueue: a fresh job, or a reference to pre-existing work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnqueueOutcome {
    /// A new `queued` job was created.
    Created { job_id: i64 },
    /// An open, saved, or still-staged job already covers this request — no
    /// new work. `result_track_id` is `Some` only for an already-saved job.
    Deduplicated {
        job_id: i64,
        result_track_id: Option<i64>,
    },
}

impl EnqueueOutcome {
    /// The job id either outcome refers to.
    pub fn job_id(self) -> i64 {
        match self {
            Self::Created { job_id } | Self::Deduplicated { job_id, .. } => job_id,
        }
    }
}

/// A batch enqueue: the shared `batch_id` plus each source track's outcome, in
/// input order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchOutcome {
    pub batch_id: String,
    pub jobs: Vec<EnqueueOutcome>,
}

/// The outcome of a cancel request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelOutcome {
    /// A `queued` job was cancelled outright (no worker involved).
    CancelledImmediately,
    /// A `running` job was flagged; its worker acks between chunks.
    CancelRequested,
    /// The job is already terminal (done/failed/cancelled) or absent.
    NotCancellable,
}

/// What a heartbeat tells the worker: whether it still owns the lease, and
/// whether a cancel has been requested since.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeartbeatOutcome {
    pub still_owner: bool,
    pub cancel_requested: bool,
}

/// Aggregate progress for a batch — powers the conversion view's single
/// progress bar (plan 2.4/7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BatchProgress {
    pub total: i64,
    pub done: i64,
    pub failed: i64,
    pub cancelled: i64,
    pub running: i64,
    pub queued: i64,
    /// Overall completion in permille, the mean of every member's
    /// `progress_permille` (a `done` member counts as 1000).
    pub permille: u16,
}

/// Builds the canonical params for an instrumental job. v1 has no tunable
/// knobs, so the fingerprint *is* the model id (`"<name>@<version>"`); adding
/// parameters later folds them into both without touching callers.
fn instrumental_params(model_id: &str) -> (String, String) {
    let params_json = serde_json::json!({ "model": model_id }).to_string();
    let fingerprint = model_id.to_string();
    (params_json, fingerprint)
}

/// Enqueues an instrumental job for `source_track_id`, or references existing
/// work (Beschluss 16). `staging` lets the dedup see a finished-but-unsaved
/// render so re-dragging a converted track is a hint, not a second render.
pub fn enqueue_instrumental(
    conn: &Connection,
    staging: &StagingStore,
    source_track_id: i64,
    model_id: &str,
    now: i64,
) -> Result<EnqueueOutcome, rusqlite::Error> {
    let (params_json, fingerprint) = instrumental_params(model_id);
    // IMMEDIATE: this reads (dedup probe) then writes (INSERT). See
    // `events::in_txn_immediate` for why a DEFERRED read-then-write races into a
    // raw `SQLITE_BUSY` instead of deduplicating.
    events::in_txn_immediate(conn, |conn| {
        enqueue_one(
            conn,
            staging,
            &NewJobSpec {
                source_track_id: Some(source_track_id),
                params_json: &params_json,
                fingerprint: &fingerprint,
                batch_id: None,
                // The conversion-playlist drop path stages the render for a
                // manual save decision; it never auto-promotes (decision 15).
                auto_promote: false,
            },
            now,
        )
    })
}

/// Enqueues one instrumental job per source track under a shared `batch_id`,
/// deduping each independently. One transaction: the whole batch lands or none
/// of it does. `auto_promote` records the save-intent on every freshly-created
/// job (decision 15: the MCP/CLI batch path saves by default); it is persisted,
/// not part of a job's dedup identity, and honored by the completion path
/// [`crate::ai_promotion::complete_render`].
pub fn enqueue_instrumental_batch(
    conn: &Connection,
    staging: &StagingStore,
    source_track_ids: &[i64],
    model_id: &str,
    auto_promote: bool,
    now: i64,
) -> Result<BatchOutcome, rusqlite::Error> {
    let (params_json, fingerprint) = instrumental_params(model_id);
    let batch_id = new_batch_id();
    // IMMEDIATE: each `enqueue_one` reads (dedup probe) then writes.
    let jobs = events::in_txn_immediate(conn, |conn| {
        let mut jobs = Vec::with_capacity(source_track_ids.len());
        for &source_track_id in source_track_ids {
            jobs.push(enqueue_one(
                conn,
                staging,
                &NewJobSpec {
                    source_track_id: Some(source_track_id),
                    params_json: &params_json,
                    fingerprint: &fingerprint,
                    batch_id: Some(&batch_id),
                    auto_promote,
                },
                now,
            )?);
        }
        Ok(jobs)
    })?;
    Ok(BatchOutcome { batch_id, jobs })
}

/// The immutable descriptor of a job to (maybe) create — everything
/// [`enqueue_one`] needs beyond the connection, staging store, and clock.
/// Bundled into one value so the shared body keeps a small, clear signature.
struct NewJobSpec<'a> {
    source_track_id: Option<i64>,
    params_json: &'a str,
    fingerprint: &'a str,
    batch_id: Option<&'a str>,
    /// The persisted save-intent (decision 15); not part of dedup identity.
    auto_promote: bool,
}

/// The shared enqueue body — must run inside a transaction (`in_txn`).
fn enqueue_one(
    conn: &Connection,
    staging: &StagingStore,
    spec: &NewJobSpec<'_>,
    now: i64,
) -> Result<EnqueueOutcome, rusqlite::Error> {
    // 1. An open job, or an already-saved one with a live result, wins outright.
    let live: Option<(i64, Option<i64>)> = conn
        .query_row(
            "SELECT id, result_track_id FROM ai_jobs \
             WHERE kind = ?1 AND source_track_id IS ?2 AND params_fingerprint = ?3 \
               AND (status IN ('queued', 'running') \
                    OR (status = 'done' AND result_track_id IS NOT NULL)) \
             ORDER BY id LIMIT 1",
            params![INSTRUMENTAL_KIND, spec.source_track_id, spec.fingerprint],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if let Some((job_id, result_track_id)) = live {
        return Ok(EnqueueOutcome::Deduplicated {
            job_id,
            result_track_id,
        });
    }
    // 2. A finished-but-unsaved render still in staging is referenced too — but
    //    only while its file is actually there. A `done`/NULL row whose render
    //    is gone (discarded, or promoted-then-deleted) frees the work.
    let staged: Option<i64> = conn
        .query_row(
            "SELECT id FROM ai_jobs \
             WHERE kind = ?1 AND source_track_id IS ?2 AND params_fingerprint = ?3 \
               AND status = 'done' AND result_track_id IS NULL \
             ORDER BY id LIMIT 1",
            params![INSTRUMENTAL_KIND, spec.source_track_id, spec.fingerprint],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(job_id) = staged {
        if staging.exists(job_id) {
            return Ok(EnqueueOutcome::Deduplicated {
                job_id,
                result_track_id: None,
            });
        }
    }
    // 3. Genuinely new work.
    conn.execute(
        "INSERT INTO ai_jobs \
           (kind, batch_id, source_track_id, params_json, params_fingerprint, status, \
            auto_promote, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, 'queued', ?6, ?7)",
        params![
            INSTRUMENTAL_KIND,
            spec.batch_id,
            spec.source_track_id,
            spec.params_json,
            spec.fingerprint,
            i64::from(spec.auto_promote),
            now
        ],
    )?;
    let job_id = conn.last_insert_rowid();
    events::record(conn, JOB_ENTITY, &job_id.to_string(), "enqueue")?;
    Ok(EnqueueOutcome::Created { job_id })
}

/// Claims the next runnable job for `worker` (a per-worker token), marking it
/// `running` with a lease expiring at `now + lease_secs`. Runnable means
/// `queued`, or `running` with an **expired** lease (reclaiming a crashed
/// worker — plan 2.4/2). Exactly one worker wins a given job: the conditional
/// `UPDATE` inside a transaction means a loser simply sees the next candidate.
///
/// The transaction is `BEGIN IMMEDIATE`: it reads a candidate, then writes the
/// claim. A DEFERRED transaction would take a read snapshot first and, under
/// concurrent claimers, fail the write-lock upgrade with a raw `SQLITE_BUSY`
/// that `busy_timeout` never retries; taking the write lock upfront makes a
/// loser wait its turn and then re-select, exactly as documented.
pub fn claim_next(
    conn: &Connection,
    worker: i64,
    now: i64,
    lease_secs: i64,
) -> Result<Option<ClaimedJob>, rusqlite::Error> {
    let lease_expires_at = now.saturating_add(lease_secs);
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let claimed = loop {
        let candidate: Option<(i64, String)> = tx
            .query_row(
                "SELECT id, status FROM ai_jobs \
                 WHERE status = 'queued' OR (status = 'running' AND lease_expires_at < ?1) \
                 ORDER BY id LIMIT 1",
                [now],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((id, prior_status)) = candidate else {
            break None;
        };
        let changed = tx.execute(
            "UPDATE ai_jobs \
             SET status = 'running', claimed_by = ?1, lease_expires_at = ?2, \
                 started_at = COALESCE(started_at, ?3) \
             WHERE id = ?4 \
               AND (status = 'queued' OR (status = 'running' AND lease_expires_at < ?3))",
            params![worker, lease_expires_at, now, id],
        )?;
        if changed == 1 {
            // A fresh start (queued -> running) is a lifecycle transition; a
            // reclaim (running -> running) is not, so it logs nothing.
            if prior_status == "queued" {
                events::record(&tx, JOB_ENTITY, &id.to_string(), "start")?;
            }
            break Some(id);
        }
        // Lost the race for this candidate; the loop re-selects the next one.
    };
    let Some(id) = claimed else {
        tx.commit()?;
        return Ok(None);
    };
    let job = tx.query_row(
        "SELECT id, kind, source_track_id, params_json, params_fingerprint, lease_expires_at \
         FROM ai_jobs WHERE id = ?1",
        [id],
        |row| {
            Ok(ClaimedJob {
                id: row.get(0)?,
                kind: row.get(1)?,
                source_track_id: row.get(2)?,
                params_json: row.get(3)?,
                params_fingerprint: row.get(4)?,
                lease_expires_at: row.get(5)?,
            })
        },
    )?;
    tx.commit()?;
    Ok(Some(job))
}

/// Extends the lease for a running job the caller still owns and reports back
/// whether a cancel has been requested — the worker calls this between chunks.
/// Not a lifecycle transition, so it appends no change-log event.
pub fn heartbeat(
    conn: &Connection,
    job_id: i64,
    worker: i64,
    now: i64,
    lease_secs: i64,
) -> Result<HeartbeatOutcome, rusqlite::Error> {
    let lease_expires_at = now.saturating_add(lease_secs);
    let still_owner = conn.execute(
        "UPDATE ai_jobs SET lease_expires_at = ?1 \
         WHERE id = ?2 AND claimed_by = ?3 AND status = 'running'",
        params![lease_expires_at, job_id, worker],
    )? == 1;
    let cancel_requested = conn
        .query_row(
            "SELECT cancel_requested FROM ai_jobs WHERE id = ?1",
            [job_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .unwrap_or(0)
        != 0;
    Ok(HeartbeatOutcome {
        still_owner,
        cancel_requested,
    })
}

/// Writes a running job's progress in place (owner-guarded). Returns whether
/// the caller still owns the job. No change-log event (plan 2.2). The caller
/// is responsible for rate-limiting these writes.
pub fn set_progress(
    conn: &Connection,
    job_id: i64,
    worker: i64,
    permille: u16,
) -> Result<bool, rusqlite::Error> {
    let clamped = permille.min(crate::stem_separation::PROGRESS_COMPLETE);
    let changed = conn.execute(
        "UPDATE ai_jobs SET progress_permille = ?1 \
         WHERE id = ?2 AND claimed_by = ?3 AND status = 'running'",
        params![clamped, job_id, worker],
    )?;
    Ok(changed == 1)
}

/// Marks a running job `done` (render written to staging). `result_track_id`
/// stays `NULL` — the job is staged until promotion attaches a track. Owner-
/// guarded; records the `done` lifecycle event.
pub fn mark_done(
    conn: &Connection,
    job_id: i64,
    worker: i64,
    now: i64,
) -> Result<bool, rusqlite::Error> {
    finish_owned(conn, job_id, worker, "done", None, now, "done")
}

/// Marks a running job `failed` with a diagnostic `error_kind`. Owner-guarded;
/// records the `fail` lifecycle event.
pub fn mark_failed(
    conn: &Connection,
    job_id: i64,
    worker: i64,
    error_kind: &str,
    now: i64,
) -> Result<bool, rusqlite::Error> {
    finish_owned(
        conn,
        job_id,
        worker,
        "failed",
        Some(error_kind),
        now,
        "fail",
    )
}

/// Acks a requested cancel on a running job the worker owns
/// (`running` -> `cancelled`). Owner-guarded and gated on `cancel_requested`,
/// so it can only complete a real cancel. Records the `cancel` event.
pub fn mark_cancelled(
    conn: &Connection,
    job_id: i64,
    worker: i64,
    now: i64,
) -> Result<bool, rusqlite::Error> {
    events::in_txn(conn, |conn| {
        let changed = conn.execute(
            "UPDATE ai_jobs SET status = 'cancelled', finished_at = ?1 \
             WHERE id = ?2 AND claimed_by = ?3 AND status = 'running' AND cancel_requested = 1",
            params![now, job_id, worker],
        )?;
        if changed == 1 {
            events::record(conn, JOB_ENTITY, &job_id.to_string(), "cancel")?;
        }
        Ok(changed == 1)
    })
}

/// Shared terminal transition for the worker's owned running job.
fn finish_owned(
    conn: &Connection,
    job_id: i64,
    worker: i64,
    status: &str,
    error_kind: Option<&str>,
    now: i64,
    op: &str,
) -> Result<bool, rusqlite::Error> {
    // IMMEDIATE: the failed/other branch reads `progress_permille` before the
    // terminal UPDATE, so it must take the write lock upfront (see
    // `events::in_txn_immediate`).
    events::in_txn_immediate(conn, |conn| {
        let progress = if status == "done" {
            i64::from(crate::stem_separation::PROGRESS_COMPLETE)
        } else {
            // Leave a failed/other job's last progress untouched by writing it
            // back to itself.
            conn.query_row(
                "SELECT progress_permille FROM ai_jobs WHERE id = ?1",
                [job_id],
                |row| row.get(0),
            )?
        };
        let changed = conn.execute(
            "UPDATE ai_jobs SET status = ?1, error_kind = ?2, progress_permille = ?3, \
                 finished_at = ?4 \
             WHERE id = ?5 AND claimed_by = ?6 AND status = 'running'",
            params![status, error_kind, progress, now, job_id, worker],
        )?;
        if changed == 1 {
            events::record(conn, JOB_ENTITY, &job_id.to_string(), op)?;
        }
        Ok(changed == 1)
    })
}

/// Requests cancellation. A `queued` job is cancelled outright (nothing is
/// running); a `running` job is flagged for its worker to ack between chunks
/// (plan 2.4/2). Terminal/absent jobs are `NotCancellable`.
pub fn request_cancel(
    conn: &Connection,
    job_id: i64,
    now: i64,
) -> Result<CancelOutcome, rusqlite::Error> {
    events::in_txn(conn, |conn| {
        let queued_cancelled = conn.execute(
            "UPDATE ai_jobs SET status = 'cancelled', cancel_requested = 1, finished_at = ?1 \
             WHERE id = ?2 AND status = 'queued'",
            params![now, job_id],
        )?;
        if queued_cancelled == 1 {
            events::record(conn, JOB_ENTITY, &job_id.to_string(), "cancel")?;
            return Ok(CancelOutcome::CancelledImmediately);
        }
        let flagged = conn.execute(
            "UPDATE ai_jobs SET cancel_requested = 1 WHERE id = ?1 AND status = 'running'",
            [job_id],
        )?;
        if flagged == 1 {
            Ok(CancelOutcome::CancelRequested)
        } else {
            Ok(CancelOutcome::NotCancellable)
        }
    })
}

/// Attaches the promoted library track to a `done` job (staged -> saved),
/// recording the `save` lifecycle event. Called inside the promotion
/// transaction. Returns whether a `done` job was updated.
pub(crate) fn attach_result_track(
    conn: &Connection,
    job_id: i64,
    result_track_id: i64,
) -> Result<bool, rusqlite::Error> {
    let changed = conn.execute(
        "UPDATE ai_jobs SET result_track_id = ?1 WHERE id = ?2 AND status = 'done'",
        params![result_track_id, job_id],
    )?;
    if changed == 1 {
        events::record(conn, JOB_ENTITY, &job_id.to_string(), "save")?;
    }
    Ok(changed == 1)
}

/// Whether `job_id` was enqueued with the auto-promote save-intent (decision
/// 15). Missing jobs read as `false`. Consulted by the completion path
/// ([`crate::ai_promotion::complete_render`]) so a worker promotes a fresh
/// render without the enqueuer still being around.
pub(crate) fn job_auto_promote(conn: &Connection, job_id: i64) -> Result<bool, rusqlite::Error> {
    let flag: Option<i64> = conn
        .query_row(
            "SELECT auto_promote FROM ai_jobs WHERE id = ?1",
            [job_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(flag.unwrap_or(0) != 0)
}

/// Records a diagnostic on a still-staged `done` job whose auto-promotion
/// failed, without changing its state: the job stays `done` + unsaved (its
/// render is still in staging), so the promotion is retryable. Guarded on
/// `status = 'done' AND result_track_id IS NULL` so it can never annotate a
/// job that actually saved. Not a lifecycle transition, so it logs no event.
pub(crate) fn note_promotion_error(
    conn: &Connection,
    job_id: i64,
    error_kind: &str,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE ai_jobs SET error_kind = ?1 \
         WHERE id = ?2 AND status = 'done' AND result_track_id IS NULL",
        params![error_kind, job_id],
    )?;
    Ok(())
}

/// Discards a finished-but-unsaved render: deletes the staging file and moves
/// the job `done` -> `cancelled` so it stops blocking dedup and leaves the
/// conversion view (Beschluss 15). Returns whether a staged job was discarded.
pub fn discard_staged(
    conn: &Connection,
    staging: &StagingStore,
    job_id: i64,
    now: i64,
) -> Result<bool, rusqlite::Error> {
    let discarded = events::in_txn(conn, |conn| {
        let changed = conn.execute(
            "UPDATE ai_jobs SET status = 'cancelled', finished_at = ?1 \
             WHERE id = ?2 AND status = 'done' AND result_track_id IS NULL",
            params![now, job_id],
        )?;
        if changed == 1 {
            events::record(conn, JOB_ENTITY, &job_id.to_string(), "cancel")?;
        }
        Ok(changed == 1)
    })?;
    if discarded {
        // Best-effort: the DB decision already committed; a leftover file would
        // only cost disk, and the next discard/list tolerates its absence.
        let _ = staging.discard(job_id);
    }
    Ok(discarded)
}

/// Reads one job in surface shape, or `None` if it does not exist.
pub fn get_job(conn: &Connection, job_id: i64) -> Result<Option<AiJob>, rusqlite::Error> {
    conn.query_row(
        &format!("{JOB_SELECT} WHERE id = ?1"),
        [job_id],
        map_job_row,
    )
    .optional()
}

/// Lists every job in a batch, in id order.
pub fn list_jobs_in_batch(
    conn: &Connection,
    batch_id: &str,
) -> Result<Vec<AiJob>, rusqlite::Error> {
    let mut statement = conn.prepare(&format!("{JOB_SELECT} WHERE batch_id = ?1 ORDER BY id"))?;
    let jobs = statement
        .query_map([batch_id], map_job_row)?
        .collect::<Result<_, _>>()?;
    Ok(jobs)
}

/// Lists every non-cancelled job in id order — the conversion view's rows
/// (queued/processing/done-unsaved/saved/failed; Beschluss 15/18).
pub fn list_active_jobs(conn: &Connection) -> Result<Vec<AiJob>, rusqlite::Error> {
    let mut statement = conn.prepare(&format!(
        "{JOB_SELECT} WHERE status != 'cancelled' ORDER BY id"
    ))?;
    let jobs = statement
        .query_map([], map_job_row)?
        .collect::<Result<_, _>>()?;
    Ok(jobs)
}

/// The number of jobs whose render has been promoted into the library (a
/// `result_track_id` is attached). The app-hosted worker auto-promotes on its
/// own thread, whose writes carry the app's writer token and are therefore
/// filtered out of the external-changes runtime; the conversion view watches
/// this count instead, reloading the library the moment it grows so a
/// worker-promoted instrumental appears without a manual refresh.
pub fn count_saved(conn: &Connection) -> Result<i64, rusqlite::Error> {
    conn.query_row(
        "SELECT COUNT(*) FROM ai_jobs WHERE result_track_id IS NOT NULL",
        [],
        |row| row.get(0),
    )
}

/// Aggregate progress for a batch's single bar (plan 2.4/7).
pub fn batch_progress(conn: &Connection, batch_id: &str) -> Result<BatchProgress, rusqlite::Error> {
    conn.query_row(
        "SELECT COUNT(*), \
                COALESCE(SUM(status = 'done'), 0), \
                COALESCE(SUM(status = 'failed'), 0), \
                COALESCE(SUM(status = 'cancelled'), 0), \
                COALESCE(SUM(status = 'running'), 0), \
                COALESCE(SUM(status = 'queued'), 0), \
                COALESCE(AVG(progress_permille), 0) \
         FROM ai_jobs WHERE batch_id = ?1",
        [batch_id],
        |row| {
            let permille: f64 = row.get(6)?;
            Ok(BatchProgress {
                total: row.get(0)?,
                done: row.get(1)?,
                failed: row.get(2)?,
                cancelled: row.get(3)?,
                running: row.get(4)?,
                queued: row.get(5)?,
                permille: permille.round() as u16,
            })
        },
    )
}

/// A random 64-bit hex token grouping a multi-select batch — collision-free
/// enough for a per-invocation grouping key (fastrand is already a core dep,
/// same source as the change-log writer token).
fn new_batch_id() -> String {
    format!("{:016x}", fastrand::u64(..))
}

const JOB_SELECT: &str = "SELECT id, kind, batch_id, source_track_id, params_fingerprint, \
     status, progress_permille, cancel_requested, error_kind, result_track_id, \
     created_at, finished_at FROM ai_jobs";

fn map_job_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AiJob> {
    let status: String = row.get(5)?;
    let state = JobState::parse(&status).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            5,
            rusqlite::types::Type::Text,
            format!("unknown ai_jobs.status {status:?}").into(),
        )
    })?;
    Ok(AiJob {
        id: row.get(0)?,
        kind: row.get(1)?,
        batch_id: row.get(2)?,
        source_track_id: row.get(3)?,
        params_fingerprint: row.get(4)?,
        state,
        progress_permille: row.get(6)?,
        cancel_requested: row.get::<_, i64>(7)? != 0,
        error_kind: row.get(8)?,
        result_track_id: row.get(9)?,
        created_at: row.get(10)?,
        finished_at: row.get(11)?,
    })
}

#[cfg(test)]
#[path = "ai_jobs_tests.rs"]
mod tests;
