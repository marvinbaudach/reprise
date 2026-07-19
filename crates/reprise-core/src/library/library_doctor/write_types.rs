use std::path::PathBuf;

use super::{DoctorField, DoctorReviewRowId, DoctorValue};
use crate::library::tag_edit::WriteErrorKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorWriteControl {
    Continue,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DoctorWriteProgress {
    pub completed_tracks: usize,
    pub total_tracks: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorWriteRowState {
    Applied,
    Reverted,
    Cancelled,
    Conflict,
    Unavailable,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorWriteRow {
    pub row_id: Option<DoctorReviewRowId>,
    pub track_id: i64,
    pub path: PathBuf,
    pub field: DoctorField,
    pub expected: DoctorValue,
    pub proposed: DoctorValue,
    pub state: DoctorWriteRowState,
    pub file_written: bool,
    pub error_kind: Option<WriteErrorKind>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorWriteReport {
    pub job_id: i64,
    pub source_job_id: Option<i64>,
    pub updated_tracks: usize,
    pub cancelled_tracks: usize,
    pub failed_tracks: usize,
    pub conflict_tracks: usize,
    pub unavailable_tracks: usize,
    pub rows: Vec<DoctorWriteRow>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DoctorCleanup {
    pub job_id: i64,
    pub scan_id: i64,
    pub created_at: i64,
    pub track_count: usize,
}
