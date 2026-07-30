use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, Transaction};

use super::{
    DoctorApplyChange, DoctorApplyPlan, DoctorCleanup, DoctorError, DoctorField, DoctorReviewRowId,
    DoctorTrackRef, DoctorValue, DoctorWriteControl, DoctorWriteProgress, DoctorWriteReport,
    DoctorWriteRow, DoctorWriteRowState, LibraryDoctor,
};
use crate::library::tag_mutation::{
    classify_write_error, commit_guarded_tag_changes, read_tag_field_values, GuardedTagChange,
    GuardedTagField, TagMutationFailure, WriteErrorKind,
};
use crate::library::tag_write_job::TagWriteRecovery;

#[derive(Debug, Clone)]
struct InputChange {
    row_id: Option<DoctorReviewRowId>,
    track: DoctorTrackRef,
    field: DoctorField,
    expected: DoctorValue,
    proposed: DoctorValue,
}

#[derive(Debug)]
struct PreparedRow {
    input: InputChange,
    before: Option<String>,
    expected: Option<String>,
    after: Option<String>,
    outcome: &'static str,
}

#[derive(Debug)]
struct PreparedFile {
    position: usize,
    track: DoctorTrackRef,
    rows: Vec<PreparedRow>,
    state: &'static str,
    error_kind: Option<&'static str>,
    error_message: Option<String>,
}

#[derive(Debug)]
struct ExecutableFile {
    id: i64,
    track_id: i64,
    path: PathBuf,
    changes: Vec<GuardedTagChange>,
}

pub(super) fn now() -> i64 {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    i64::try_from(seconds).unwrap_or(i64::MAX)
}

fn encoded(field: DoctorField, value: &DoctorValue) -> Result<Option<String>, DoctorError> {
    match (field, value) {
        (DoctorField::Year, DoctorValue::Empty) => Ok(None),
        (DoctorField::Year, DoctorValue::Year(year)) if *year <= u32::from(u16::MAX) => {
            Ok(Some(year.to_string()))
        }
        (DoctorField::Year, _) => Err(DoctorError::InvalidStoredData(
            "Library Doctor year has an invalid value".into(),
        )),
        (_, DoctorValue::Empty) => Ok(Some(String::new())),
        (_, DoctorValue::Text(value)) => Ok(Some(value.clone())),
        (_, DoctorValue::Year(_)) => Err(DoctorError::InvalidStoredData(
            "Library Doctor text field has a year value".into(),
        )),
    }
}

const fn guarded_field(field: DoctorField) -> GuardedTagField {
    match field {
        DoctorField::Title => GuardedTagField::Title,
        DoctorField::Artist => GuardedTagField::Artist,
        DoctorField::Album => GuardedTagField::Album,
        DoctorField::AlbumArtist => GuardedTagField::AlbumArtist,
        DoctorField::Year => GuardedTagField::Year,
        DoctorField::Genre => GuardedTagField::Genre,
        DoctorField::RecordingMbid => GuardedTagField::RecordingMbid,
    }
}

fn decoded(field: DoctorField, value: Option<String>) -> DoctorValue {
    match (field, value) {
        (DoctorField::Year, None) => DoctorValue::Empty,
        (DoctorField::Year, Some(value)) => value
            .parse()
            .map_or_else(|_| DoctorValue::Text(value), DoctorValue::Year),
        (_, None) => DoctorValue::Empty,
        (_, Some(value)) if value.is_empty() => DoctorValue::Empty,
        (_, Some(value)) => DoctorValue::Text(value),
    }
}

fn file_inputs(changes: &[InputChange]) -> Result<Vec<Vec<InputChange>>, DoctorError> {
    let mut positions = HashMap::<i64, usize>::new();
    let mut files = Vec::<Vec<InputChange>>::new();
    let mut seen = HashSet::new();
    for change in changes {
        if !seen.insert((change.track.track_id, change.field)) {
            return Err(DoctorError::InvalidStoredData(
                "review plan contains the same track field twice".into(),
            ));
        }
        let position = *positions.entry(change.track.track_id).or_insert_with(|| {
            files.push(Vec::new());
            files.len() - 1
        });
        if files[position]
            .first()
            .is_some_and(|existing| existing.track.path != change.track.path)
        {
            return Err(DoctorError::InvalidStoredData(
                "review plan contains conflicting paths for one track".into(),
            ));
        }
        files[position].push(change.clone());
    }
    Ok(files)
}

