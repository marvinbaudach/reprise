//! Shared harness for the CLI integration tests.
//!
//! Every test runs the real built binary (`CARGO_BIN_EXE_reprise-cli`) against
//! a throwaway database in a `TempDir`, and arranges/inspects that database
//! through the same `reprise-core` facades the CLI uses. The user's real
//! library at `~/.local/share/reprise/reprise.db` is never touched — `--db`
//! always points at the temp file.

#![allow(dead_code)]

use std::path::PathBuf;
use std::process::{Command, Output};

use tempfile::TempDir;

pub struct Harness {
    // Kept alive so the temp directory (and its -wal/-shm siblings) survives
    // for the whole test.
    pub dir: TempDir,
    pub db: PathBuf,
}

impl Harness {
    /// Creates a fresh, migrated, empty database in a new temp directory.
    pub fn new() -> Self {
        let dir = TempDir::new().expect("create temp dir");
        let db = dir.path().join("reprise.db");
        reprise_core::db::open_migrated(Some(&db)).expect("migrate temp database");
        Self { dir, db }
    }

    /// A core-opened connection over the same database, for test arrangement
    /// and assertions.
    pub fn conn(&self) -> rusqlite::Connection {
        reprise_core::db::open_migrated(Some(&self.db)).expect("open temp database")
    }

    /// Runs the CLI with `--db <temp>` prepended, returning the captured output.
    pub fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_reprise-cli"))
            .arg("--db")
            .arg(&self.db)
            .args(args)
            .output()
            .expect("run reprise-cli")
    }

    /// Inserts `n` predictable tracks (`Song i` by `Artist i`) via direct SQL —
    /// deliberately *not* through an event-logging facade, so the change log
    /// stays empty until a command mutates it.
    pub fn seed_tracks(&self, n: i64) {
        let conn = self.conn();
        for i in 1..=n {
            conn.execute(
                "INSERT INTO tracks (path, title, artist, album, genre, duration_ms, added_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    format!("/music/song{i}.flac"),
                    format!("Song {i}"),
                    format!("Artist {i}"),
                    "Test Album",
                    "Rock",
                    180_000_i64,
                    1_000_i64 + i,
                ],
            )
            .expect("seed track");
        }
    }

    /// Number of change-log rows currently recorded (via the core facade).
    pub fn change_log_len(&self) -> usize {
        reprise_core::events::read_since(&self.conn(), 0, None)
            .expect("read change log")
            .len()
    }
}

pub fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// The process exit code (tests only run on platforms that report one).
pub fn code(output: &Output) -> i32 {
    output.status.code().expect("process exited with a code")
}

pub fn parse_json(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).expect("stdout is valid JSON")
}
