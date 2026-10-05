use super::CoreError;
use rusqlite::ffi;

/// The five errors the classification has to tell apart. `rusqlite::Error` is
/// not `Clone`, so every call builds a fresh set.
fn sample_errors() -> Vec<rusqlite::Error> {
    vec![
        failure(ffi::SQLITE_BUSY),
        failure(ffi::SQLITE_LOCKED),
        failure(ffi::SQLITE_CONSTRAINT),
        failure(ffi::SQLITE_READONLY),
        rusqlite::Error::QueryReturnedNoRows,
    ]
}

fn failure(code: std::os::raw::c_int) -> rusqlite::Error {
    rusqlite::Error::SqliteFailure(ffi::Error::new(code), None)
}

#[test]
fn busy_and_locked_fold_into_busy() {
    for code in [ffi::SQLITE_BUSY, ffi::SQLITE_LOCKED] {
        let error = CoreError::from(failure(code));
        assert!(error.is_busy(), "code {code} must classify as busy");
        assert!(!error.is_conflict());
    }
}

#[test]
fn constraint_violation_folds_into_conflict() {
    let error = CoreError::from(failure(ffi::SQLITE_CONSTRAINT));
    assert!(error.is_conflict());
    assert!(!error.is_busy());
}

#[test]
fn read_only_and_no_rows_fold_into_storage() {
    for source in [
        failure(ffi::SQLITE_READONLY),
        rusqlite::Error::QueryReturnedNoRows,
    ] {
        let error = CoreError::from(source);
        assert!(!error.is_busy());
        assert!(!error.is_conflict());
        assert!(matches!(error, CoreError::Storage(_)));
    }
}

#[test]
fn display_is_the_sqlite_message() {
    for (original, converted) in sample_errors().into_iter().zip(sample_errors()) {
        assert_eq!(CoreError::from(converted).to_string(), original.to_string());
    }
}

#[test]
fn source_is_the_sqlite_error() {
    for (original, converted) in sample_errors().into_iter().zip(sample_errors()) {
        let error = CoreError::from(converted);
        let source = std::error::Error::source(&error).expect("storage failures carry a source");
        assert_eq!(source.to_string(), original.to_string());
    }
}

#[test]
fn round_trip_through_the_backend_type_is_lossless() {
    for (original, converted) in sample_errors().into_iter().zip(sample_errors()) {
        let busy_before = crate::library::stats::is_database_busy(&original);
        let back = rusqlite::Error::from(CoreError::from(converted));
        assert_eq!(back.to_string(), original.to_string());
        assert_eq!(crate::library::stats::is_database_busy(&back), busy_before);
    }
}

#[test]
fn is_database_busy_and_core_error_agree() {
    for (original, converted) in sample_errors().into_iter().zip(sample_errors()) {
        assert_eq!(
            crate::library::stats::is_database_busy(&original),
            CoreError::from(converted).is_busy()
        );
    }
}

#[test]
fn core_error_is_send_and_sync() {
    fn assert<T: Send + Sync>() {}
    assert::<CoreError>();
}