fn prepare_files(
    conn: &Connection,
    changes: &[InputChange],
) -> Result<Vec<PreparedFile>, DoctorError> {
    let grouped = file_inputs(changes)?;
    let mut prepared = Vec::with_capacity(grouped.len());
    for (position, inputs) in grouped.into_iter().enumerate() {
        let track = inputs[0].track.clone();
        let registered = conn
            .query_row(
                "SELECT path FROM tracks WHERE id=?1 AND removed_at IS NULL",
                [track.track_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let identity_valid = registered.as_deref() == Some(track.path.to_string_lossy().as_ref());
        let field_names = inputs
            .iter()
            .map(|change| guarded_field(change.field))
            .collect::<Vec<_>>();
        let read = identity_valid
            .then(|| read_tag_field_values(&track.path, &field_names))
            .transpose();
        let (values, read_failure) = match read {
            Ok(Some(values)) => (values.into_iter().collect::<HashMap<_, _>>(), None),
            Err(error) => {
                let kind = classify_write_error(&error);
                (HashMap::new(), Some((kind, error.to_string())))
            }
            Ok(None) => (
                HashMap::new(),
                Some((
                    WriteErrorKind::NotFound,
                    "track path is no longer current".into(),
                )),
            ),
        };
        let mut rows = Vec::with_capacity(inputs.len());
        for input in inputs {
            let expected = encoded(input.field, &input.expected)?;
            let after = encoded(input.field, &input.proposed)?;
            let before = values
                .get(&guarded_field(input.field))
                .cloned()
                .unwrap_or_else(|| expected.clone());
            let outcome = if let Some((kind, _)) = &read_failure {
                if *kind == WriteErrorKind::NotFound {
                    "unavailable"
                } else {
                    "failed"
                }
            } else if before == expected {
                "pending"
            } else {
                "conflict"
            };
            rows.push(PreparedRow {
                input,
                before,
                expected,
                after,
                outcome,
            });
        }
        let has_pending = rows.iter().any(|row| row.outcome == "pending");
        let (state, error_kind, error_message) = match (has_pending, read_failure) {
            (true, _) => ("pending", None, None),
            (false, Some((kind, message))) => (
                if kind == WriteErrorKind::NotFound {
                    "unavailable"
                } else {
                    "failed"
                },
                Some(error_kind(kind)),
                Some(message),
            ),
            (false, None) => (
                "failed",
                Some("io"),
                Some("all selected fields conflict".into()),
            ),
        };
        prepared.push(PreparedFile {
            position,
            track,
            rows,
            state,
            error_kind,
            error_message,
        });
    }
    Ok(prepared)
}

fn insert_file(
    transaction: &Transaction<'_>,
    job_id: i64,
    file: PreparedFile,
) -> Result<Option<ExecutableFile>, DoctorError> {
    transaction.execute(
        "INSERT INTO tag_write_job_files \
         (job_id, position, track_id, path, state, error_kind, error_message, file_written) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
        params![
            job_id,
            i64::try_from(file.position).unwrap_or(i64::MAX),
            file.track.track_id,
            file.track.path.to_string_lossy(),
            file.state,
            file.error_kind,
            file.error_message,
        ],
    )?;
    let file_id = transaction.last_insert_rowid();
    let mut changes = Vec::new();
    for (position, row) in file.rows.into_iter().enumerate() {
        let expected_is_null = i64::from(row.expected.is_none());
        let before_is_null = i64::from(row.before.is_none());
        let after_is_null = i64::from(row.after.is_none());
        let row_id = row
            .input
            .row_id
            .map(|id| i64::try_from(id.raw()).unwrap_or(i64::MAX));
        transaction.execute(
            "INSERT INTO tag_write_journal \
             (file_id, position, review_row_id, field, guard_is_set, expected_value, \
              expected_is_null, before_value, before_is_null, after_value, after_is_null, outcome) \
             VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                file_id,
                i64::try_from(position).unwrap_or(i64::MAX),
                row_id,
                row.input.field.as_str(),
                row.expected,
                expected_is_null,
                row.before,
                before_is_null,
                row.after,
                after_is_null,
                row.outcome,
            ],
        )?;
        if row.outcome == "pending" {
            changes.push(GuardedTagChange {
                field: guarded_field(row.input.field),
                expected: row.expected,
                after: row.after,
            });
        }
    }
    Ok((file.state == "pending").then_some(ExecutableFile {
        id: file_id,
        track_id: file.track.track_id,
        path: file.track.path,
        changes,
    }))
}

fn prepare_job(
    conn: &Connection,
    kind: &'static str,
    source_job_id: Option<i64>,
    scan_id: Option<i64>,
    changes: &[InputChange],
) -> Result<(i64, Vec<ExecutableFile>), DoctorError> {
    let files = prepare_files(conn, changes)?;
    let transaction = conn.unchecked_transaction()?;
    transaction.execute(
        "INSERT INTO tag_write_jobs \
         (kind, source_job_id, scan_id, state, created_at, finished_at, total_tracks) \
         VALUES (?1, ?2, ?3, 'prepared', ?4, NULL, ?5)",
        params![
            kind,
            source_job_id,
            scan_id,
            now(),
            i64::try_from(files.len()).unwrap_or(i64::MAX)
        ],
    )?;
    let job_id = transaction.last_insert_rowid();
    let mut executable = Vec::new();
    for file in files {
        if let Some(file) = insert_file(&transaction, job_id, file)? {
            executable.push(file);
        }
    }
    transaction.commit()?;
    Ok((job_id, executable))
}

pub(super) fn error_kind(kind: WriteErrorKind) -> &'static str {
    match kind {
        WriteErrorKind::PermissionDenied => "permission_denied",
        WriteErrorKind::NotFound => "not_found",
        WriteErrorKind::UnsupportedFormat => "unsupported_format",
        WriteErrorKind::UnreadableTags => "unreadable_tags",
        WriteErrorKind::Io => "io",
    }
}

