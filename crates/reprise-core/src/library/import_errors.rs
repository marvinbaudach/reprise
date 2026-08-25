//! Task 1.7: typed import-error classification, episode tracking, and the
//! dismiss-skip fast path — the self-healing-list core a later UI task reads
//! from (grouping rows by kind, showing "seen in N scans", letting a row be
//! dismissed until the file actually changes). Declared directly in
//! `library/mod.rs` (`pub(crate) mod import_errors;`), the same way `mounts`
//! is — NOT via `scanner.rs`'s `#[path = ...] mod vanish;`/`mod mount;`
//! pattern those two siblings use.
//!
//! ## Classify at the source, never parse error text
//!
//! `scanner::track_meta::read_meta`'s only fallible line is `lofty::read_
//! from_path`. Before this task, its `Err` was immediately collapsed to
//! `e.to_string()` — from that point on every import failure was an
//! indistinguishable string, and the only way to group them (as the UI
//! needs to) would have been matching on lofty's `Display` text: a
//! formatting change in any future patch release of a third-party crate
//! would silently reclassify everything into `Unknown`, and no test would
//! go red, because no test knows the foreign string constant. Worse,
//! "permission denied" is not reliably obtainable that way at all — lofty
//! surfaces `EACCES` through an `io::Error`, whose `Display` text varies by
//! platform/libc. [`classify_lofty`] instead walks the typed error source
//! chain, breaking `io::Error` down further by `kind()`, and keeps the
//! original message only as `reason_detail` — a display payload this module
//! never inspects again.
//! [`classify_walkdir`] applies the same principle to directory-traversal
//! failures.
//!
//! ## Hint rows (Task 1.8): a query-layer contract, not a stored flag
//!
//! Task 1.8 lets a file with unreadable tags but an intact container import
//! anyway (`tracks.untagged = 1`, real `duration_ms` from the container's
//! properties — see `scanner_meta.rs`'s module doc comment for the full
//! two-pass rationale) instead of being refused. Its `import_errors` row is
//! kept alive rather than cleared — see `scanner.rs`'s `## Hint coexistence`
//! doc section on `scan_folder_inner` for exactly when it's cleared versus
//! refreshed — and becomes a HINT: "imported without metadata", not a
//! failure needing attention.
//!
//! There is deliberately NO separate `is_hint` column on `import_errors`: a
//! second column recording a fact already fully determined by `tracks`
//! would be a second truth that can drift out of sync with it (e.g. if a
//! later migration or manual fix touches one table but not the other).
//! Instead, hint-ness is derivable — a query-layer/UI contract every
//! consumer of this table must use identically:
//!
//! ```sql
//! -- `import_errors.path` row is a HINT iff:
//! EXISTS(
//!   SELECT 1 FROM tracks
//!   WHERE tracks.path = import_errors.path
//!     AND tracks.untagged = 1
//!     AND tracks.missing_since IS NULL   -- `queries::PRESENT`
//!     AND tracks.removed_at IS NULL      -- `queries::PRESENT`
//! )
//! ```
//!
//! A hint must never count toward the sidebar badge a later task adds: the
//! app is asking the user for tags there, not for help — a file that
//! already imported successfully, just without metadata, is not the kind of
//! problem that badge exists to surface.

use rusqlite::{OptionalExtension, Transaction};

use crate::models::ImportErrorKind;

/// Finds a typed error anywhere in an error's source chain, including the
/// outer error itself.
pub(crate) fn find_source<'a, T: std::error::Error + 'static>(
    mut error: &'a (dyn std::error::Error + 'static),
) -> Option<&'a T> {
    loop {
        if let Some(found) = error.downcast_ref::<T>() {
            return Some(found);
        }
        error = error.source()?;
    }
}

