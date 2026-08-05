use reprise_core::library::TagWriteBusy;
use reprise_core::library_doctor::DoctorError;

use super::jobs::JobFailure;
use crate::ui::strings;

/// The silent apply is the first write that runs without the user asking for
/// it, so its failure toast is the first one they never clicked for. "Another
/// job is writing" and "the database is broken" call for different reactions —
/// waiting versus reporting — and must not arrive as the same sentence.
#[test]
fn doc_5c_a_busy_write_slot_stays_distinguishable_from_a_broken_one() {
    let busy = JobFailure::from(DoctorError::TagWriteBusy(TagWriteBusy));
    let broken = JobFailure::from(DoctorError::InvalidStoredData("proposals.field".to_owned()));

    assert!(busy.busy, "TagWriteBusy must classify as busy");
    assert!(!broken.busy, "stored-data damage is not a busy slot");
    assert_ne!(busy.user_message(), broken.user_message());
    assert_eq!(
        busy.user_message(),
        strings::text(strings::TAG_WRITE_BUSY),
        "the busy case has a translated sentence of its own"
    );
}

/// Rust error text is written for a log, not for a person: it names columns,
/// files and internal states. A toast the user never asked for must not be the
/// place they meet it.
#[test]
fn doc_5c_an_internal_failure_never_reaches_the_user_verbatim() {
    let failure = JobFailure::from(DoctorError::InvalidStoredData(
        "library_doctor_proposals.field".to_owned(),
    ));

    assert!(
        !failure
            .user_message()
            .contains("library_doctor_proposals.field"),
        "internal detail leaked into the toast: {}",
        failure.user_message()
    );
    assert!(
        failure.detail.contains("library_doctor_proposals.field"),
        "the detail must survive for the log"
    );
}