fn parse_error_kind(kind: Option<&str>) -> Option<WriteErrorKind> {
    match kind {
        Some("permission_denied") => Some(WriteErrorKind::PermissionDenied),
        Some("not_found") => Some(WriteErrorKind::NotFound),
        Some("unsupported_format") => Some(WriteErrorKind::UnsupportedFormat),
        Some("unreadable_tags") => Some(WriteErrorKind::UnreadableTags),
        Some("io") => Some(WriteErrorKind::Io),
        _ => None,
    }
}

fn claim_file(conn: &Connection, job_id: i64, file: &ExecutableFile) -> Result<(), DoctorError> {
    let transaction = conn.unchecked_transaction()?;
    transaction.execute(
        "UPDATE tag_write_jobs SET state='running' WHERE id=?1 AND state='prepared'",
        [job_id],
    )?;
    let changed = transaction.execute(
        "UPDATE tag_write_job_files SET state='running' WHERE id=?1 AND state='pending'",
        [file.id],
    )?;
    if changed != 1 {
        return Err(DoctorError::InvalidStoredData(
            "tag-write file is not pending".into(),
        ));
    }
    transaction.execute(
        "UPDATE tag_write_journal SET outcome='prepared' \
         WHERE file_id=?1 AND outcome='pending'",
        [file.id],
    )?;
    transaction.commit()?;
    Ok(())
}

fn terminal_failure(
    conn: &Connection,
    file: &ExecutableFile,
    failure: TagMutationFailure,
    source_job_id: Option<i64>,
) -> Result<(), DoctorError> {
    let (kind, message, file_written) = failure.into_parts();
    let unavailable = !file_written && kind == WriteErrorKind::NotFound;
    let transaction = conn.unchecked_transaction()?;
    transaction.execute(
        "UPDATE tag_write_job_files SET state=?1, error_kind=?2, error_message=?3, \
         file_written=?4 WHERE id=?5 AND state='running'",
        params![
            if unavailable { "unavailable" } else { "failed" },
            error_kind(kind),
            message,
            i64::from(file_written),
            file.id
        ],
    )?;
    let fields_changed = transaction.execute(
        "UPDATE tag_write_journal SET outcome=?1 WHERE file_id=?2 AND outcome='prepared'",
        params![
            if unavailable {
                "unavailable"
            } else if file_written {
                "applied"
            } else {
                "failed"
            },
            file.id
        ],
    )?;
    if fields_changed != file.changes.len() {
        return Err(DoctorError::InvalidStoredData(
            "tag-write journal field count changed during execution".into(),
        ));
    }
    if file_written {
        if let Some(source) = source_job_id {
            for change in &file.changes {
                let changed = transaction.execute(
                    "UPDATE tag_write_journal SET outcome='reverted' \
                     WHERE file_id=(SELECT id FROM tag_write_job_files \
                       WHERE job_id=?1 AND track_id=?2) \
                       AND field=?3 AND outcome='applied'",
                    params![source, file.track_id, change.field.as_str()],
                )?;
                if changed != 1 {
                    return Err(DoctorError::InvalidStoredData(
                        "revert source journal no longer matches the written field".into(),
                    ));
                }
            }
        }
    }
    transaction.commit()?;
    Ok(())
}

