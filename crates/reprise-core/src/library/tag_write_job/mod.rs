mod recovery;
mod store;
mod types;

pub use recovery::recover_incomplete_tag_write_jobs;
pub(crate) use recovery::recover_incomplete_tag_write_jobs_in;
pub(crate) use recovery::{recover_incomplete_tag_write_fields, TagWriteFieldRecovery};
#[cfg(test)]
pub(crate) use store::execute_tag_write_file;
#[cfg(test)]
pub(crate) use store::prepare_tag_write_job;
pub(crate) use store::{
    begin_tag_write_file, complete_tag_write_file, finish_tag_write_job,
    prepare_tag_write_job_with_lock, validate_tag_write_file, write_tag_write_file,
};
pub(crate) use types::{JournaledTagMutation, TagWriteJobLock, TagWriteJobSpec};
pub use types::{RecoveryState, TagWriteJobKind, TagWriteRecovery};

#[cfg(test)]
mod tests;
