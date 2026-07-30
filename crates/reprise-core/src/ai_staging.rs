//! The staging store for finished-but-undecided AI renders (Beschluss 15).
//!
//! When a job finishes, the worker writes its instrumental render here — a
//! real, immediately-playable FLAC that is **not** in the library and **not**
//! under any scan root. It stays until the user decides: **promote** it
//! (`ai_promotion` moves it into the library) or **discard** it. Crucially it
//! survives restarts — hours of compute never evaporate — so this store keeps
//! no in-memory index; the files on disk are the whole truth, and any fresh
//! `StagingStore` over the same root sees them.
//!
//! Paths are deterministic (`<root>/job-<id>.flac`) and keyed on the job id,
//! not a caller-supplied string, so there is no path-traversal surface. The
//! root is injectable: production resolves [`default_staging_dir`] under the
//! XDG data dir (like `db::default_path`), while every test points at a temp
//! dir and never touches `~/.local/share/reprise/staging`.

use std::path::{Path, PathBuf};

use crate::ai_jobs::{self, JobState};
use rusqlite::Connection;

/// The staging subdirectory under the XDG data dir
/// (`<data_dir>/reprise/staging`). Resolves the *same* base as
/// [`crate::db::default_path`], so app, CLI and MCP agree on where renders
/// live.
pub fn default_staging_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("reprise/staging")
}

/// One finished render sitting in staging, awaiting the save/discard decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagingEntry {
    /// The `ai_jobs.id` this render belongs to.
    pub job_id: i64,
    /// Absolute path of the render on disk.
    pub path: PathBuf,
    /// File size in bytes — the disk cost the conversion view shows
    /// (Beschluss 15: staging cost is visible, there is no silent reaper).
    pub size_bytes: u64,
}

/// A filesystem-backed staging area rooted at one directory. Cheap to
/// construct and stateless: it only ever computes paths and touches the disk.
#[derive(Debug, Clone)]
pub struct StagingStore {
    root: PathBuf,
}

/// The render file extension — staging renders are always FLAC (Beschluss 15).
const RENDER_EXTENSION: &str = "flac";
const RENDER_PREFIX: &str = "job-";
/// The extension of a claim-scoped, not-yet-published temp render
/// ([`StagingStore::temp_path_for_job`]). Never a `*.flac`, so it is invisible
/// to the render listing and the orphan sweep.
const TEMP_EXTENSION: &str = "partial";

impl StagingStore {
    /// A store rooted at an explicit directory — the test-isolation entry
    /// point (point it at a temp dir).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// A store rooted at the real XDG staging directory — the production
    /// entry point.
    pub fn with_default_dir() -> Self {
        Self::new(default_staging_dir())
    }

    /// The directory this store manages.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The deterministic render path for `job_id`. Derived purely from the
    /// integer id, so no key value can escape the root.
    pub fn path_for_job(&self, job_id: i64) -> PathBuf {
        self.root
            .join(format!("{RENDER_PREFIX}{job_id}.{RENDER_EXTENSION}"))
    }

    /// A claim-scoped temporary render path for `job_id`, tagged with the
    /// `worker` token and a `TEMP_EXTENSION` suffix. A worker renders here
    /// first, then publishes the finished file to its canonical
    /// [`path_for_job`](Self::path_for_job) only after an owner-guarded
    /// completion — so two workers legitimately holding the same job (a
    /// straggler whose lease expired and the reclaimer that finished it) never
    /// share the render file, and only the winner writes the canonical path. The
    /// `.partial` extension keeps these temps out of [`list`](Self::list) and
    /// [`sweep_orphans`](Self::sweep_orphans), which only see `*.flac` renders.
    pub fn temp_path_for_job(&self, job_id: i64, worker: i64) -> PathBuf {
        self.path_for_job(job_id)
            .with_extension(format!("{worker:016x}.{TEMP_EXTENSION}"))
    }