fn terminal_success(
    conn: &Connection,
    file: &ExecutableFile,
    applied: &[GuardedTagField],
    conflicts: &[GuardedTagField],
    source_job_id: Option<i64>,
    post_write_failure: Option<TagMutationFailure>,
) -> Result<(), DoctorError> {
    let transaction = conn.unchecked_transaction()?;
    for field in applied {
        let changed = transaction.execute(
            "UPDATE tag_write_journal SET outcome='applied' \
             WHERE file_id=?1 AND field=?2 AND outcome='prepared'",
            params![file.id, field.as_str()],
        )?;
        if changed != 1 {
            return Err(DoctorError::InvalidStoredData(
                "applied field was not prepared in the tag-write journal".into(),
            ));
        }
    }
    for field in conflicts {
        let changed = transaction.execute(
            "UPDATE tag_write_journal SET outcome='conflict' \
             WHERE file_id=?1 AND field=?2 AND outcome='prepared'",
            params![file.id, field.as_str()],
        )?;
        if changed != 1 {
            return Err(DoctorError::InvalidStoredData(
                "conflicting field was not prepared in the tag-write journal".into(),
            ));
        }
    }
    let wrote = !applied.is_empty();
    let (state, kind, message) = match post_write_failure {
        Some(failure) => {
            let (kind, message, file_written) = failure.into_parts();
            if !file_written {
                return Err(DoctorError::InvalidStoredData(
                    "post-write failure was not marked as written".into(),
                ));
            }
            ("failed", Some(error_kind(kind)), Some(message))
        }
        None if wrote => ("complete", None, None),
        None => (
            "failed",
            Some("io"),
            Some("all selected fields conflict".into()),
        ),
    };
    transaction.execute(
        "UPDATE tag_write_job_files SET state=?1, error_kind=?2, error_message=?3, \
         file_written=?4 WHERE id=?5 AND state='running'",
        params![state, kind, message, i64::from(wrote), file.id],
    )?;
    if let Some(source) = source_job_id {
        for field in applied {
            let changed = transaction.execute(
                "UPDATE tag_write_journal SET outcome='reverted' \
                 WHERE file_id=(SELECT id FROM tag_write_job_files \
                   WHERE job_id=?1 AND track_id=?2) \
                   AND field=?3 AND outcome='applied'",
                params![source, file.track_id, field.as_str()],
            )?;
            if changed != 1 {
                return Err(DoctorError::InvalidStoredData(
                    "revert source journal no longer matches the written field".into(),
                ));
            }
        }
    }
    transaction.commit()?;
    Ok(())
}

fn cancel_remaining(conn: &Connection, job_id: i64) -> Result<(), DoctorError> {
    let transaction = conn.unchecked_transaction()?;
    transaction.execute(
        "UPDATE tag_write_journal SET outcome='not_applied' WHERE file_id IN \
         (SELECT id FROM tag_write_job_files WHERE job_id=?1 AND state='pending') \
         AND outcome='pending'",
        [job_id],
    )?;
    transaction.execute(
        "UPDATE tag_write_job_files SET state='cancelled' WHERE job_id=?1 AND state='pending'",
        [job_id],
    )?;
    transaction.execute(
        "UPDATE tag_write_jobs SET state='cancelled', finished_at=?1 \
         WHERE id=?2 AND state IN ('prepared', 'running')",
        params![now(), job_id],
    )?;
    transaction.commit()?;
    Ok(())
}

fn complete_job(conn: &Connection, job_id: i64) -> Result<(), DoctorError> {
    conn.execute(
        "UPDATE tag_write_jobs SET state='completed', finished_at=?1 \
         WHERE id=?2 AND state IN ('prepared', 'running')",
        params![now(), job_id],
    )?;
    Ok(())
}

