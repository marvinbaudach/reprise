use rusqlite::{Connection, OptionalExtension};

use super::{
    DoctorCandidate, DoctorError, DoctorField, DoctorGroupMember, DoctorProposal, DoctorScan,
    DoctorScanOptions, DoctorTrackRef, DoctorTrackSnapshot, DoctorUnresolvedGroup, DoctorValue,
    ProblemClass, ProposalSource,
};
use crate::library::tag_edit::EditableTags;

pub(super) struct CompleteScanData<'a> {
    pub conn: &'a Connection,
    pub scope_kind: &'a str,
    pub created_at: i64,
    pub options: DoctorScanOptions,
    pub checked_tracks: usize,
    pub skipped_tracks: usize,
    pub tracks: &'a [DoctorTrackSnapshot],
    pub proposals: &'a [DoctorProposal],
    pub unresolved_groups: &'a [DoctorUnresolvedGroup],
}

pub(super) fn persist_complete_scan(
    data: &CompleteScanData<'_>,
) -> Result<DoctorScan, DoctorError> {
    let conn = data.conn;
    let scope_kind = data.scope_kind;
    let created_at = data.created_at;
    let options = data.options;
    let checked_tracks = data.checked_tracks;
    let skipped_tracks = data.skipped_tracks;
    let tracks = data.tracks;
    let proposals = data.proposals;
    let unresolved_groups = data.unresolved_groups;
    let transaction = conn.unchecked_transaction()?;
    transaction.execute(
        "INSERT INTO library_doctor_scans \
         (scope_kind, created_at, remote_enabled, checked_tracks, skipped_tracks) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            scope_kind,
            created_at,
            options.remote_enabled,
            i64::try_from(checked_tracks).unwrap_or(i64::MAX),
            i64::try_from(skipped_tracks).unwrap_or(i64::MAX)
        ],
    )?;
    let scan_id = transaction.last_insert_rowid();
    {
        let mut statement = transaction.prepare(
            "INSERT INTO library_doctor_scan_tracks \
             (scan_id, position, track_id, path, file_mtime, file_size, device, inode, read_ok, \
              title, artist, album, album_artist, year, track_no, genre) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        )?;
        for (position, track) in tracks.iter().enumerate() {
            let tags = track.tags.as_ref();
            statement.execute(rusqlite::params![
                scan_id,
                i64::try_from(position).unwrap_or(i64::MAX),
                track.reference.track_id,
                track.reference.path.to_string_lossy(),
                track.reference.file_mtime,
                track.reference.file_size,
                track.reference.device,
                track.reference.inode,
                tags.is_some(),
                tags.map(|tags| &tags.title),
                tags.map(|tags| &tags.artist),
                tags.map(|tags| &tags.album),
                tags.map(|tags| &tags.album_artist),
                tags.and_then(|tags| tags.year),
                tags.and_then(|tags| tags.track_no),
                tags.map(|tags| &tags.genre),
            ])?;
        }
    }
    {
        let mut statement = transaction.prepare(
            "INSERT INTO library_doctor_proposals \
             (scan_id, position, track_id, field, current_value, proposed_value, source, \
              confidence, preselected, problem_class) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )?;
        for (position, proposal) in proposals.iter().enumerate() {
            statement.execute(rusqlite::params![
                scan_id,
                i64::try_from(position).unwrap_or(i64::MAX),
                proposal.track_id,
                proposal.field.as_str(),
                proposal.current.encode(),
                proposal.proposed.encode(),
                proposal.source.as_str(),
                proposal.confidence,
                proposal.preselected,
                proposal.problem_class.as_str()
            ])?;
        }
    }
    for (position, group) in unresolved_groups.iter().enumerate() {
        transaction.execute(
            "INSERT INTO library_doctor_groups (scan_id, position, field, group_key) \
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                scan_id,
                i64::try_from(position).unwrap_or(i64::MAX),
                group.field.as_str(),
                group.group_key
            ],
        )?;
        let group_id = transaction.last_insert_rowid();
        for (candidate_position, candidate) in group.candidates.iter().enumerate() {
            transaction.execute(
                "INSERT INTO library_doctor_group_candidates \
                 (group_id, position, candidate_value, candidate_count) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![
                    group_id,
                    i64::try_from(candidate_position).unwrap_or(i64::MAX),
                    candidate.value.encode().unwrap_or_default(),
                    i64::try_from(candidate.count).unwrap_or(i64::MAX)
                ],
            )?;
        }
        for (member_position, member) in group.members.iter().enumerate() {
            transaction.execute(
                "INSERT INTO library_doctor_group_members \
                 (group_id, position, track_id, current_value) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![
                    group_id,
                    i64::try_from(member_position).unwrap_or(i64::MAX),
                    member.track_id,
                    member.current.encode()
                ],
            )?;
        }
    }
    transaction.execute(
        "UPDATE library_doctor_state SET last_complete_scan_id=?1 WHERE singleton=1",
        [scan_id],
    )?;
    transaction.commit()?;

    Ok(DoctorScan {
        id: scan_id,
        scope_kind: scope_kind.to_owned(),
        created_at,
        options,
        checked_tracks,
        skipped_tracks,
        track_ids: tracks
            .iter()
            .map(|track| track.reference.track_id)
            .collect(),
        tracks: tracks.to_vec(),
        proposals: proposals.to_vec(),
        unresolved_groups: unresolved_groups.to_vec(),
    })
}

