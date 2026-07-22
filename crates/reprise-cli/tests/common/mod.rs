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

    /// Like [`run`](Self::run) but with extra environment variables — used to
    /// isolate `XDG_DATA_HOME` so the real backend's `default_model_dir` cannot
    /// read (or find) the user's real provisioned model.
    pub fn run_env(&self, envs: &[(&str, &str)], args: &[&str]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_reprise-cli"));
        command.arg("--db").arg(&self.db).args(args);
        for (key, value) in envs {
            command.env(key, value);
        }
        command.output().expect("run reprise-cli")
    }

    /// Spawns the CLI (with `--db` prepended) without waiting — for launching a
    /// long-running worker the test kills or races against.
    pub fn spawn(&self, args: &[&str]) -> std::process::Child {
        Command::new(env!("CARGO_BIN_EXE_reprise-cli"))
            .arg("--db")
            .arg(&self.db)
            .args(args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn reprise-cli")
    }

    /// Every `ai_job` lifecycle event as `(job_id, op)` pairs, via the core
    /// change-log facade — the ground truth for "was this job claimed once".
    pub fn ai_job_events(&self) -> Vec<(String, String)> {
        reprise_core::events::read_since(&self.conn(), 0, None)
            .expect("read change log")
            .into_iter()
            .filter(|change| change.entity == "ai_job")
            .map(|change| (change.entity_id, change.operation))
            .collect()
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

    /// Seeds one track whose `path` is a real, readable FLAC copied into the
    /// temp dir — needed wherever a worker's fake backend copies the source
    /// through, or promotion reads its tags. Returns the on-disk path.
    pub fn seed_track_with_file(&self, id: i64) -> PathBuf {
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
        let path = self.dir.path().join(format!("track{id}.flac"));
        std::fs::copy(&source, &path).expect("copy fixture");
        let conn = self.conn();
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, album, album_artist, genre, duration_ms, added_at, file_mtime, file_size) \
             VALUES (?1, ?2, ?3, ?4, 'Test Album', ?4, 'Rock', 180000, 1000, 1, 1)",
            rusqlite::params![id, path.to_string_lossy(), format!("Song {id}"), format!("Artist {id}")],
        )
        .expect("seed track with file");
        path
    }

    /// Total number of rows in `ai_jobs` (all states) — direct SQL, allowed in
    /// tests, so the dedup assertions can pin "exactly one job row".
    pub fn ai_job_row_count(&self) -> i64 {
        self.conn()
            .query_row("SELECT COUNT(*) FROM ai_jobs", [], |row| row.get(0))
            .expect("count ai_jobs")
    }

    /// The stored status string of one job (direct SQL).
    pub fn ai_job_status(&self, job_id: i64) -> Option<String> {
        self.conn()
            .query_row(
                "SELECT status FROM ai_jobs WHERE id = ?1",
                [job_id],
                |row| row.get(0),
            )
            .ok()
    }

    /// Marks a job `done` with an empty (unsaved) result and drops a real FLAC
    /// render into the staging dir — the state promotion/discard act on, mirrored
    /// from core's own `ai_promotion` tests. Returns the render path.
    pub fn stage_done_render(&self, staging_dir: &std::path::Path, job_id: i64) -> PathBuf {
        let store = reprise_core::ai_staging::StagingStore::new(staging_dir);
        store.ensure_dir().expect("ensure staging dir");
        let render = store.path_for_job(job_id);
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
        std::fs::copy(&source, &render).expect("copy render");
        self.conn()
            .execute("UPDATE ai_jobs SET status = 'done' WHERE id = ?1", [job_id])
            .expect("mark job done");
        render
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
