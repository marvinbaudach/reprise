use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};

use super::super::tag_edit::{EditableTags, TagPatch};
use super::super::tag_mutation::{
    commit_tag_mutation, PreparedTagMutation, TagMutationFailure, WriteErrorKind,
};
use super::types::{JournaledTagMutation, PreparedTagWriteJob, TagWriteJobSpec};

#[derive(Debug)]
struct JournalEntry {
    field: &'static str,
    before: Option<String>,
    after: Option<String>,
}

fn text_entry(field: &'static str, before: &str, after: &Option<String>) -> Option<JournalEntry> {
    after.as_ref().map(|after| JournalEntry {
        field,
        before: Some(before.to_string()),
        after: Some(after.clone()),
    })
}

fn number_entry(
    field: &'static str,
    before: Option<u32>,
    after: Option<Option<u32>>,
) -> Option<JournalEntry> {
    after.map(|after| JournalEntry {
        field,
        before: before.map(|value| value.to_string()),
        after: after.map(|value| value.to_string()),
    })
}

fn journal_entries(before: &EditableTags, patch: &TagPatch) -> Vec<JournalEntry> {
    [
        text_entry("title", &before.title, &patch.title),
        text_entry("artist", &before.artist, &patch.artist),
        text_entry("album", &before.album, &patch.album),
        text_entry("album_artist", &before.album_artist, &patch.album_artist),
        number_entry("year", before.year, patch.year),
        number_entry("track_no", before.track_no, patch.track_no),
        text_entry("genre", &before.genre, &patch.genre),
    ]
    .into_iter()
    .flatten()
    .collect()
}

pub(crate) fn prepare_tag_write_job(
    conn: &Connection,
    spec: TagWriteJobSpec,
    mutations: &[(usize, PreparedTagMutation)],
) -> Result<PreparedTagWriteJob, rusqlite::Error> {
    let transaction = conn.unchecked_transaction()?;
    let created_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    transaction.execute(
        "INSERT INTO tag_write_jobs \
         (kind, source_job_id, scan_id, state, created_at, finished_at, total_tracks) \
         VALUES (?1, ?2, ?3, 'prepared', ?4, NULL, ?5)",
        params![
            spec.kind.as_str(),
            spec.source_job_id,
            spec.scan_id,
            i64::try_from(created_at).unwrap_or(i64::MAX),
            i64::try_from(mutations.len()).unwrap_or(i64::MAX),
        ],
    )?;
    let job_id = transaction.last_insert_rowid();
    let mut files = Vec::with_capacity(mutations.len());
    for (position, mutation) in mutations {
        transaction.execute(
            "INSERT INTO tag_write_job_files \
             (job_id, position, track_id, path, state, file_written) \
             VALUES (?1, ?2, ?3, ?4, 'pending', 0)",
            params![
                job_id,
                i64::try_from(*position).unwrap_or(i64::MAX),
                mutation.id,
                mutation.path.to_string_lossy(),
            ],
        )?;
        let file_id = transaction.last_insert_rowid();
        let entries = journal_entries(&mutation.before, &mutation.patch);
        for (field_position, entry) in entries.iter().enumerate() {
            let before_is_null = i64::from(entry.before.is_none());
            let after_is_null = i64::from(entry.after.is_none());
            transaction.execute(
                "INSERT INTO tag_write_journal \
                 (file_id, position, review_row_id, field, guard_is_set, expected_value, \
                  expected_is_null, before_value, before_is_null, after_value, after_is_null, outcome) \
                 VALUES (?1, ?2, NULL, ?3, 0, NULL, 1, ?4, ?5, ?6, ?7, 'pending')",
                params![
                    file_id,
                    i64::try_from(field_position).unwrap_or(i64::MAX),
                    entry.field,
                    entry.before,
                    before_is_null,
                    entry.after,
                    after_is_null,
                ],
            )?;
        }
        files.push(JournaledTagMutation {
            file_id,
            position: *position,
            field_count: entries.len(),
            mutation: mutation.clone(),
        });
    }
    transaction.commit()?;
    Ok(PreparedTagWriteJob { id: job_id, files })
}

fn error_kind_name(kind: WriteErrorKind) -> &'static str {
    match kind {
        WriteErrorKind::PermissionDenied => "permission_denied",
        WriteErrorKind::NotFound => "not_found",
        WriteErrorKind::UnsupportedFormat => "unsupported_format",
        WriteErrorKind::UnreadableTags => "unreadable_tags",
        WriteErrorKind::Io => "io",
    }
}

fn sqlite_failure(error: &rusqlite::Error, file_written: bool) -> TagMutationFailure {
    TagMutationFailure {
        kind: WriteErrorKind::Io,
        error: format!("could not update tag-write journal: {error}"),
        file_written,
    }
}

fn affected_exactly(actual: usize, expected: usize) -> Result<(), rusqlite::Error> {
    if actual == expected {
        Ok(())
    } else {
        Err(rusqlite::Error::InvalidQuery)
    }
}

