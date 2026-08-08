use std::collections::{HashMap, HashSet};

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

#[derive(Debug, Clone)]
pub(super) struct PreviousTrackScan {
    pub(super) snapshot: DoctorTrackSnapshot,
    pub(super) proposals: Vec<DoctorProposal>,
    pub(super) unresolved_groups: Vec<DoctorUnresolvedGroup>,
}

pub(super) fn previous_scan_identities(
    conn: &Connection,
    scan_id: i64,
) -> Result<HashMap<i64, PreviousTrackScan>, DoctorError> {
    let mut tracks = load_tracks(conn, scan_id)?
        .into_iter()
        .map(|snapshot| {
            (
                snapshot.reference.track_id,
                PreviousTrackScan {
                    snapshot,
                    proposals: Vec::new(),
                    unresolved_groups: Vec::new(),
                },
            )
        })
        .collect::<HashMap<_, _>>();
    for proposal in load_proposals(conn, scan_id)? {
        if let Some(track) = tracks.get_mut(&proposal.track_id) {
            track.proposals.push(proposal);
        }
    }
    for group in load_groups(conn, scan_id)? {
        for track_id in group
            .members
            .iter()
            .map(|member| member.track_id)
            .collect::<HashSet<_>>()
        {
            if let Some(track) = tracks.get_mut(&track_id) {
                track.unresolved_groups.push(group.clone());
            }
        }
    }
    Ok(tracks)
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
              confidence, preselected, never_preselect, problem_class, resolved_release_mbid, \
              evidence_json, local_fallback_json) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
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
                proposal.never_preselect,
                proposal.problem_class.as_str(),
                proposal.resolved_release_mbid.as_deref(),
                serde_json::to_string(&proposal.evidence).map_err(|error| {
                    DoctorError::InvalidStoredData(format!("remote evidence: {error}"))
                })?,
                serde_json::to_string(&proposal.local_fallback).map_err(|error| {
                    DoctorError::InvalidStoredData(format!("local fallback: {error}"))
                })?
            ])?;
        }
    }
    for (position, group) in unresolved_groups.iter().enumerate() {
        transaction.execute(
            "INSERT INTO library_doctor_groups \
             (scan_id, position, field, group_key, local_fallback_json) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                scan_id,
                i64::try_from(position).unwrap_or(i64::MAX),
                group.field.as_str(),
                group.group_key,
                serde_json::to_string(&group.local_fallback).map_err(|error| {
                    DoctorError::InvalidStoredData(format!("local fallback: {error}"))
                })?
            ],
        )?;
        let group_id = transaction.last_insert_rowid();
        for (candidate_position, candidate) in group.candidates.iter().enumerate() {
            transaction.execute(
                "INSERT INTO library_doctor_group_candidates \
                 (group_id, position, candidate_value, candidate_count, evidence_json) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    group_id,
                    i64::try_from(candidate_position).unwrap_or(i64::MAX),
                    candidate.value.encode().unwrap_or_default(),
                    i64::try_from(candidate.count).unwrap_or(i64::MAX),
                    serde_json::to_string(&candidate.evidence).map_err(|error| {
                        DoctorError::InvalidStoredData(format!("remote evidence: {error}"))
                    })?
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

pub fn set_reviewed_scan(conn: &Connection, scan_id: i64) -> Result<(), DoctorError> {
    conn.execute(
        "UPDATE library_doctor_state SET reviewed_scan_id=?1 WHERE singleton=1",
        [scan_id],
    )?;
    Ok(())
}

pub fn reviewed_scan_id(conn: &Connection) -> Result<Option<i64>, DoctorError> {
    conn.query_row(
        "SELECT reviewed_scan_id FROM library_doctor_state WHERE singleton=1",
        [],
        |row| row.get(0),
    )
    .optional()
    .map(Option::flatten)
    .map_err(DoctorError::from)
}

impl super::LibraryDoctor<'_> {
    /// Marks the currently stored scan as acknowledged through the Core
    /// facade, keeping SQLite connections out of frontend code.
    pub fn set_reviewed_scan(&self, scan_id: i64) -> Result<(), DoctorError> {
        set_reviewed_scan(self.conn, scan_id)
    }

    pub fn reviewed_scan_id(&self) -> Result<Option<i64>, DoctorError> {
        reviewed_scan_id(self.conn)
    }
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

/// The `(track, field)` pairs this scan's own tag-write job has on disk.
///
/// The one place that SQL lives. `queries::doctor` used to carry its own copy,
/// and the sidebar count and the result page drifted apart as a result. A
/// revert sets these rows back to `reverted`, so an undone fix leaves this set
/// and becomes a finding again.
pub fn written_pairs(
    conn: &Connection,
    scan_id: i64,
) -> Result<HashSet<(i64, DoctorField)>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT f.track_id, v.field FROM tag_write_journal v \
         JOIN tag_write_job_files f ON v.file_id = f.id \
         JOIN tag_write_jobs j ON j.id = f.job_id \
         WHERE j.scan_id = ?1 AND j.kind = 'doctor_apply' AND v.outcome = 'applied'",
    )?;
    let pairs = statement
        .query_map([scan_id], |row| {
            let raw_field = row.get::<_, String>(1)?;
            let field = DoctorField::parse(&raw_field).ok_or_else(|| {
                rusqlite::Error::InvalidColumnType(1, raw_field, rusqlite::types::Type::Text)
            })?;
            Ok((row.get::<_, i64>(0)?, field))
        })?
        .collect::<Result<HashSet<_>, _>>()?;
    Ok(pairs)
}

