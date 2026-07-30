use std::collections::BTreeMap;

use rusqlite::{params, Connection};

use super::write::{error_kind, now, report};
use super::{DoctorError, DoctorWriteReport, LibraryDoctor};
use crate::library::tag_mutation::WriteErrorKind;
use crate::library::tag_write_job::{
    recover_incomplete_tag_write_fields, RecoveryState, TagWriteFieldRecovery, TagWriteJobKind,
};

fn outcome(state: RecoveryState) -> &'static str {
    match state {
        RecoveryState::Applied => "applied",
        RecoveryState::NotApplied => "not_applied",
        RecoveryState::Conflict => "conflict",
        RecoveryState::Unavailable => "unavailable",
        RecoveryState::Failed => "failed",
    }
}

fn file_terminal(
    fields: &[&TagWriteFieldRecovery],
) -> (&'static str, bool, Option<WriteErrorKind>, Option<String>) {
    if fields
        .iter()
        .any(|field| field.state == RecoveryState::Applied)
    {
        if let Some(failed) = fields
            .iter()
            .find(|field| field.state == RecoveryState::Failed)
        {
            return ("failed", true, failed.error_kind, failed.error.clone());
        }
        return ("complete", true, None, None);
    }
    if let Some(failed) = fields
        .iter()
        .find(|field| field.state == RecoveryState::Failed)
    {
        return ("failed", false, failed.error_kind, failed.error.clone());
    }
    if let Some(unavailable) = fields
        .iter()
        .find(|field| field.state == RecoveryState::Unavailable)
    {
        return (
            "unavailable",
            false,
            unavailable.error_kind.or(Some(WriteErrorKind::NotFound)),
            unavailable
                .error
                .clone()
                .or_else(|| Some("track is unavailable".into())),
        );
    }
    if fields
        .iter()
        .all(|field| field.state == RecoveryState::NotApplied)
    {
        return ("cancelled", false, None, None);
    }
    (
        "failed",
        false,
        Some(WriteErrorKind::Io),
        Some("tag fields conflict after interrupted write".into()),
    )
}

fn finalize_job(
    conn: &Connection,
    job_id: i64,
    fields: &[TagWriteFieldRecovery],
) -> Result<(), DoctorError> {
    let source_job_id = fields.first().and_then(|field| field.source_job_id);
    let was_claimed = fields
        .iter()
        .any(|field| field.job_state != "prepared" || field.file_state == "running");
    let transaction = conn.unchecked_transaction()?;
    for field in fields {
        let changed = transaction.execute(
            "UPDATE tag_write_journal SET outcome=?1 WHERE file_id=?2 AND field=?3 \
             AND outcome IN ('pending', 'prepared')",
            params![outcome(field.state), field.file_id, field.field.as_str()],
        )?;
        if changed != 1 {
            return Err(DoctorError::InvalidStoredData(
                "uncertain Doctor journal field changed during recovery".into(),
            ));
        }
        if field.job_kind == TagWriteJobKind::DoctorRevert && field.state == RecoveryState::Applied
        {
            let source = source_job_id.ok_or_else(|| {
                DoctorError::InvalidStoredData("Doctor revert has no source job".into())
            })?;
            let source_changed = transaction.execute(
                "UPDATE tag_write_journal SET outcome='reverted' WHERE file_id=( \
                   SELECT id FROM tag_write_job_files WHERE job_id=?1 AND track_id=?2) \
                 AND field=?3 AND outcome='applied'",
                params![source, field.track_id, field.field.as_str()],
            )?;
            if source_changed != 1 {
                return Err(DoctorError::InvalidStoredData(
                    "recovered revert no longer matches its source journal".into(),
                ));
            }
        }
    }
    let mut by_file = BTreeMap::<i64, Vec<&TagWriteFieldRecovery>>::new();
    for field in fields {
        by_file.entry(field.file_id).or_default().push(field);
    }
    for (file_id, file_fields) in by_file {
        let (state, written, kind, message) = file_terminal(&file_fields);
        let changed = transaction.execute(
            "UPDATE tag_write_job_files SET state=?1, file_written=?2, error_kind=?3, \
             error_message=?4 WHERE id=?5 AND state IN ('pending', 'running')",
            params![
                state,
                i64::from(written),
                kind.map(error_kind),
                message,
                file_id
            ],
        )?;
        if changed != 1 {
            return Err(DoctorError::InvalidStoredData(
                "uncertain Doctor file changed during recovery".into(),
            ));
        }
    }
    let state = if was_claimed {
        "interrupted"
    } else {
        "cancelled"
    };
    let changed = transaction.execute(
        "UPDATE tag_write_jobs SET state=?1, finished_at=?2 WHERE id=?3 \
         AND state IN ('prepared', 'running', 'interrupted')",
        params![state, now(), job_id],
    )?;
    if changed != 1 {
        return Err(DoctorError::InvalidStoredData(
            "uncertain Doctor job changed during recovery".into(),
        ));
    }
    transaction.commit()?;
    Ok(())
}

impl LibraryDoctor<'_> {
    /// Finalizes only journal state after a crash. It performs no tag write,
    /// retry, reconciliation scan, or automatic rollback.
    pub fn finalize_incomplete_writes(&mut self) -> Result<Vec<DoctorWriteReport>, DoctorError> {
        let fields = recover_incomplete_tag_write_fields(self.conn)?;
        let mut jobs = BTreeMap::<i64, Vec<TagWriteFieldRecovery>>::new();
        for field in fields {
            if matches!(
                field.job_kind,
                TagWriteJobKind::DoctorApply | TagWriteJobKind::DoctorRevert
            ) {
                jobs.entry(field.job_id).or_default().push(field);
            }
        }
        let mut reports = Vec::with_capacity(jobs.len());
        for (job_id, fields) in jobs {
            finalize_job(self.conn, job_id, &fields)?;
            reports.push(report(self.conn, job_id)?);
        }
        Ok(reports)
    }
}
