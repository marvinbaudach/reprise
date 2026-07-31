//! File-backed database fixtures for GNOME tests that need direct SQL seeding.
//!
//! Production code only receives [`Db`]. Tests may open a separate, short-lived
//! connection to the same throwaway file so fixture setup does not widen
//! Core's public database boundary. The general fixture keeps the historical
//! online-enabled precondition used by feature tests; first-run tests use
//! [`open_fresh`] to exercise the real fresh-install default.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use reprise_core::db::{Db, DbError};
use rusqlite::Connection;

const FIXTURE_PREFIX: &str = "reprise-gnome-tests-";

/// How many fixture roots may survive a sweep. Each is tens of megabytes, so a
/// handful is a bounded cost while still leaving the last few runs inspectable
/// when a test failure needs a post-mortem.
const FIXTURE_KEEP: usize = 8;

/// A root younger than this is never removed, however many there are: a test
/// binary runs for seconds, and no sweep may pull the fixtures out from under a
/// run happening in parallel.
const FIXTURE_MIN_AGE: Duration = Duration::from_secs(5 * 60);

static FIXTURE_ROOT: OnceLock<tempfile::TempDir> = OnceLock::new();
static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

/// Removes fixture roots left behind by earlier test binaries.
///
/// `TempDir` cleans up when it drops, but this one lives in a `static`, and
/// Rust runs no destructors for statics at process exit — so every run used to
/// leave its whole fixture directory behind. On a host where the temp directory
/// is a tmpfs that is not a slow leak but a hard stop: a day of test runs left
/// 904 directories and 8.2 GB, and unrelated commands began failing with
/// ENOSPC.
///
/// Sweeping on the way in is deliberate rather than trying to make the drop
/// happen. A destructor cannot be relied on here at all — the process may also
/// abort, or a test may panic the harness — whereas a sweep is self-healing:
/// whatever any previous run left, the next one clears.
fn sweep_stale_fixture_roots() {
    sweep_fixture_roots_in(&std::env::temp_dir(), FIXTURE_KEEP, FIXTURE_MIN_AGE);
}

/// The sweep with its bounds injected, so a test can drive the real directory
/// walk and removal without backdating an mtime (which would mean taking on a
/// dependency purely for a test) — and, just as importantly, inside a directory
/// of its own. A test that swept the real temp directory would delete the
/// fixture root the running suite is using out from under it.
///
/// Keeping the newest `keep` roots rather than only expiring by age is what
/// actually bounds the cost: at roughly fourteen seconds a run, an age-only
/// rule still lets an agent-driven session accumulate dozens of roots inside
/// its window, which on a tmpfs is the same failure with extra steps.
fn sweep_fixture_roots_in(dir: &std::path::Path, keep: usize, min_age: Duration) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut roots: Vec<(std::time::SystemTime, std::path::PathBuf)> = entries
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(FIXTURE_PREFIX))
        })
        .filter_map(|entry| {
            let modified = entry.metadata().and_then(|data| data.modified()).ok()?;
            Some((modified, entry.path()))
        })
        .collect();
    // Newest first, so everything past `keep` is a candidate.
    roots.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    for (modified, path) in roots.into_iter().skip(keep) {
        let old_enough = modified.elapsed().is_ok_and(|age| age >= min_age);
        if old_enough {
            // Best effort: a parallel run may be removing the same directory.
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

pub(crate) fn open() -> Result<Db, DbError> {
    let db = open_fresh()?;
    reprise_core::online_sources::set_enabled(&db, true)?;
    connection(&db).execute("DELETE FROM change_log", [])?;
    Ok(db)
}

pub(crate) fn open_fresh() -> Result<Db, DbError> {
    let root = FIXTURE_ROOT.get_or_init(|| {
        sweep_stale_fixture_roots();
        tempfile::Builder::new()
            .prefix(FIXTURE_PREFIX)
            .tempdir()
            .expect("create GNOME database fixture directory")
    });
    let id = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
    let path = root.path().join(format!("fixture-{id}.sqlite3"));
    Db::open_migrated(Some(&path))
}

pub(crate) fn connection(db: &Db) -> Connection {
    let path = db.path().expect("GNOME test database must be file-backed");
    let connection = Connection::open(path).expect("open GNOME database fixture connection");
    connection
        .busy_timeout(Duration::from_secs(5))
        .expect("configure GNOME database fixture busy timeout");
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .expect("enable foreign keys for GNOME database fixture");
    connection
}

#[cfg(test)]
mod tests {
    use super::{sweep_fixture_roots_in, FIXTURE_PREFIX};
    use std::time::Duration;

    /// Three properties in one place, because they only hold together: the
    /// bound is kept, a run in flight is never disturbed, and nothing outside
    /// the prefix is touched.
    ///
    /// Swept inside a directory of its own — pointing this at the real temp
    /// directory would delete the fixture root this very test binary is using.
    #[test]
    fn the_sweep_bounds_fixture_roots_without_disturbing_a_live_run() {
        let dir = tempfile::Builder::new()
            .prefix("sweep-under-test-")
            .tempdir()
            .expect("create the sweep's own directory");
        let roots: Vec<_> = (0..3)
            .map(|n| dir.path().join(format!("{FIXTURE_PREFIX}{n}")))
            .collect();
        let unrelated = dir.path().join("unrelated-dir");
        for path in roots.iter().chain(std::iter::once(&unrelated)) {
            std::fs::create_dir_all(path).expect("create sweep fixture");
        }

        // Freshly created: the minimum age protects them whatever the bound.
        sweep_fixture_roots_in(dir.path(), 0, Duration::from_secs(60 * 60));
        for path in &roots {
            assert!(path.exists(), "a run in flight must never be swept");
        }

        // No minimum age, but room for everything: still nothing goes.
        sweep_fixture_roots_in(dir.path(), usize::MAX, Duration::ZERO);
        for path in &roots {
            assert!(path.exists(), "roots within the bound must survive");
        }

        // Enforce a bound: the excess goes, and the directory that is not ours
        // survives either way.
        sweep_fixture_roots_in(dir.path(), 1, Duration::ZERO);
        let survivors = roots.iter().filter(|path| path.exists()).count();
        assert_eq!(survivors, 1, "the sweep must keep exactly the bound");
        assert!(
            unrelated.exists(),
            "the sweep must only ever touch its own prefix"
        );
    }
}
