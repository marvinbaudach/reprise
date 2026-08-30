use std::path::Path;

use rusqlite::OptionalExtension;

use super::{DoctorError, LibraryDoctor};
use crate::library::tag_write_job::TagWriteJobKind;
use crate::library::{TagWriteLiveness, TagWriteLock};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagWriteSlotOwner {
    pub job_id: i64,
    pub kind: TagWriteJobKind,
    pub completed_tracks: usize,
    pub total_tracks: usize,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TagWriteSlotStatus {
    Free,
    Busy(TagWriteSlotOwner),
    Orphaned(TagWriteSlotOwner),
}

pub(super) fn tag_write_slot_status_for_liveness(
    owner: TagWriteSlotOwner,
    liveness: TagWriteLiveness,
) -> TagWriteSlotStatus {
    match liveness {
        TagWriteLiveness::Live | TagWriteLiveness::Unknown => TagWriteSlotStatus::Busy(owner),
        TagWriteLiveness::Absent => TagWriteSlotStatus::Orphaned(owner),
    }
}

fn slot_owner(doctor: &LibraryDoctor<'_>) -> Result<Option<TagWriteSlotOwner>, DoctorError> {
    let stored = doctor
        .conn
        .query_row(
            "SELECT j.id, j.kind, \
                    (SELECT COUNT(*) FROM tag_write_job_files f \
                     WHERE f.job_id=j.id AND f.state='complete'), \
                    j.total_tracks, j.created_at \
             FROM tag_write_jobs j \
             WHERE j.state IN ('prepared', 'running') \
             ORDER BY j.created_at, j.id LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )
        .optional()?;
    let Some((job_id, kind, completed_tracks, total_tracks, created_at)) = stored else {
        return Ok(None);
    };
    let kind = TagWriteJobKind::parse(&kind).ok_or_else(|| {
        DoctorError::InvalidStoredData("tag-write job has an unknown kind".to_owned())
    })?;
    let completed_tracks = usize::try_from(completed_tracks).map_err(|_| {
        DoctorError::InvalidStoredData("tag-write job has a negative completed count".to_owned())
    })?;
    let total_tracks = usize::try_from(total_tracks).map_err(|_| {
        DoctorError::InvalidStoredData("tag-write job has a negative total count".to_owned())
    })?;
    Ok(Some(TagWriteSlotOwner {
        job_id,
        kind,
        completed_tracks,
        total_tracks,
        created_at,
    }))
}

impl LibraryDoctor<'_> {
    pub fn tag_write_slot_status(
        &mut self,
        db_dir: &Path,
    ) -> Result<TagWriteSlotStatus, DoctorError> {
        let Some(owner) = slot_owner(self)? else {
            return Ok(TagWriteSlotStatus::Free);
        };
        let status = tag_write_slot_status_for_liveness(owner, TagWriteLock::probe(db_dir));
        if matches!(status, TagWriteSlotStatus::Orphaned(_)) {
            match TagWriteLock::acquire(db_dir) {
                Ok(lock_attempt) => {
                    self.finalize_incomplete_writes(lock_attempt)?;
                }
                Err(error) => {
                    tracing::warn!(%error, "could not acquire the orphaned tag-write recovery lock");
                }
            }
        }
        Ok(status)
    }
}