/// Per snapshotted track: does it still look the way the scan read it?
///
/// Compared with the same `DoctorTrackRef` equality `load_tracks` uses, so
/// "changed under us" cannot mean two different things in two places. Callers
/// read it exactly as `DoctorReviewSession` does — `get(id).unwrap_or(true)` —
/// so a track with no snapshot at all counts as changed, which is the cautious
/// answer and the one the review list already gave.
pub fn stale_flags(conn: &Connection, scan_id: i64) -> Result<HashMap<i64, bool>, rusqlite::Error> {
    let mut statement = conn.prepare(&format!(
        "SELECT s.track_id, s.path, s.file_mtime, s.file_size, s.device, s.inode, \
                t.id, t.path, t.file_mtime, t.file_size, t.device, t.inode \
         FROM library_doctor_scan_tracks s \
         LEFT JOIN tracks t ON t.id = s.track_id AND {} \
         WHERE s.scan_id = ?1",
        crate::queries::PRESENT
    ))?;
    let rows = statement.query_map([scan_id], |row| {
        let snapshot = DoctorTrackRef {
            track_id: row.get(0)?,
            path: std::path::PathBuf::from(row.get::<_, String>(1)?),
            file_mtime: row.get(2)?,
            file_size: row.get(3)?,
            device: row.get(4)?,
            inode: row.get(5)?,
        };
        let current = match row.get::<_, Option<i64>>(6)? {
            Some(track_id) => Some(DoctorTrackRef {
                track_id,
                path: std::path::PathBuf::from(row.get::<_, String>(7)?),
                file_mtime: row.get(8)?,
                file_size: row.get(9)?,
                device: row.get(10)?,
                inode: row.get(11)?,
            }),
            None => None,
        };
        Ok((snapshot, current))
    })?;
    let mut stale = HashMap::new();
    for row in rows {
        let (snapshot, current) = row?;
        let changed = current.is_none_or(|current| current != snapshot);
        stale.insert(snapshot.track_id, changed);
    }
    Ok(stale)
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
         preselected, never_preselect, problem_class, resolved_release_mbid, evidence_json, \
         local_fallback_json \
         FROM library_doctor_proposals \
         WHERE scan_id=?1 ORDER BY position",
    )?;
    let proposals = statement
        .query_map([scan_id], |row| {
            let field_text = row.get::<_, String>(1)?;
            let source_text = row.get::<_, String>(4)?;
            let problem_text = row.get::<_, String>(8)?;
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
                never_preselect: row.get(7)?,
                problem_class,
                resolved_release_mbid: row.get(9)?,
                evidence: serde_json::from_str(&row.get::<_, String>(10)?).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        10,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?,
                local_fallback: serde_json::from_str(&row.get::<_, String>(11)?).map_err(
                    |error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            11,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    },
                )?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(DoctorError::from)?;
    // A proposal this scan's own job already wrote is finished, and a finished
    // proposal is not a finding. Dropping it here is what keeps every surface
    // agreeing: the summary, the review list and the sidebar count all read the
    // scan, so none of them can offer a change that is already on disk — which
    // is what happened after a restart, because our own write moves the file's
    // mtime and a moved mtime reads as "changed under us", i.e. as stale, and
    // stale rows fall out of the quiet tier into review.
    let written = written_pairs(conn, scan_id)?;
    Ok(proposals
        .into_iter()
        .filter(|proposal| !written.contains(&(proposal.track_id, proposal.field)))
        .collect())
}