pub(super) fn last_complete_scan(conn: &Connection) -> Result<Option<DoctorScan>, DoctorError> {
    let scan_id = conn
        .query_row(
            "SELECT last_complete_scan_id FROM library_doctor_state WHERE singleton=1",
            [],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()?
        .flatten();
    let Some(scan_id) = scan_id else {
        return Ok(None);
    };
    let (scope_kind, created_at, remote_enabled, checked_tracks, skipped_tracks) = conn.query_row(
        "SELECT scope_kind, created_at, remote_enabled, checked_tracks, skipped_tracks \
         FROM library_doctor_scans WHERE id=?1",
        [scan_id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
            ))
        },
    )?;
    let tracks = load_tracks(conn, scan_id)?;
    let track_ids = tracks
        .iter()
        .map(|track| track.reference.track_id)
        .collect();
    let proposals = load_proposals(conn, scan_id)?;
    let unresolved_groups = load_groups(conn, scan_id)?;
    Ok(Some(DoctorScan {
        id: scan_id,
        scope_kind,
        created_at,
        options: DoctorScanOptions {
            remote_enabled: remote_enabled != 0,
        },
        checked_tracks: usize::try_from(checked_tracks).unwrap_or_default(),
        skipped_tracks: usize::try_from(skipped_tracks).unwrap_or_default(),
        track_ids,
        tracks,
        proposals,
        unresolved_groups,
    }))
}