fn claim_tag_write_file(
    conn: &Connection,
    job_id: i64,
    file: &JournaledTagMutation,
) -> Result<(), TagMutationFailure> {
    if file.field_count == 0 {
        return Err(TagMutationFailure {
            kind: WriteErrorKind::Io,
            error: "tag-write file has no journal fields".into(),
            file_written: false,
        });
    }
    let transaction = conn
        .unchecked_transaction()
        .map_err(|error| sqlite_failure(&error, false))?;
    let job_changed = transaction
        .execute(
            "UPDATE tag_write_jobs SET state='running' \
             WHERE id=?1 AND state='prepared'",
            [job_id],
        )
        .map_err(|error| sqlite_failure(&error, false))?;
    if job_changed == 0 {
        let state = transaction
            .query_row(
                "SELECT state FROM tag_write_jobs WHERE id=?1",
                [job_id],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| sqlite_failure(&error, false))?;
        if state != "running" {
            return Err(TagMutationFailure {
                kind: WriteErrorKind::Io,
                error: format!("tag-write job is not executable in state {state}"),
                file_written: false,
            });
        }
    }
    let file_changed = transaction
        .execute(
            "UPDATE tag_write_job_files SET state='running' \
             WHERE id=?1 AND job_id=?2 AND state='pending'",
            params![file.file_id, job_id],
        )
        .map_err(|error| sqlite_failure(&error, false))?;
    affected_exactly(file_changed, 1).map_err(|error| sqlite_failure(&error, false))?;
    let fields_changed = transaction
        .execute(
            "UPDATE tag_write_journal SET outcome='prepared' \
             WHERE file_id=?1 AND outcome='pending'",
            [file.file_id],
        )
        .map_err(|error| sqlite_failure(&error, false))?;
    affected_exactly(fields_changed, file.field_count)
        .map_err(|error| sqlite_failure(&error, false))?;
    transaction
        .commit()
        .map_err(|error| sqlite_failure(&error, false))
}

fn mark_terminal(
    conn: &Connection,
    file: &JournaledTagMutation,
    failure: Option<&TagMutationFailure>,
) -> Result<(), TagMutationFailure> {
    let file_written = failure.is_none_or(|failure| failure.file_written);
    let file_state = if failure.is_some() {
        "failed"
    } else {
        "complete"
    };
    let field_outcome = if file_written { "applied" } else { "failed" };
    let kind = failure.map(|failure| error_kind_name(failure.kind));
    let message = failure.map(|failure| failure.error.as_str());
    let transaction = conn
        .unchecked_transaction()
        .map_err(|error| sqlite_failure(&error, file_written))?;
    let file_changed = transaction
        .execute(
            "UPDATE tag_write_job_files \
             SET state=?1, error_kind=?2, error_message=?3, file_written=?4 \
             WHERE id=?5 AND state='running'",
            params![
                file_state,
                kind,
                message,
                i64::from(file_written),
                file.file_id
            ],
        )
        .map_err(|error| sqlite_failure(&error, file_written))?;
    affected_exactly(file_changed, 1).map_err(|error| sqlite_failure(&error, file_written))?;
    let fields_changed = transaction
        .execute(
            "UPDATE tag_write_journal SET outcome=?1 \
             WHERE file_id=?2 AND outcome='prepared'",
            params![field_outcome, file.file_id],
        )
        .map_err(|error| sqlite_failure(&error, file_written))?;
    affected_exactly(fields_changed, file.field_count)
        .map_err(|error| sqlite_failure(&error, file_written))?;
    transaction
        .commit()
        .map_err(|error| sqlite_failure(&error, file_written))
}

pub(crate) fn execute_tag_write_file(
    conn: &Connection,
    job_id: i64,
    file: &JournaledTagMutation,
    ignore_watcher: bool,
    before_save: &mut dyn FnMut(&Connection, i64, i64),
) -> Result<(), TagMutationFailure> {
    claim_tag_write_file(conn, job_id, file)?;
    before_save(conn, job_id, file.file_id);

    match commit_tag_mutation(conn, &file.mutation, ignore_watcher) {
        Ok(()) => mark_terminal(conn, file, None),
        Err(failure) => {
            mark_terminal(conn, file, Some(&failure))?;
            Err(failure)
        }
    }
}

pub(crate) fn finish_tag_write_job(conn: &Connection, job_id: i64) -> Result<(), rusqlite::Error> {
    let uncertain: i64 = conn.query_row(
        "SELECT \
           (SELECT COUNT(*) FROM tag_write_job_files \
            WHERE job_id=?1 AND state IN ('pending', 'running')) + \
           (SELECT COUNT(*) FROM tag_write_journal v \
            JOIN tag_write_job_files f ON f.id=v.file_id \
            WHERE f.job_id=?1 AND v.outcome IN ('pending', 'prepared'))",
        [job_id],
        |row| row.get(0),
    )?;
    let state = if uncertain == 0 {
        "completed"
    } else {
        "interrupted"
    };
    let finished_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let changed = conn.execute(
        "UPDATE tag_write_jobs SET state=?1, finished_at=?2 \
         WHERE id=?3 AND state IN ('prepared', 'running')",
        params![
            state,
            i64::try_from(finished_at).unwrap_or(i64::MAX),
            job_id
        ],
    )?;
    affected_exactly(changed, 1)?;
    Ok(())
}
