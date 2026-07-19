use std::collections::HashMap;
use std::path::PathBuf;

use rusqlite::{Connection, OptionalExtension};

use super::super::tag_mutation::{
    classify_write_error, read_tag_field_values, GuardedTagField, WriteErrorKind,
};
use super::types::{RecoveryState, TagWriteJobKind, TagWriteRecovery};

#[derive(Debug, Clone)]
pub(crate) struct TagWriteFieldRecovery {
    pub(crate) job_id: i64,
    pub(crate) job_kind: TagWriteJobKind,
    pub(crate) job_state: String,
    pub(crate) source_job_id: Option<i64>,
    pub(crate) file_id: i64,
    pub(crate) file_state: String,
    pub(crate) track_id: i64,
    pub(crate) path: PathBuf,
    pub(crate) field: GuardedTagField,
    pub(crate) state: RecoveryState,
    pub(crate) error_kind: Option<WriteErrorKind>,
    pub(crate) error: Option<String>,
}

#[derive(Debug)]
struct UncertainField {
    field: GuardedTagField,
    before: Option<String>,
    after: Option<String>,
    outcome: String,
}

#[derive(Debug)]
struct UncertainFile {
    job_id: i64,
    job_kind: TagWriteJobKind,
    job_state: String,
    source_job_id: Option<i64>,
    file_id: i64,
    file_state: String,
    track_id: i64,
    path: PathBuf,
}

fn failed_fields(
    file: &UncertainFile,
    fields: &[UncertainField],
    state: RecoveryState,
    kind: WriteErrorKind,
    error: &str,
) -> Vec<TagWriteFieldRecovery> {
    fields
        .iter()
        .map(|field| TagWriteFieldRecovery {
            job_id: file.job_id,
            job_kind: file.job_kind,
            job_state: file.job_state.clone(),
            source_job_id: file.source_job_id,
            file_id: file.file_id,
            file_state: file.file_state.clone(),
            track_id: file.track_id,
            path: file.path.clone(),
            field: field.field,
            state,
            error_kind: Some(kind),
            error: Some(error.to_owned()),
        })
        .collect()
}

fn classify_file(
    conn: &Connection,
    file: &UncertainFile,
    fields: &[UncertainField],
) -> Result<Vec<TagWriteFieldRecovery>, rusqlite::Error> {
    if file.file_state == "pending" && fields.iter().all(|field| field.outcome == "pending") {
        return Ok(fields
            .iter()
            .map(|field| TagWriteFieldRecovery {
                job_id: file.job_id,
                job_kind: file.job_kind,
                job_state: file.job_state.clone(),
                source_job_id: file.source_job_id,
                file_id: file.file_id,
                file_state: file.file_state.clone(),
                track_id: file.track_id,
                path: file.path.clone(),
                field: field.field,
                state: RecoveryState::NotApplied,
                error_kind: None,
                error: None,
            })
            .collect());
    }
    let registered = conn
        .query_row(
            "SELECT path FROM tracks WHERE id=?1 AND removed_at IS NULL",
            [file.track_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if registered.as_deref() != Some(file.path.to_string_lossy().as_ref()) {
        return Ok(failed_fields(
            file,
            fields,
            RecoveryState::Unavailable,
            WriteErrorKind::NotFound,
            "track path is no longer current",
        ));
    }
    let names = fields.iter().map(|field| field.field).collect::<Vec<_>>();
    let values = match read_tag_field_values(&file.path, &names) {
        Ok(values) => values.into_iter().collect::<HashMap<_, _>>(),
        Err(error) => {
            let kind = classify_write_error(&error);
            let state = if kind == WriteErrorKind::NotFound {
                RecoveryState::Unavailable
            } else {
                RecoveryState::Failed
            };
            return Ok(failed_fields(file, fields, state, kind, &error.to_string()));
        }
    };
    Ok(fields
        .iter()
        .map(|field| {
            let current = values.get(&field.field).cloned().unwrap_or(None);
            let state = if current == field.after {
                RecoveryState::Applied
            } else if current == field.before {
                RecoveryState::NotApplied
            } else {
                RecoveryState::Conflict
            };
            TagWriteFieldRecovery {
                job_id: file.job_id,
                job_kind: file.job_kind,
                job_state: file.job_state.clone(),
                source_job_id: file.source_job_id,
                file_id: file.file_id,
                file_state: file.file_state.clone(),
                track_id: file.track_id,
                path: file.path.clone(),
                field: field.field,
                state,
                error_kind: None,
                error: None,
            }
        })
        .collect())
}

pub(crate) fn recover_incomplete_tag_write_fields(
    conn: &Connection,
) -> Result<Vec<TagWriteFieldRecovery>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT j.id, j.kind, j.state, j.source_job_id, f.id, f.state, f.track_id, f.path \
         FROM tag_write_jobs j JOIN tag_write_job_files f ON f.job_id=j.id \
         WHERE j.state IN ('prepared', 'running', 'interrupted') AND EXISTS ( \
           SELECT 1 FROM tag_write_journal v WHERE v.file_id=f.id \
           AND v.outcome IN ('pending', 'prepared')) ORDER BY j.id, f.position",
    )?;
    let files = statement
        .query_map([], |row| {
            let kind = row.get::<_, String>(1)?;
            Ok(UncertainFile {
                job_id: row.get(0)?,
                job_kind: TagWriteJobKind::parse(&kind).ok_or(rusqlite::Error::InvalidQuery)?,
                job_state: row.get(2)?,
                source_job_id: row.get(3)?,
                file_id: row.get(4)?,
                file_state: row.get(5)?,
                track_id: row.get(6)?,
                path: PathBuf::from(row.get::<_, String>(7)?),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut recoveries = Vec::new();
    for file in files {
        let mut fields_statement = conn.prepare(
            "SELECT field, before_value, after_value, outcome FROM tag_write_journal \
             WHERE file_id=?1 AND outcome IN ('pending', 'prepared') ORDER BY position",
        )?;
        let fields = fields_statement
            .query_map([file.file_id], |row| {
                let raw = row.get::<_, String>(0)?;
                Ok(UncertainField {
                    field: GuardedTagField::parse(&raw).ok_or(rusqlite::Error::InvalidQuery)?,
                    before: row.get(1)?,
                    after: row.get(2)?,
                    outcome: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        recoveries.extend(classify_file(conn, &file, &fields)?);
    }
    Ok(recoveries)
}

pub fn recover_incomplete_tag_write_jobs(
    conn: &Connection,
) -> Result<Vec<TagWriteRecovery>, rusqlite::Error> {
    let fields = recover_incomplete_tag_write_fields(conn)?;
    let mut grouped = Vec::<TagWriteRecovery>::new();
    for field in fields {
        if let Some(file) = grouped
            .iter_mut()
            .find(|file| file.file_id == field.file_id)
        {
            if file.state != field.state {
                file.state = RecoveryState::Conflict;
                file.error_kind = None;
                file.error = None;
            }
            continue;
        }
        grouped.push(TagWriteRecovery {
            job_id: field.job_id,
            file_id: field.file_id,
            track_id: field.track_id,
            path: field.path,
            state: field.state,
            error_kind: field.error_kind,
            error: field.error,
        });
    }
    Ok(grouped)
}
