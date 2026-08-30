use super::{DoctorError, LibraryDoctor};

impl LibraryDoctor<'_> {
    pub fn apply_auto_tier_with_lock(
        &mut self,
        scan: &super::DoctorScan,
        lock_attempt: crate::library::TagWriteLockAttempt,
        progress: impl FnMut(super::DoctorWriteProgress) -> super::DoctorWriteControl,
    ) -> Result<Option<super::DoctorWriteReport>, DoctorError> {
        let plan = super::DoctorReviewSession::from_scan(
            scan.clone(),
            super::DoctorReviewFilter::AutoApply,
        )
        .freeze_plan();
        if plan.changes().is_empty() {
            return Ok(None);
        }
        self.apply_review_plan_with_lock(&plan, lock_attempt, progress)
            .map(Some)
    }

    #[cfg(not(test))]
    pub fn apply_auto_tier(
        &mut self,
        scan: &super::DoctorScan,
        lock_attempt: crate::library::TagWriteLockAttempt,
        progress: impl FnMut(super::DoctorWriteProgress) -> super::DoctorWriteControl,
    ) -> Result<Option<super::DoctorWriteReport>, DoctorError> {
        self.apply_auto_tier_with_lock(scan, lock_attempt, progress)
    }

    #[cfg(test)]
    pub fn apply_auto_tier(
        &mut self,
        scan: &super::DoctorScan,
        progress: impl FnMut(super::DoctorWriteProgress) -> super::DoctorWriteControl,
    ) -> Result<Option<super::DoctorWriteReport>, DoctorError> {
        self.apply_auto_tier_with_lock(
            scan,
            crate::library::TagWriteLockAttempt::Unenforceable,
            progress,
        )
    }
}
