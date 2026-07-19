use std::path::PathBuf;

use super::super::tag_mutation::PreparedTagMutation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagWriteJobKind {
    TagEditor,
    DoctorApply,
    DoctorRevert,
}

impl TagWriteJobKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::TagEditor => "tag_editor",
            Self::DoctorApply => "doctor_apply",
            Self::DoctorRevert => "doctor_revert",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TagWriteJobSpec {
    pub(crate) kind: TagWriteJobKind,
    pub(crate) source_job_id: Option<i64>,
    pub(crate) scan_id: Option<i64>,
}

impl TagWriteJobSpec {
    pub(crate) const fn tag_editor() -> Self {
        Self {
            kind: TagWriteJobKind::TagEditor,
            source_job_id: None,
            scan_id: None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct JournaledTagMutation {
    pub(crate) file_id: i64,
    pub(crate) position: usize,
    pub(crate) field_count: usize,
    pub(crate) mutation: PreparedTagMutation,
}

#[derive(Debug)]
pub(crate) struct PreparedTagWriteJob {
    pub(crate) id: i64,
    pub(crate) files: Vec<JournaledTagMutation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryState {
    Applied,
    NotApplied,
    Conflict,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagWriteRecovery {
    pub job_id: i64,
    pub file_id: i64,
    pub track_id: i64,
    pub path: PathBuf,
    pub state: RecoveryState,
}
