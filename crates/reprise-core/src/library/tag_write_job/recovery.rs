use std::path::PathBuf;

use rusqlite::Connection;

use super::super::tag_edit::{read_editable_tags, EditableTags};
use super::types::{RecoveryState, TagWriteRecovery};

#[derive(Debug)]
struct StoredField {
    field: String,
    before: Option<String>,
    after: Option<String>,
}

fn current_value(tags: &EditableTags, field: &str) -> Option<String> {
    match field {
        "title" => Some(tags.title.clone()),
        "artist" => Some(tags.artist.clone()),
        "album" => Some(tags.album.clone()),
        "album_artist" => Some(tags.album_artist.clone()),
        "year" => tags.year.map(|value| value.to_string()),
        "track_no" => tags.track_no.map(|value| value.to_string()),
        "genre" => Some(tags.genre.clone()),
        _ => None,
    }
}

fn classify(path: &std::path::Path, fields: &[StoredField]) -> RecoveryState {
    if fields.is_empty() {
        return RecoveryState::Conflict;
    }
    let Ok(tags) = read_editable_tags(path) else {
        return RecoveryState::Unavailable;
    };
    let matches_before = fields
        .iter()
        .all(|field| current_value(&tags, &field.field) == field.before);
    let matches_after = fields
        .iter()
        .all(|field| current_value(&tags, &field.field) == field.after);
    if matches_after {
        RecoveryState::Applied
    } else if matches_before {
        RecoveryState::NotApplied
    } else {
        RecoveryState::Conflict
    }
}

pub fn recover_incomplete_tag_write_jobs(
    conn: &Connection,
) -> Result<Vec<TagWriteRecovery>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT j.id, f.id, f.track_id, f.path \
         FROM tag_write_jobs j \
         JOIN tag_write_job_files f ON f.job_id=j.id \
         WHERE j.state IN ('prepared', 'running', 'interrupted') \
           AND (f.state IN ('pending', 'running') OR EXISTS ( \
             SELECT 1 FROM tag_write_journal v \
             WHERE v.file_id=f.id AND v.outcome IN ('pending', 'prepared') \
           )) \
         ORDER BY j.id, f.position",
    )?;
    let files = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                PathBuf::from(row.get::<_, String>(3)?),
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut recoveries = Vec::with_capacity(files.len());
    for (job_id, file_id, track_id, path) in files {
        let mut fields_statement = conn.prepare(
            "SELECT field, before_value, after_value \
             FROM tag_write_journal WHERE file_id=?1 ORDER BY field",
        )?;
        let fields = fields_statement
            .query_map([file_id], |row| {
                Ok(StoredField {
                    field: row.get(0)?,
                    before: row.get(1)?,
                    after: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        recoveries.push(TagWriteRecovery {
            job_id,
            file_id,
            track_id,
            state: classify(&path, &fields),
            path,
        });
    }
    Ok(recoveries)
}