fn load_groups(conn: &Connection, scan_id: i64) -> Result<Vec<DoctorUnresolvedGroup>, DoctorError> {
    let groups = conn
        .prepare(
            "SELECT id, field, group_key, local_fallback_json FROM library_doctor_groups \
             WHERE scan_id=?1 ORDER BY position",
        )?
        .query_map([scan_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    groups
        .into_iter()
        .map(|(group_id, field_text, group_key, local_fallback_json)| {
            let field = DoctorField::parse(&field_text)
                .ok_or_else(|| DoctorError::InvalidStoredData(format!("field {field_text}")))?;
            let candidates = conn
                .prepare(
                    "SELECT candidate_value, candidate_count, evidence_json \
                     FROM library_doctor_group_candidates WHERE group_id=?1 ORDER BY position",
                )?
                .query_map([group_id], |row| {
                    let count = row.get::<_, i64>(1)?;
                    Ok((
                        DoctorValue::decode(field, Some(row.get::<_, String>(0)?)),
                        count,
                        row.get::<_, String>(2)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .map(|(value, count, evidence)| {
                    Ok(DoctorCandidate {
                        value,
                        count: usize::try_from(count).unwrap_or_default(),
                        evidence: serde_json::from_str(&evidence).map_err(|error| {
                            DoctorError::InvalidStoredData(format!("remote evidence: {error}"))
                        })?,
                    })
                })
                .collect::<Result<Vec<_>, DoctorError>>()?;
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
                local_fallback: serde_json::from_str(&local_fallback_json).map_err(|error| {
                    DoctorError::InvalidStoredData(format!("local fallback: {error}"))
                })?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc_4c_never_preselect_survives_a_store_round_trip() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let proposals = vec![DoctorProposal {
            track_id: 7,
            field: DoctorField::Title,
            current: DoctorValue::Text("Full title".into()),
            proposed: DoctorValue::Text("Full".into()),
            source: ProposalSource::MusicBrainz,
            confidence: 49,
            preselected: false,
            never_preselect: true,
            problem_class: ProblemClass::CasingWhitespace,
            resolved_release_mbid: None,
            evidence: Vec::new(),
            local_fallback: None,
        }];

        persist_complete_scan(&CompleteScanData {
            conn: db.conn(),
            scope_kind: "selection",
            created_at: 1,
            options: DoctorScanOptions {
                remote_enabled: true,
            },
            checked_tracks: 0,
            skipped_tracks: 0,
            tracks: &[],
            proposals: &proposals,
            unresolved_groups: &[],
        })
        .unwrap();

        let loaded = last_complete_scan(db.conn()).unwrap().unwrap();
        assert!(loaded.proposals[0].never_preselect);
    }

    #[test]
    fn doc_1e_the_resolved_release_mbid_survives_a_store_round_trip() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let proposals = vec![DoctorProposal {
            track_id: 7,
            field: DoctorField::Album,
            current: DoctorValue::Text("Local album".into()),
            proposed: DoctorValue::Text("Matched album".into()),
            source: ProposalSource::MusicBrainz,
            confidence: 88,
            preselected: false,
            never_preselect: false,
            problem_class: ProblemClass::CasingWhitespace,
            resolved_release_mbid: Some("123e4567-e89b-12d3-a456-426614174001".into()),
            evidence: Vec::new(),
            local_fallback: None,
        }];

        persist_complete_scan(&CompleteScanData {
            conn: db.conn(),
            scope_kind: "selection",
            created_at: 1,
            options: DoctorScanOptions {
                remote_enabled: true,
            },
            checked_tracks: 0,
            skipped_tracks: 0,
            tracks: &[],
            proposals: &proposals,
            unresolved_groups: &[],
        })
        .unwrap();

        let loaded = last_complete_scan(db.conn()).unwrap().unwrap();
        assert_eq!(loaded.proposals, proposals);
    }
}
