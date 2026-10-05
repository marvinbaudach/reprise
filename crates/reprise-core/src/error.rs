//! The engine's error type for callers outside the engine. Today it classifies storage failures;
//! facades convert to it at the boundary, internals keep their own error types.

/// A storage failure, with the SQLite error behind it. The field is private on purpose: callers
/// read `Display` or walk `source()`, they never match the backend's type.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct StorageError(#[source] rusqlite::Error);

/// What a core facade hands out when the library store fails.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CoreError {
    /// `SQLITE_BUSY` or `SQLITE_LOCKED`: another writer holds the store; the caller may retry.
    #[error(transparent)]
    Busy(StorageError),
    /// A constraint rejected the write (foreign key, uniqueness, NOT NULL).
    #[error(transparent)]
    Conflict(StorageError),
    /// Every other storage failure.
    #[error(transparent)]
    Storage(StorageError),
}

impl CoreError {
    /// Another writer holds the store; offering the write again may succeed.
    #[must_use]
    pub fn is_busy(&self) -> bool {
        matches!(self, Self::Busy(_))
    }

    /// A constraint rejected the write, for example a track id that does not exist.
    #[must_use]
    pub fn is_conflict(&self) -> bool {
        matches!(self, Self::Conflict(_))
    }
}

/// Shared predicate: `SqliteFailure` whose code is `DatabaseBusy` or `DatabaseLocked`.
pub(crate) fn sqlite_error_is_busy(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(failure, _)
            if matches!(
                failure.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            )
    )
}

/// Shared predicate: `SqliteFailure` whose code is `ConstraintViolation`.
pub(crate) fn sqlite_error_is_conflict(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(failure, _)
            if failure.code == rusqlite::ErrorCode::ConstraintViolation
    )
}

impl From<rusqlite::Error> for CoreError {
    fn from(error: rusqlite::Error) -> Self {
        if sqlite_error_is_busy(&error) {
            Self::Busy(StorageError(error))
        } else if sqlite_error_is_conflict(&error) {
            Self::Conflict(StorageError(error))
        } else {
            Self::Storage(StorageError(error))
        }
    }
}

/// Transitional: lets every internal `Result<_, rusqlite::Error>` and every frontend `?` keep
/// compiling while the internal signatures are still converted. Lossless for all current variants.
/// Delete it together with the last internal `rusqlite::Error` signature.
impl From<CoreError> for rusqlite::Error {
    fn from(error: CoreError) -> Self {
        match error {
            CoreError::Busy(inner) | CoreError::Conflict(inner) | CoreError::Storage(inner) => {
                inner.0
            }
        }
    }
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
