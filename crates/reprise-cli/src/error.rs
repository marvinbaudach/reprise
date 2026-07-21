//! Typed CLI errors and the process exit codes they map to.
//!
//! Every fallible command returns [`CliError`]; `main` renders it to stderr
//! and exits with the matching [`ExitCode`]. Keeping the mapping in one place
//! makes the CLI's contract with automation explicit and testable: a caller
//! can branch on the exit status without scraping human text.

use reprise_core::db::DbError;

/// The exact user-facing message shown when the on-disk schema is newer than
/// this binary understands. Fixed verbatim by the multi-frontend-core plan
/// (section 2.5, "CLI-Festlegungen"): a stale binary must tell the user which
/// direction the drift runs so they update the CLI rather than the database.
///
/// It is German on purpose — the plan pins this precise wording, the same way
/// the repository already keeps verbatim German rule tokens (`[aktiv]`,
/// `[geplant]`) inside otherwise-English code. User-facing localization comes
/// later via gettext.
pub const SCHEMA_TOO_NEW_MESSAGE: &str =
    "Datenbank ist neuer als dieses CLI — bitte aktualisieren.";

/// Process exit codes. `Usage` (2) is emitted by clap itself on argument
/// errors; the rest are produced by [`CliError::exit_code`]. Values are stable
/// — automation depends on them — so append new variants rather than renumber.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExitCode {
    Success = 0,
    Usage = 2,
    NotFound = 3,
    ConfirmationRequired = 4,
    SchemaTooNew = 5,
    Database = 6,
    InvalidInput = 7,
    Unavailable = 8,
}

impl ExitCode {
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

/// A command failure, tagged with enough structure to pick an exit code and
/// render a clear message. Detail strings are pre-formatted English (except
/// the plan-pinned [`SCHEMA_TOO_NEW_MESSAGE`]).
#[derive(Debug)]
pub enum CliError {
    /// A referenced entity (e.g. a playlist id) does not exist.
    NotFound(String),
    /// A destructive command was invoked without its required `--yes`.
    ConfirmationRequired(String),
    /// The database schema is newer than this binary supports.
    SchemaTooNew,
    /// A database or I/O failure from a core facade.
    Database(String),
    /// A caller-supplied value or missing configuration prevents the command.
    InvalidInput(String),
    /// The command could not run against the current environment (e.g. a scan
    /// root that is not reachable) — distinct from a hard failure.
    Unavailable(String),
}

impl CliError {
    pub fn exit_code(&self) -> ExitCode {
        match self {
            Self::NotFound(_) => ExitCode::NotFound,
            Self::ConfirmationRequired(_) => ExitCode::ConfirmationRequired,
            Self::SchemaTooNew => ExitCode::SchemaTooNew,
            Self::Database(_) => ExitCode::Database,
            Self::InvalidInput(_) => ExitCode::InvalidInput,
            Self::Unavailable(_) => ExitCode::Unavailable,
        }
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(what) => write!(f, "{what} not found"),
            Self::ConfirmationRequired(what) => write!(f, "{what}"),
            Self::SchemaTooNew => write!(f, "{SCHEMA_TOO_NEW_MESSAGE}"),
            Self::Database(detail) => write!(f, "{detail}"),
            Self::InvalidInput(detail) => write!(f, "{detail}"),
            Self::Unavailable(detail) => write!(f, "{detail}"),
        }
    }
}

impl std::error::Error for CliError {}

impl From<DbError> for CliError {
    fn from(error: DbError) -> Self {
        match error {
            DbError::SchemaTooNew { .. } => Self::SchemaTooNew,
            other => Self::Database(other.to_string()),
        }
    }
}

impl From<rusqlite::Error> for CliError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Locks the stable exit-code contract automation depends on. `Success`
    /// and `Usage` are produced by `main`/clap rather than `CliError`, but the
    /// enum documents the whole contract, so their values are pinned here too.
    #[test]
    fn exit_codes_are_stable() {
        assert_eq!(ExitCode::Success.as_u8(), 0);
        assert_eq!(ExitCode::Usage.as_u8(), 2);
        assert_eq!(ExitCode::NotFound.as_u8(), 3);
        assert_eq!(ExitCode::ConfirmationRequired.as_u8(), 4);
        assert_eq!(ExitCode::SchemaTooNew.as_u8(), 5);
        assert_eq!(ExitCode::Database.as_u8(), 6);
        assert_eq!(ExitCode::InvalidInput.as_u8(), 7);
        assert_eq!(ExitCode::Unavailable.as_u8(), 8);
    }

    #[test]
    fn errors_map_to_their_exit_codes() {
        assert_eq!(
            CliError::NotFound("x".into()).exit_code(),
            ExitCode::NotFound
        );
        assert_eq!(
            CliError::ConfirmationRequired("x".into()).exit_code(),
            ExitCode::ConfirmationRequired
        );
        assert_eq!(CliError::SchemaTooNew.exit_code(), ExitCode::SchemaTooNew);
        assert_eq!(
            CliError::Database("x".into()).exit_code(),
            ExitCode::Database
        );
        assert_eq!(
            CliError::InvalidInput("x".into()).exit_code(),
            ExitCode::InvalidInput
        );
        assert_eq!(
            CliError::Unavailable("x".into()).exit_code(),
            ExitCode::Unavailable
        );
    }

    #[test]
    fn schema_too_new_renders_the_plan_pinned_message() {
        assert_eq!(CliError::SchemaTooNew.to_string(), SCHEMA_TOO_NEW_MESSAGE);
    }

    #[test]
    fn db_error_schema_too_new_maps_across() {
        let mapped: CliError = DbError::SchemaTooNew {
            found: 99,
            supported: 26,
        }
        .into();
        assert!(matches!(mapped, CliError::SchemaTooNew));
    }
}