fn run_job(
    conn: &Connection,
    job_id: i64,
    files: &[ExecutableFile],
    source_job_id: Option<i64>,
    mut progress: impl FnMut(DoctorWriteProgress) -> DoctorWriteControl,
) -> Result<DoctorWriteReport, DoctorError> {
    let total = conn.query_row(
        "SELECT total_tracks FROM tag_write_jobs WHERE id=?1",
        [job_id],
        |row| row.get::<_, i64>(0),
    )?;
    let total = usize::try_from(total).unwrap_or_default();
    let mut completed = total.saturating_sub(files.len());
    for file in files {
        if progress(DoctorWriteProgress {
            completed_tracks: completed,
            total_tracks: total,
        }) == DoctorWriteControl::Cancel
        {
            cancel_remaining(conn, job_id)?;
            return report(conn, job_id);
        }
        claim_file(conn, job_id, file)?;
        match commit_guarded_tag_changes(conn, file.track_id, &file.path, &file.changes, true) {
            Ok(outcome) => terminal_success(
                conn,
                file,
                &outcome.applied,
                &outcome.conflicts,
                source_job_id,
                outcome.post_write_failure,
            )?,
            Err(failure) => terminal_failure(conn, file, failure, source_job_id)?,
        }
        completed += 1;
    }
    complete_job(conn, job_id)?;
    progress(DoctorWriteProgress {
        completed_tracks: completed,
        total_tracks: total,
    });
    report(conn, job_id)
}

