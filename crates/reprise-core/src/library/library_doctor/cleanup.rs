use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;

use rusqlite::{Connection, OptionalExtension};

use super::write::{decoded, prepare_job, run_job, InputChange};
use super::{
    DoctorCleanup, DoctorCleanupReport, DoctorError, DoctorField, DoctorReviewRowId,
    DoctorTrackRef, DoctorWriteControl, DoctorWriteProgress, LibraryDoctor,
};

const ELIGIBLE: &str = "j.kind='doctor_apply' \
    AND j.state IN ('completed', 'cancelled', 'interrupted') \
    AND EXISTS (SELECT 1 FROM tag_write_job_files af \
      JOIN tag_write_journal av ON av.file_id=af.id \
      WHERE af.job_id=j.id AND av.outcome='applied') \
    AND NOT EXISTS (SELECT 1 FROM tag_write_job_files uf \
      JOIN tag_write_journal uv ON uv.file_id=uf.id \
      WHERE uf.job_id=j.id AND uv.outcome IN ('pending', 'prepared'))";

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
    pub fn last_cleanup(&self) -> Result<Option<DoctorCleanup>, DoctorError> {
        let scan_id = self
            .conn
            .query_row(
                &format!("SELECT MAX(j.scan_id) FROM tag_write_jobs j WHERE {ELIGIBLE}"),
                [],
                |row| row.get::<_, Option<i64>>(0),
            )
            .optional()?
            .flatten();
        let Some(scan_id) = scan_id else {
            return Ok(None);
        };
        let mut statement = self.conn.prepare(&format!(
            "SELECT j.id, j.created_at, f.track_id FROM tag_write_jobs j \
             JOIN tag_write_job_files f ON f.job_id=j.id \
             WHERE j.scan_id=?1 AND {ELIGIBLE} GROUP BY j.id, f.track_id ORDER BY j.id"
        ))?;
        let rows = statement
            .query_map([scan_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let job_ids = rows
            .iter()
            .map(|(job_id, _, _)| *job_id)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let created_at = rows
            .iter()
            .map(|(_, created_at, _)| *created_at)
            .max()
            .unwrap_or_default();
        let track_count = rows
            .iter()
            .map(|(_, _, track_id)| *track_id)
            .collect::<HashSet<_>>()
            .len();
        Ok(Some(DoctorCleanup {
            scan_id,
            job_ids,
            created_at,
            track_count,
        }))
    }

    pub fn revert_last_cleanup(
        &mut self,
        mut progress: impl FnMut(DoctorWriteProgress) -> DoctorWriteControl,
    ) -> Result<Option<DoctorCleanupReport>, DoctorError> {
        let Some(cleanup) = self.last_cleanup()? else {
            return Ok(None);
        };
        let jobs = cleanup
            .job_ids
            .iter()
            .rev()
            .map(|job_id| {
                let inputs = revert_inputs(self.conn, *job_id)?;
                let track_count = inputs
                    .iter()
                    .map(|input| input.track.track_id)
                    .collect::<HashSet<_>>()
                    .len();
                Ok((*job_id, inputs, track_count))
            })
            .collect::<Result<Vec<_>, DoctorError>>()?;
        let total_tracks = jobs.iter().map(|(_, _, count)| count).sum();
        let mut completed_tracks = 0;
        let mut cancelled = false;
        let mut reports = Vec::new();
        for (source_job_id, inputs, track_count) in jobs {
            let (job_id, files) = prepare_job(
                self.conn,
                "doctor_revert",
                Some(source_job_id),
                Some(cleanup.scan_id),
                &inputs,
            )?;
            let report = run_job(
                self.conn,
                job_id,
                &files,
                Some(source_job_id),
                |job_progress| {
                    let control = progress(DoctorWriteProgress {
                        completed_tracks: completed_tracks + job_progress.completed_tracks,
                        total_tracks,
                    });
                    cancelled |= control == DoctorWriteControl::Cancel;
                    control
                },
            )?;
            reports.push(report);
            completed_tracks += track_count;
            if cancelled {
                break;
            }
        }
        Ok(Some(DoctorCleanupReport {
            reverted_tracks: reports.iter().map(|report| report.updated_tracks).sum(),
            failed_tracks: reports.iter().map(|report| report.failed_tracks).sum(),
            conflict_tracks: reports.iter().map(|report| report.conflict_tracks).sum(),
            unavailable_tracks: reports.iter().map(|report| report.unavailable_tracks).sum(),
            reports,
            cancelled,
        }))
    }
}
