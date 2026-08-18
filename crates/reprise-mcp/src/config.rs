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
}

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
}
