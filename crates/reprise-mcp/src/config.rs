//! Launch configuration for the stdio MCP server.
//!
//! The MCP server is spawned by an agent client, so its only knob is which
//! database file to open. `--db <path>` (mirroring `reprise-cli`'s convention,
//! Beschluss 4) overrides the XDG-derived default from
//! [`reprise_core::db::default_path`]; the integration test harness uses it to
//! point the server at a throwaway database.

use std::path::PathBuf;

const DB_FLAG: &str = "--db";
const DB_FLAG_EQ: &str = "--db=";

/// Parsed command-line configuration.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Config {
    db_path: Option<PathBuf>,
}

impl Config {
    /// Parses `--db <path>` / `--db=<path>` out of the process arguments
    /// (already stripped of `argv[0]`). Any other argument is rejected so a
    /// typo can never silently open the wrong database.
    pub fn from_args<I>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut db_path = None;
        let mut iter = args.into_iter();
        while let Some(arg) = iter.next() {
            if arg == DB_FLAG {
                let value = iter
                    .next()
                    .ok_or_else(|| format!("{DB_FLAG} requires a path argument"))?;
                db_path = Some(PathBuf::from(value));
            } else if let Some(value) = arg.strip_prefix(DB_FLAG_EQ) {
                db_path = Some(PathBuf::from(value));
            } else {
                return Err(format!("unrecognized argument: {arg}"));
            }
        }
        Ok(Self { db_path })
    }

    /// The database path to open: the explicit `--db` value when given, else
    /// the core default (which honours `XDG_DATA_HOME`).
    pub fn database_path(&self) -> PathBuf {
        self.db_path
            .clone()
            .unwrap_or_else(reprise_core::db::default_path)
    }

    /// Where finished-but-undecided renders live (Beschluss 15). Derived as
    /// `<db parent>/staging`, so the default database
    /// (`<data>/reprise/reprise.db`) yields `<data>/reprise/staging` — exactly
    /// [`reprise_core::ai_staging::default_staging_dir`], the same directory the
    /// app and CLI workers use. A test that points `--db` at a temp file gets an
    /// isolated staging directory alongside it for free.
    pub fn staging_path(&self) -> PathBuf {
        let db = self.database_path();
        db.parent().map_or_else(
            || PathBuf::from(STAGING_SUBDIR),
            |parent| parent.join(STAGING_SUBDIR),
        )
    }
}

/// The staging subdirectory name, kept in sync with the last path component of
/// [`reprise_core::ai_staging::default_staging_dir`].
const STAGING_SUBDIR: &str = "staging";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_core_path_when_no_flag() {
        let config = Config::from_args(Vec::<String>::new()).unwrap();
        assert_eq!(config, Config::default());
        assert_eq!(config.database_path(), reprise_core::db::default_path());
    }

    #[test]
    fn parses_separate_db_flag() {
        let config = Config::from_args(vec!["--db".to_string(), "/tmp/x.db".to_string()]).unwrap();
        assert_eq!(config.database_path(), PathBuf::from("/tmp/x.db"));
    }

    #[test]
    fn parses_joined_db_flag() {
        let config = Config::from_args(vec!["--db=/tmp/y.db".to_string()]).unwrap();
        assert_eq!(config.database_path(), PathBuf::from("/tmp/y.db"));
    }

    #[test]
    fn rejects_db_flag_without_value() {
        assert!(Config::from_args(vec!["--db".to_string()]).is_err());
    }

    #[test]
    fn rejects_unknown_argument() {
        assert!(Config::from_args(vec!["--http".to_string()]).is_err());
    }

    #[test]
    fn default_staging_path_matches_core_default() {
        let config = Config::from_args(Vec::<String>::new()).unwrap();
        assert_eq!(
            config.staging_path(),
            reprise_core::ai_staging::default_staging_dir(),
            "with no --db the staging dir must equal the core default the app/CLI use"
        );
    }

    #[test]
    fn staging_path_sits_beside_an_explicit_db() {
        let config = Config::from_args(vec!["--db=/data/reprise/reprise.db".to_string()]).unwrap();
        assert_eq!(
            config.staging_path(),
            PathBuf::from("/data/reprise/staging")
        );
    }
}