    /// Creates the staging directory if it does not exist. Callers do this
    /// once before a worker writes its first render.
    pub fn ensure_dir(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.root)
    }

    /// Whether a completed render exists for `job_id`.
    pub fn exists(&self, job_id: i64) -> bool {
        self.path_for_job(job_id).is_file()
    }

    /// Deletes the render for `job_id`, returning whether a file was actually
    /// removed. A missing render is `Ok(false)`, never an error — discard is
    /// idempotent (a double-click, or a discard racing a promotion, is
    /// harmless).
    pub fn discard(&self, job_id: i64) -> std::io::Result<bool> {
        let path = self.path_for_job(job_id);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// Lists every render currently in staging, ordered by job id — what the
    /// conversion view enumerates. A never-created staging dir lists as empty
    /// (not an error). Files that don't match `job-<id>.flac` are ignored, so
    /// a stray file never crashes the listing.
    pub fn list(&self) -> std::io::Result<Vec<StagingEntry>> {
        let entries = match std::fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let mut renders = Vec::new();
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let Some(job_id) = job_id_from_path(&path) else {
                continue;
            };
            let size_bytes = entry.metadata().map_or(0, |meta| meta.len());
            renders.push(StagingEntry {
                job_id,
                path,
                size_bytes,
            });
        }
        renders.sort_by_key(|render| render.job_id);
        Ok(renders)
    }
}