/// Preserves an error's complete explanation even when an outer error omits
/// its source from `Display`, as lofty's typed file errors do.
pub(crate) fn error_detail(mut error: &(dyn std::error::Error + 'static)) -> String {
    let mut detail = error.to_string();
    while let Some(source) = error.source() {
        detail.push_str(": ");
        detail.push_str(&source.to_string());
        error = source;
    }
    detail
}

/// Maps a lofty failure to `(kind, detail)` at the source — see this module's
/// doc comment for why classification must inspect typed errors rather than
/// either concrete lofty's `Display` text.
pub(crate) fn classify_lofty(e: &(dyn std::error::Error + 'static)) -> (ImportErrorKind, String) {
    let detail = error_detail(e);
    let kind = if find_source::<lofty::error::UnknownFormatError>(e).is_some() {
        ImportErrorKind::UnsupportedFormat
    } else if let Some(io_err) = find_source::<std::io::Error>(e) {
        match io_err.kind() {
            std::io::ErrorKind::PermissionDenied => ImportErrorKind::PermissionDenied,
            _ => ImportErrorKind::Io,
        }
    } else {
        ImportErrorKind::UnreadableTags
    };
    (kind, detail)
}

/// Maps a `walkdir` directory-traversal failure to a kind, the same
/// classify-at-the-source principle [`classify_lofty`] applies to lofty
/// errors. `err.io_error()` is `None` only for a symlink-loop error (no
/// underlying `io::Error` exists for that case) — see `walkdir::Error`'s own
/// doc comment — which this crate has no more specific bucket for than
/// `Unknown`.
pub(crate) fn classify_walkdir(err: &walkdir::Error) -> ImportErrorKind {
    match err.io_error().map(std::io::Error::kind) {
        Some(std::io::ErrorKind::PermissionDenied) => ImportErrorKind::PermissionDenied,
        Some(_) => ImportErrorKind::Io,
        None => ImportErrorKind::Unknown,
    }
}

/// Episode upsert: records one failed import attempt for `path`. A first
/// failure inserts a fresh row (`seen_count = 1`, `first_seen = last_seen =
/// now`); a repeat failure for a path that already has a row bumps
/// `seen_count` and `last_seen` while leaving `first_seen` untouched — this
/// is what makes repeated scans of the same broken file converge on ONE row
/// instead of the pre-Task-1.7 DELETE-then-INSERT pair's fresh-timestamp
/// churn (the `path` primary key, schema v10, is what makes the upsert
/// possible at all). `reason_kind`/`reason_detail` are always refreshed to
/// the latest attempt's classification: a file can fail for a different
/// reason on a later scan (e.g. permission fixed, but the tag data
/// underneath turns out corrupt too), and the row should reflect the most
/// recent diagnosis, not the first one.
pub(crate) fn record_error(
    tx: &Transaction,
    path: &str,
    kind: ImportErrorKind,
    detail: &str,
    now: i64,
) -> rusqlite::Result<()> {
    tx.execute(
        "INSERT INTO import_errors (path, reason_kind, reason_detail, first_seen, last_seen, seen_count) \
         VALUES (?1, ?2, ?3, ?4, ?4, 1) \
         ON CONFLICT(path) DO UPDATE SET reason_kind=?2, reason_detail=?3, last_seen=?4, seen_count=seen_count+1",
        rusqlite::params![path, kind.as_str(), detail, now],
    )?;
    Ok(())
}

/// Clears any `import_errors` row for `path` (a file that imported
/// successfully must not stay flagged). Returns whether a row actually
/// existed to delete — `true` is this crate's "healed" signal: a future
/// scan-summary consumer can count it without a second query, though this
/// task doesn't itself add such a counter.
pub(crate) fn clear_error(tx: &Transaction, path: &str) -> rusqlite::Result<bool> {
    let changed = tx.execute("DELETE FROM import_errors WHERE path = ?1", [path])?;
    Ok(changed > 0)
}

/// The dismiss-skip fast path: called BEFORE `read_meta`, with only a `stat`
/// (`mtime`/`size`) already in hand — never a tag parse — so a dismissed
/// file costs the scan almost nothing. Returns `true` when the caller should
/// skip re-parsing this file entirely (dismissed AND unchanged since the
/// dismissal); `false` in every other case, including "no row" and "a row
/// exists but was never dismissed" (`dismissed_mtime`/`dismissed_size` both
/// `NULL`).
///
/// When a row WAS dismissed but `mtime`/`size` no longer match what was
/// recorded at dismissal time, the file genuinely changed since the user
/// last saw it — this function reactivates the episode itself, in the same
/// call: it clears both `dismissed_*` columns and resets `first_seen = now`,
/// `seen_count = 0`. A fresh `first_seen` matters beyond bookkeeping: the
/// sidebar badge (a later task) counts rows where `first_seen > last_viewed`
/// to know what's new, and the old, dismissed episode's story is over — this
/// changed file deserves to look new again, not like the same stale
/// complaint the user already dismissed. `seen_count` resets to `0` rather
/// than `1` because this function never counts as a failed *attempt* by
/// itself (it's a stat, not a parse) — the caller's very next `record_error`
/// call, once the re-parse itself fails, is what takes it to `1`.
pub(crate) fn check_dismissed(
    tx: &Transaction,
    path: &str,
    mtime: i64,
    size: i64,
    now: i64,
) -> rusqlite::Result<bool> {
    let dismissed: Option<(Option<i64>, Option<i64>)> = tx
        .query_row(
            "SELECT dismissed_mtime, dismissed_size FROM import_errors WHERE path = ?1",
            [path],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((Some(dismissed_mtime), Some(dismissed_size))) = dismissed else {
        // No row at all, or a row that was never dismissed
        // (`dismissed_mtime`/`dismissed_size` both `NULL`) — proceed to the
        // normal read_meta path.
        return Ok(false);
    };
    if dismissed_mtime == mtime && dismissed_size == size {
        return Ok(true);
    }
    tx.execute(
        "UPDATE import_errors SET dismissed_mtime = NULL, dismissed_size = NULL, \
         first_seen = ?2, seen_count = 0 WHERE path = ?1",
        rusqlite::params![path, now],
    )?;
    Ok(false)
}

#[cfg(test)]
#[path = "import_errors_tests.rs"]
mod tests;