pub(super) fn report(conn: &Connection, job_id: i64) -> Result<DoctorWriteReport, DoctorError> {
    let (kind, source_job_id) = conn.query_row(
        "SELECT kind, source_job_id FROM tag_write_jobs WHERE id=?1",
        [job_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?)),
    )?;
    let mut statement = conn.prepare(
        "SELECT v.review_row_id, f.track_id, f.path, v.field, v.expected_value, \
         v.after_value, v.outcome, f.file_written, f.state, f.error_kind, f.error_message \
         FROM tag_write_job_files f JOIN tag_write_journal v ON v.file_id=f.id \
         WHERE f.job_id=?1 ORDER BY f.position, v.position",
    )?;
    let rows = statement
        .query_map([job_id], |row| {
            let raw_field = row.get::<_, String>(3)?;
            let field = DoctorField::parse(&raw_field).ok_or(rusqlite::Error::InvalidQuery)?;
            let outcome = row.get::<_, String>(6)?;
            let file_state = row.get::<_, String>(8)?;
            let state = match outcome.as_str() {
                "applied" if kind == "doctor_revert" => DoctorWriteRowState::Reverted,
                "applied" => DoctorWriteRowState::Applied,
                "reverted" => DoctorWriteRowState::Reverted,
                "not_applied" => DoctorWriteRowState::Cancelled,
                "conflict" => DoctorWriteRowState::Conflict,
                "unavailable" => DoctorWriteRowState::Unavailable,
                "failed" => DoctorWriteRowState::Failed,
                _ if file_state == "cancelled" => DoctorWriteRowState::Cancelled,
                _ => DoctorWriteRowState::Failed,
            };
            let raw_id = row.get::<_, Option<i64>>(0)?;
            let stored_error_kind = row.get::<_, Option<String>>(9)?;
            Ok(DoctorWriteRow {
                row_id: raw_id
                    .and_then(|id| u64::try_from(id).ok())
                    .map(DoctorReviewRowId::from_raw),
                track_id: row.get(1)?,
                path: PathBuf::from(row.get::<_, String>(2)?),
                field,
                expected: decoded(field, row.get(4)?),
                proposed: decoded(field, row.get(5)?),
                state,
                file_written: row.get::<_, i64>(7)? != 0,
                error_kind: parse_error_kind(stored_error_kind.as_deref()),
                error: row.get(10)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let count = |state| {
        rows.iter()
            .filter(|row| row.state == state)
            .map(|row| row.track_id)
            .collect::<HashSet<_>>()
            .len()
    };
    let failed_file_tracks = conn.query_row(
        "SELECT COUNT(DISTINCT track_id) FROM tag_write_job_files \
         WHERE job_id=?1 AND state='failed' AND EXISTS \
         (SELECT 1 FROM tag_write_journal WHERE file_id=tag_write_job_files.id \
          AND outcome IN ('failed', 'applied'))",
        [job_id],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(DoctorWriteReport {
        job_id,
        source_job_id,
        updated_tracks: count(if kind == "doctor_revert" {
            DoctorWriteRowState::Reverted
        } else {
            DoctorWriteRowState::Applied
        }),
        cancelled_tracks: count(DoctorWriteRowState::Cancelled),
        failed_tracks: usize::try_from(failed_file_tracks).unwrap_or_default(),
        conflict_tracks: count(DoctorWriteRowState::Conflict),
        unavailable_tracks: count(DoctorWriteRowState::Unavailable),
        rows,
    })
}

fn apply_inputs(plan: &DoctorApplyPlan) -> Vec<InputChange> {
    plan.changes()
        .iter()
        .map(|change: &DoctorApplyChange| InputChange {
            row_id: Some(change.row_id),
            track: change.track.clone(),
            field: change.field,
            expected: change.expected.clone(),
            proposed: change.proposed.clone(),
        })
        .collect()
}

fn revert_inputs(conn: &Connection, source_job_id: i64) -> Result<Vec<InputChange>, DoctorError> {
    let mut statement = conn.prepare(
        "SELECT v.review_row_id, f.track_id, f.path, v.field, v.after_value, v.before_value \
         FROM tag_write_job_files f JOIN tag_write_journal v ON v.file_id=f.id \
         WHERE f.job_id=?1 AND v.outcome='applied' ORDER BY f.position, v.position",
    )?;
    let inputs = statement
        .query_map([source_job_id], |row| {
            let raw = row.get::<_, String>(3)?;
            let field = DoctorField::parse(&raw).ok_or(rusqlite::Error::InvalidQuery)?;
            Ok(InputChange {
                row_id: row
                    .get::<_, Option<i64>>(0)?
                    .and_then(|id| u64::try_from(id).ok())
                    .map(DoctorReviewRowId::from_raw),
                track: DoctorTrackRef {
                    track_id: row.get(1)?,
                    path: PathBuf::from(row.get::<_, String>(2)?),
                    file_mtime: 0,
                    file_size: 0,
                    device: None,
                    inode: None,
                },
                field,
                expected: decoded(field, row.get(4)?),
                proposed: decoded(field, row.get(5)?),
            })
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(DoctorError::from)?;
    Ok(inputs)
}

impl LibraryDoctor<'_> {
    pub fn apply_review_plan(
        &mut self,
        plan: &DoctorApplyPlan,
        progress: impl FnMut(DoctorWriteProgress) -> DoctorWriteControl,
    ) -> Result<DoctorWriteReport, DoctorError> {
        let inputs = apply_inputs(plan);
        let (job_id, files) = prepare_job(
            self.conn,
            "doctor_apply",
            None,
            Some(plan.scan_id()),
            &inputs,
        )?;
        run_job(self.conn, job_id, &files, None, progress)
    }

    pub fn last_cleanup(&self) -> Result<Option<DoctorCleanup>, DoctorError> {
        self.conn
            .query_row(
                "SELECT j.id, j.scan_id, j.created_at, COUNT(DISTINCT f.track_id) \
                 FROM tag_write_jobs j JOIN tag_write_job_files f ON f.job_id=j.id \
                 JOIN tag_write_journal v ON v.file_id=f.id \
                 WHERE j.kind='doctor_apply' AND j.state IN ('completed', 'cancelled', 'interrupted') \
                   AND v.outcome='applied' AND NOT EXISTS (SELECT 1 FROM tag_write_job_files uf \
                     JOIN tag_write_journal uv ON uv.file_id=uf.id WHERE uf.job_id=j.id \
                     AND uv.outcome IN ('pending', 'prepared')) \
                 GROUP BY j.id ORDER BY j.id DESC LIMIT 1",
                [],
                |row| {
                    Ok(DoctorCleanup {
                        job_id: row.get(0)?,
                        scan_id: row.get(1)?,
                        created_at: row.get(2)?,
                        track_count: usize::try_from(row.get::<_, i64>(3)?).unwrap_or_default(),
                    })
                },
            )
            .optional()
            .map_err(DoctorError::from)
    }

    pub fn revert_last_cleanup(
        &mut self,
        progress: impl FnMut(DoctorWriteProgress) -> DoctorWriteControl,
    ) -> Result<Option<DoctorWriteReport>, DoctorError> {
        let Some(cleanup) = self.last_cleanup()? else {
            return Ok(None);
        };
        let inputs = revert_inputs(self.conn, cleanup.job_id)?;
        let (job_id, files) = prepare_job(
            self.conn,
            "doctor_revert",
            Some(cleanup.job_id),
            Some(cleanup.scan_id),
            &inputs,
        )?;
        run_job(self.conn, job_id, &files, Some(cleanup.job_id), progress).map(Some)
    }

    pub fn recover_incomplete_writes(&self) -> Result<Vec<TagWriteRecovery>, DoctorError> {
        crate::library::tag_write_job::recover_incomplete_tag_write_jobs_in(self.conn)
            .map_err(DoctorError::from)
    }
}
