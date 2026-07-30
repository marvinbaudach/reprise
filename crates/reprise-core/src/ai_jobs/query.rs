use rusqlite::{Connection, OptionalExtension};

use super::{AiJob, BatchProgress, JobState};

const JOB_SELECT: &str = "SELECT id, kind, batch_id, source_track_id, params_fingerprint, \
     status, progress_permille, cancel_requested, error_kind, result_track_id, \
     created_at, finished_at FROM ai_jobs";

/// Reads one job in surface shape, or `None` if it does not exist.
pub fn get_job(db: &crate::db::Db, job_id: i64) -> Result<Option<AiJob>, rusqlite::Error> {
    let conn = db.conn();
    get_job_in(conn, job_id)
}

pub(crate) fn get_job_in(conn: &Connection, job_id: i64) -> Result<Option<AiJob>, rusqlite::Error> {
    conn.query_row(
        &format!("{JOB_SELECT} WHERE id = ?1"),
        [job_id],
        map_job_row,
    )
    .optional()
}

/// Lists every job in a batch, in id order.
pub fn list_jobs_in_batch(
    db: &crate::db::Db,
    batch_id: &str,
) -> Result<Vec<AiJob>, rusqlite::Error> {
    let conn = db.conn();
    let mut statement = conn.prepare(&format!("{JOB_SELECT} WHERE batch_id = ?1 ORDER BY id"))?;
    let jobs = statement
        .query_map([batch_id], map_job_row)?
        .collect::<Result<_, _>>()?;
    Ok(jobs)
}

/// Lists every non-cancelled job in id order — the conversion view's rows
/// (queued/processing/done-unsaved/saved/failed; Beschluss 15/18).
pub fn list_active_jobs(db: &crate::db::Db) -> Result<Vec<AiJob>, rusqlite::Error> {
    let conn = db.conn();
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
pub fn count_saved(db: &crate::db::Db) -> Result<i64, rusqlite::Error> {
    let conn = db.conn();
    conn.query_row(
        "SELECT COUNT(*) FROM ai_jobs WHERE result_track_id IS NOT NULL",
        [],
        |row| row.get(0),
    )
}

/// Aggregate progress for a batch's single bar (plan 2.4/7).
pub fn batch_progress(
    db: &crate::db::Db,
    batch_id: &str,
) -> Result<BatchProgress, rusqlite::Error> {
    let conn = db.conn();
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