fn load_tracks(conn: &Connection, scan_id: i64) -> Result<Vec<DoctorTrackSnapshot>, DoctorError> {
    let mut statement = conn.prepare(
        "SELECT track_id, path, file_mtime, file_size, device, inode, read_ok, \
         title, artist, album, album_artist, year, track_no, genre \
         FROM library_doctor_scan_tracks WHERE scan_id=?1 ORDER BY position",
    )?;
    let tracks = statement
        .query_map([scan_id], |row| {
            let reference = DoctorTrackRef {
                track_id: row.get(0)?,
                path: std::path::PathBuf::from(row.get::<_, String>(1)?),
                file_mtime: row.get(2)?,
                file_size: row.get(3)?,
                device: row.get(4)?,
                inode: row.get(5)?,
            };
            let read_ok = row.get::<_, bool>(6)?;
            let tags = if read_ok {
                Some(EditableTags {
                    title: row.get(7)?,
                    artist: row.get(8)?,
                    album: row.get(9)?,
                    album_artist: row.get(10)?,
                    year: row.get(11)?,
                    track_no: row.get(12)?,
                    genre: row.get(13)?,
                })
            } else {
                None
            };
            let stale = current_identity(conn, reference.track_id)?
                .is_none_or(|current| current != reference);
            Ok(DoctorTrackSnapshot {
                reference,
                tags,
                stale,
            })
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(DoctorError::from)?;
    Ok(tracks)
}

fn current_identity(
    conn: &Connection,
    track_id: i64,
) -> Result<Option<DoctorTrackRef>, rusqlite::Error> {
    conn.query_row(
        &format!(
            "SELECT id, path, file_mtime, file_size, device, inode \
             FROM tracks WHERE id=?1 AND {}",
            crate::queries::PRESENT
        ),
        [track_id],
        |row| {
            Ok(DoctorTrackRef {
                track_id: row.get(0)?,
                path: std::path::PathBuf::from(row.get::<_, String>(1)?),
                file_mtime: row.get(2)?,
                file_size: row.get(3)?,
                device: row.get(4)?,
                inode: row.get(5)?,
            })
        },
    )
    .optional()
}

fn load_proposals(conn: &Connection, scan_id: i64) -> Result<Vec<DoctorProposal>, DoctorError> {
    let mut statement = conn.prepare(
        "SELECT track_id, field, current_value, proposed_value, source, confidence, \
         preselected, problem_class FROM library_doctor_proposals \
         WHERE scan_id=?1 ORDER BY position",
    )?;
    let proposals = statement
        .query_map([scan_id], |row| {
            let field_text = row.get::<_, String>(1)?;
            let source_text = row.get::<_, String>(4)?;
            let problem_text = row.get::<_, String>(7)?;
            let field = DoctorField::parse(&field_text).ok_or_else(|| {
                rusqlite::Error::InvalidColumnType(
                    1,
                    field_text.clone(),
                    rusqlite::types::Type::Text,
                )
            })?;
            let source = ProposalSource::parse(&source_text).ok_or_else(|| {
                rusqlite::Error::InvalidColumnType(
                    4,
                    source_text.clone(),
                    rusqlite::types::Type::Text,
                )
            })?;
            let problem_class = ProblemClass::parse(&problem_text).ok_or_else(|| {
                rusqlite::Error::InvalidColumnType(
                    7,
                    problem_text.clone(),
                    rusqlite::types::Type::Text,
                )
            })?;
            Ok(DoctorProposal {
                track_id: row.get(0)?,
                field,
                current: DoctorValue::decode(field, row.get(2)?),
                proposed: DoctorValue::decode(field, row.get(3)?),
                source,
                confidence: row.get(5)?,
                preselected: row.get(6)?,
                problem_class,
            })
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(DoctorError::from)?;
    Ok(proposals)
}

fn load_groups(conn: &Connection, scan_id: i64) -> Result<Vec<DoctorUnresolvedGroup>, DoctorError> {
    let groups = conn
        .prepare(
            "SELECT id, field, group_key FROM library_doctor_groups \
             WHERE scan_id=?1 ORDER BY position",
        )?
        .query_map([scan_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    groups
        .into_iter()
        .map(|(group_id, field_text, group_key)| {
            let field = DoctorField::parse(&field_text)
                .ok_or_else(|| DoctorError::InvalidStoredData(format!("field {field_text}")))?;
            let candidates = conn
                .prepare(
                    "SELECT candidate_value, candidate_count \
                     FROM library_doctor_group_candidates WHERE group_id=?1 ORDER BY position",
                )?
                .query_map([group_id], |row| {
                    let count = row.get::<_, i64>(1)?;
                    Ok((DoctorValue::from_text(&row.get::<_, String>(0)?), count))
                })?
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .map(|(value, count)| DoctorCandidate {
                    value,
                    count: usize::try_from(count).unwrap_or_default(),
                })
                .collect();
            let members = conn
                .prepare(
                    "SELECT track_id, current_value FROM library_doctor_group_members \
                     WHERE group_id=?1 ORDER BY position",
                )?
                .query_map([group_id], |row| {
                    Ok(DoctorGroupMember {
                        track_id: row.get(0)?,
                        current: DoctorValue::decode(field, row.get(1)?),
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(DoctorUnresolvedGroup {
                field,
                group_key,
                candidates,
                members,
            })
        })
        .collect()
}
