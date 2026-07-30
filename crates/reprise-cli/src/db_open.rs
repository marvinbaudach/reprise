//! Opening the library database for a CLI invocation.

use std::path::PathBuf;

use reprise_core::{db, db::Db};

use crate::error::CliError;

/// Opens and migrates the database the command should act on. `--db` points at
/// an explicit file (used by every test and by anyone pointing the CLI at a
/// non-default library); without it the standard per-user location is used.
///
/// Migration runs here, so a schema newer than this binary is rejected up
/// front as [`CliError::SchemaTooNew`] before any command logic runs.
pub fn open(db: Option<&PathBuf>) -> Result<Db, CliError> {
    let db = match db {
        Some(path) => Db::open_migrated(Some(path))?,
        None => Db::open_migrated(Some(&db::default_path()))?,
    };
    Ok(db)
}
