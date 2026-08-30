use std::path::Path;

use reprise_core::library::TagWriteLockAttempt;

use crate::ui::strings;

#[derive(Debug)]
pub(super) struct TagWriteAdmissionFailure {
    pub(super) busy: bool,
    pub(super) detail: String,
}

impl TagWriteAdmissionFailure {
    pub(super) fn user_message(&self) -> String {
        if self.busy {
            strings::text(strings::TAG_WRITE_BUSY_SEE_PROGRESS)
        } else {
            strings::text(strings::TAG_EDIT_DATABASE_UNAVAILABLE)
        }
    }
}

pub(super) fn acquire(db_path: &Path) -> Result<TagWriteLockAttempt, TagWriteAdmissionFailure> {
    let db_dir = db_path.parent().ok_or_else(|| TagWriteAdmissionFailure {
        busy: false,
        detail: "database path has no parent directory".to_owned(),
    })?;
    let attempt = reprise_core::library::TagWriteLock::acquire(db_dir).map_err(|error| {
        TagWriteAdmissionFailure {
            busy: false,
            detail: error.to_string(),
        }
    })?;
    match attempt {
        TagWriteLockAttempt::Busy => Err(TagWriteAdmissionFailure {
            busy: true,
            detail: "another tag-writing job is already running".to_owned(),
        }),
        TagWriteLockAttempt::Held(_) | TagWriteLockAttempt::Unenforceable => Ok(attempt),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::library::tag_edit::WriteErrorKind;

    #[test]
    fn busy_write_slot_stays_distinguishable_from_a_tag_write_failure() {
        let dir = tempfile::tempdir().unwrap();
        let held = reprise_core::library::TagWriteLock::acquire(dir.path()).unwrap();
        assert!(matches!(held, TagWriteLockAttempt::Held(_)));
        let database = dir.path().join("reprise.db");

        let Err(failure) = acquire(&database) else {
            panic!("a live tag-write holder must refuse a second save");
        };

        assert!(failure.busy, "a contended lock must remain a busy slot");
        assert_eq!(
            failure.user_message(),
            strings::text(strings::TAG_WRITE_BUSY_SEE_PROGRESS)
        );
        assert_ne!(failure.user_message(), WriteErrorKind::Io.user_message());
    }
}