impl StagingStore {
    /// Removes every staging render whose job can no longer own it — an orphan
    /// sweep to run once at each worker startup:
    ///
    /// * a **saved** job (`done` with a `result_track_id`): its render was
    ///   promoted into the library, so the copy still here is a leftover;
    /// * a **cancelled** job (also the state of a user-discarded render); or
    /// * a render whose job **no longer exists**.
    ///
    /// Active (`queued`/`running`) and **undecided** (`done`, unsaved) renders
    /// are spared — the latter is Beschluss 15 retention: a finished render
    /// waits, visible, for the user's save/discard decision and is never
    /// silently reaped. A `failed` job's render is likewise spared (the worker
    /// discards a failed render itself, so none should exist), matching the
    /// documented removal set exactly rather than reaping speculatively.
    ///
    /// Idempotent. Returns the job ids whose renders were removed (ascending).
    /// Only `*.flac` renders are considered; claim-scoped `*.partial` temps are
    /// invisible to the listing this walks.
    pub fn sweep_orphans(&self, db: &crate::db::Db) -> Result<Vec<i64>, SweepError> {
        let conn = db.conn();
        let mut removed = Vec::new();
        for entry in self.list()? {
            if render_is_orphan(conn, entry.job_id)? {
                match std::fs::remove_file(&entry.path) {
                    Ok(()) => removed.push(entry.job_id),
                    // Already gone (listed then vanished): nothing to remove.
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
        Ok(removed)
    }
}

/// Whether the render for `job_id` is an orphan [`StagingStore::sweep_orphans`]
/// removes (see its doc for the exact rule).
fn render_is_orphan(conn: &Connection, job_id: i64) -> Result<bool, rusqlite::Error> {
    let Some(job) = ai_jobs::get_job_in(conn, job_id)? else {
        return Ok(true); // no job owns this render
    };
    Ok(match job.state {
        JobState::Cancelled => true,
        JobState::Done => job.result_track_id.is_some(), // saved => a leftover
        JobState::Queued | JobState::Running | JobState::Failed => false,
    })
}

/// Why a staging orphan sweep could not complete.
#[derive(Debug, thiserror::Error)]
pub enum SweepError {
    #[error("database error during staging sweep: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("filesystem error during staging sweep: {0}")]
    Io(#[from] std::io::Error),
}

/// Parses `job-<id>.flac` back to its job id, or `None` for any other name.
fn job_id_from_path(path: &Path) -> Option<i64> {
    if path.extension().and_then(|ext| ext.to_str()) != Some(RENDER_EXTENSION) {
        return None;
    }
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| stem.strip_prefix(RENDER_PREFIX))
        .and_then(|id| id.parse::<i64>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_render(store: &StagingStore, job_id: i64, bytes: &[u8]) {
        store.ensure_dir().unwrap();
        std::fs::write(store.path_for_job(job_id), bytes).unwrap();
    }

    #[test]
    fn path_for_job_is_deterministic_and_inside_the_root() {
        let store = StagingStore::new("/data/staging");
        assert_eq!(
            store.path_for_job(42),
            PathBuf::from("/data/staging/job-42.flac")
        );
        assert!(store.path_for_job(42).starts_with(store.root()));
    }

    #[test]
    fn default_staging_dir_lives_under_reprise() {
        assert!(default_staging_dir().ends_with("reprise/staging"));
    }

    #[test]
    fn write_then_exists_and_list_report_the_render() {
        let dir = tempfile::tempdir().unwrap();
        let store = StagingStore::new(dir.path());
        write_render(&store, 7, b"render-bytes");

        assert!(store.exists(7));
        assert_eq!(
            store.list().unwrap(),
            vec![StagingEntry {
                job_id: 7,
                path: store.path_for_job(7),
                size_bytes: 12,
            }]
        );
    }

    #[test]
    fn discard_deletes_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let store = StagingStore::new(dir.path());
        write_render(&store, 3, b"x");

        assert!(store.discard(3).unwrap(), "first discard removes the file");
        assert!(!store.exists(3));
        assert!(
            !store.discard(3).unwrap(),
            "discarding an absent render is a no-op, not an error"
        );
    }

    #[test]
    fn renders_survive_a_fresh_store_over_the_same_root() {
        let dir = tempfile::tempdir().unwrap();
        write_render(&StagingStore::new(dir.path()), 5, b"persist");
        // A brand-new store (as after a restart) sees the same render.
        let reopened = StagingStore::new(dir.path());
        assert!(reopened.exists(5));
        assert_eq!(reopened.list().unwrap().len(), 1);
    }

    #[test]
    fn listing_is_ordered_and_skips_foreign_files() {
        let dir = tempfile::tempdir().unwrap();
        let store = StagingStore::new(dir.path());
        write_render(&store, 30, b"c");
        write_render(&store, 10, b"a");
        write_render(&store, 20, b"b");
        // Junk that must never appear in the listing.
        std::fs::write(dir.path().join("notes.txt"), b"junk").unwrap();
        std::fs::write(dir.path().join("job-nan.flac"), b"junk").unwrap();

        let ids: Vec<i64> = store
            .list()
            .unwrap()
            .into_iter()
            .map(|r| r.job_id)
            .collect();
        assert_eq!(ids, [10, 20, 30]);
    }

    #[test]
    fn listing_a_missing_directory_is_empty_not_an_error() {
        let store = StagingStore::new("/definitely/not/here/staging");
        assert_eq!(store.list().unwrap(), Vec::new());
        assert!(!store.exists(1));
    }

    #[test]
    fn temp_path_for_job_is_worker_tagged_and_ignored_by_the_listing() {
        let dir = tempfile::tempdir().unwrap();
        let store = StagingStore::new(dir.path());
        let temp = store.temp_path_for_job(7, 5);
        assert_ne!(temp, store.path_for_job(7));
        assert_eq!(temp.extension().and_then(|e| e.to_str()), Some("partial"));
        // A stray temp on disk never appears as a render.
        store.ensure_dir().unwrap();
        std::fs::write(&temp, b"partial").unwrap();
        assert!(store.list().unwrap().is_empty());
        assert!(!store.exists(7));
    }

    #[test]
    fn sweep_orphans_removes_saved_cancelled_and_orphaned_but_spares_active_and_undecided() {
        let dir = tempfile::tempdir().unwrap();
        let store = StagingStore::new(dir.path());
        store.ensure_dir().unwrap();

        let db = crate::db::Db::open_in_memory().unwrap();
        // A track for the saved job's result_track_id foreign key.
        db.conn()
            .execute(
                "INSERT INTO tracks (id, path, title, artist, added_at, file_mtime, file_size) \
             VALUES (1, '/music/x.flac', 'X', 'A', 1, 1, 1)",
                [],
            )
            .unwrap();

        let make = |id: i64, status: &str, result: Option<i64>| {
            db.conn().execute(
                "INSERT INTO ai_jobs (id, kind, params_json, params_fingerprint, status, result_track_id, created_at) \
                 VALUES (?1, 'instrumental', '{}', ?2, ?3, ?4, 0)",
                rusqlite::params![id, format!("fp-{id}"), status, result],
            )
            .unwrap();
            std::fs::write(store.path_for_job(id), b"render").unwrap();
        };
        make(1, "running", None); // active     -> spare
        make(2, "done", None); //     undecided  -> spare (Beschluss 15)
        make(3, "done", Some(1)); //  saved      -> remove
        make(4, "cancelled", None); // cancelled -> remove
        make(5, "failed", None); //   failed     -> spare (documented set)
                                 // A render whose job never existed -> remove.
        std::fs::write(store.path_for_job(9), b"orphan").unwrap();

        let removed = store.sweep_orphans(&db).unwrap();

        assert_eq!(
            removed,
            vec![3, 4, 9],
            "removes only saved, cancelled, and job-less renders"
        );
        assert!(store.exists(1), "an active render is spared");
        assert!(store.exists(2), "an undecided render is spared");
        assert!(!store.exists(3));
        assert!(!store.exists(4));
        assert!(store.exists(5), "a failed job's render is not reaped here");
        assert!(!store.exists(9));

        // Idempotent: a second sweep finds nothing more to remove.
        assert!(store.sweep_orphans(&db).unwrap().is_empty());
    }
}
