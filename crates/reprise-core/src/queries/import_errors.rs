//! Grouped import-error read/write queries (Task 2.4): the read/write half
//! the ImportErrors triage UI (`ui::import_errors_view`, a later task) is
//! built directly against, replacing the flat `query_import_errors`/
//! `ImportErrorRow`/`delete_import_error` trio one call at a time as that
//! UI is rebuilt. Split into its own sibling of `issues.rs`/`maintenance.rs`
//! rather than folded into either: `issues.rs` is already a cohesive unit
//! for the *missing-file* taxonomy (`MissingGroupKind`), and `maintenance.rs`
//! is close to the project's 800-line rule with its own already-large test
//! module — this is a third, equally cohesive taxonomy (`ImportErrorKind`)
//! that deserves the same treatment, not a grab-bag addition to either.
//!
//! ## The hint contract
//!
//! A file whose tags are broken but whose audio container is intact still
//! gets IMPORTED — `library::scanner` gives it a stem-derived title/album
//! and sets `tracks.untagged = 1` — and its `import_errors` row deliberately
//! SURVIVES alongside that track row, as a *hint* ("imported without
//! metadata") rather than a failure. There is no `import_errors.is_hint`
//! column: a second, hand-maintained boolean would be a second truth that
//! can drift from the `tracks` row it's supposed to describe. Instead,
//! [`ImportErrorEntry::is_hint`] is computed on every read via an `EXISTS`
//! against `tracks`, composing [`PRESENT`] rather than hand-writing its two
//! conditions (`missing_since IS NULL AND removed_at IS NULL`) a second
//! time — the same "one source of truth" reasoning [`PRESENT`] itself exists
//! for. A hint must never count toward the sidebar's Import-errors badge:
//! the app already solved that file (title/album from the filename), so a
//! badge for it would be a false alarm asking the user to act on something
//! that isn't actionable in the same way an unreadable/permission-denied
//! file is. `query_import_errors_grouped`'s non-dismissed rows carry the
//! flag; badge counting is a later task's job, filtering on it.
//!
//! ## Dismiss/restore semantics
//!
//! "Dismiss" records the file's current `(mtime, size)` into `dismissed_
//! mtime`/`dismissed_size` rather than deleting the row. The scanner (a
//! later task) is documented to skip a dismissed path with a cheap `stat`
//! instead of a full tag parse as long as both still match; the moment
//! either changes, the file has genuinely changed since it was dismissed,
//! so the scanner reactivates the row with a new episode (fresh `first_
//! seen`/`seen_count`) rather than trusting stale dismissal forever.
//!
//! [`restore_import_error`] only nulls the two `dismissed_*` columns — it
//! deliberately does NOT re-scan the file. An immediate retry is the UI's
//! job (a synchronous single-file scan, the same shape `ui::import_errors_
//! view`'s existing "Retry" button already runs), not this query's: a
//! read/write query module has no business owning a scan side effect, and
//! "restore" and "retry" are two different user intents that happen to
//! often be clicked together (the existing `handle_retry` already restores
//! by virtue of the scan re-recording the row) — see `handle_dismiss`/
//! `handle_retry` in that module for the pattern this mirrors.
//!
//! ## Group and row ordering
//!
//! [`query_import_errors_grouped`] groups by [`ImportErrorKind`] in the
//! order the enum itself declares its variants (`UnreadableTags`,
//! `PermissionDenied`, `UnsupportedFormat`, `Io`, `Unknown`) — a fixed,
//! deterministic order chosen so the UI's card order never reshuffles
//! between refreshes just because scan timing changed which kind happened
//! to accumulate rows first. `UnreadableTags` leads because it's the
//! single most common, most actionable case (re-tag the file); `Unknown`
//! trails because it's the catch-all with no clear remediation story. The
//! SQL `CASE` driving this binds `ImportErrorKind::as_str()` values as
//! parameters rather than hand-copying the literal strings a second time,
//! for the same "single source of truth" reason [`PRESENT`] is composed
//! rather than hand-copied elsewhere in this module — any reason string the
//! `CASE` doesn't recognize (including `"unknown"` itself) falls into the
//! same last bucket, mirroring [`ImportErrorKind::parse`]'s own fallback.
//!
//! Within a group/list, rows sort `last_seen DESC, path ASC`: the most
//! recently-still-failing file surfaces first (the freshest evidence is the
//! most relevant), with `path` as a deterministic tie-break for same-second
//! scan batches. [`query_dismissed_import_errors`] uses the identical order
//! for consistency — there's no separate "dismissed at" timestamp in the
//! schema to sort by instead (`dismissed_mtime`/`dismissed_size` are a file
//! stat fingerprint, not an action timestamp), so mirroring the active
//! list's own ordering is the simplest deterministic choice available.

use rusqlite::Connection;

use crate::models::ImportErrorKind;

use super::clauses::PRESENT;

/// An `import_errors` row is dismissed iff BOTH stat columns are set — see
/// the `import_errors` table's own "both NULL = not dismissed" schema
/// comment. [`dismiss_import_error`] always writes both together, so in
/// practice one implies the other, but both predicates below check both
/// columns rather than just one, matching the schema's documented
/// invariant exactly instead of assuming it holds.
const NOT_DISMISSED: &str = "dismissed_mtime IS NULL AND dismissed_size IS NULL";
const DISMISSED: &str = "dismissed_mtime IS NOT NULL AND dismissed_size IS NOT NULL";

/// The `is_hint` `EXISTS` fragment — see the module doc's "hint contract"
/// section. Correlated against the enclosing query's `import_errors.path`;
/// composes [`PRESENT`] rather than hand-writing `missing_since IS NULL AND
/// removed_at IS NULL` a second time, per Task 2.4's brief.
fn is_hint_expr() -> String {
    format!(
        "EXISTS(SELECT 1 FROM tracks WHERE tracks.path = import_errors.path \
         AND tracks.untagged = 1 AND {PRESENT})"
    )
}

/// Shared column list for both [`query_import_errors_grouped`] and
/// [`query_dismissed_import_errors`] — kept as one function so the two can
/// never drift on which columns (or which `is_hint` definition) they
/// select.
fn entry_select() -> String {
    let is_hint = is_hint_expr();
    format!(
        "SELECT path, reason_kind, reason_detail, first_seen, last_seen, seen_count, {is_hint} \
         FROM import_errors"
    )
}

fn row_to_entry(r: &rusqlite::Row) -> rusqlite::Result<ImportErrorEntry> {
    Ok(ImportErrorEntry {
        path: r.get(0)?,
        kind: ImportErrorKind::parse(&r.get::<_, String>(1)?),
        detail: r.get(2)?,
        first_seen: r.get(3)?,
        last_seen: r.get(4)?,
        seen_count: r.get(5)?,
        is_hint: r.get::<_, i64>(6)? != 0,
    })
}

/// One `import_errors` row, projected for the ImportErrors triage UI. `kind`
/// is the parsed [`ImportErrorKind`] (never the raw storage string — see
/// [`ImportErrorKind::parse`]'s own fallback-to-`Unknown` doc comment for
/// why this never fails to load a row). `is_hint` is computed, never
/// stored — see the module doc's "hint contract" section for why there is
/// no `is_hint` column to read instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportErrorEntry {
    pub path: String,
    pub kind: ImportErrorKind,
    pub detail: String,
    pub first_seen: i64,
    pub last_seen: i64,
    pub seen_count: i64,
    pub is_hint: bool,
}

/// Every non-dismissed `import_errors` row, grouped by [`ImportErrorKind`]
/// — the ImportErrors triage UI's main list. See the module doc's "Group
/// and row ordering" section for the exact, deterministic order both the
/// groups and the rows within each group are returned in. A kind with zero
/// matching rows is simply absent from the returned `Vec` (no "empty
/// group" entries), matching `query_missing_groups`'s own "a group with
/// zero matching rows is simply absent" convention (`issues.rs`).
pub fn query_import_errors_grouped(
    conn: &Connection,
) -> Result<Vec<(ImportErrorKind, Vec<ImportErrorEntry>)>, rusqlite::Error> {
    let sql = format!(
        "{} WHERE {NOT_DISMISSED} \
         ORDER BY CASE reason_kind \
           WHEN ?1 THEN 0 WHEN ?2 THEN 1 WHEN ?3 THEN 2 WHEN ?4 THEN 3 ELSE 4 END, \
         last_seen DESC, path COLLATE NOCASE ASC",
        entry_select()
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        rusqlite::params![
            ImportErrorKind::UnreadableTags.as_str(),
            ImportErrorKind::PermissionDenied.as_str(),
            ImportErrorKind::UnsupportedFormat.as_str(),
            ImportErrorKind::Io.as_str(),
        ],
        row_to_entry,
    )?;

    // The SQL `ORDER BY` already clusters same-kind rows into one
    // contiguous run, so folding consecutive entries into groups here is
    // just consuming that guarantee in Rust — never a second, independent
    // sort that could disagree with the SQL order.
    let mut groups: Vec<(ImportErrorKind, Vec<ImportErrorEntry>)> = Vec::new();
    for entry in rows {
        let entry = entry?;
        match groups.last_mut() {
            Some((kind, entries)) if *kind == entry.kind => entries.push(entry),
            _ => groups.push((entry.kind, vec![entry])),
        }
    }
    Ok(groups)
}

/// Every dismissed `import_errors` row — the triage UI's separate
/// "Dismissed" list/tab. See the module doc's "Group and row ordering"
/// section for why this uses the same `last_seen DESC, path ASC` order as
/// [`query_import_errors_grouped`]'s rows.
pub fn query_dismissed_import_errors(
    conn: &Connection,
) -> Result<Vec<ImportErrorEntry>, rusqlite::Error> {
    let sql = format!(
        "{} WHERE {DISMISSED} ORDER BY last_seen DESC, path COLLATE NOCASE ASC",
        entry_select()
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], row_to_entry)?;
    rows.collect()
}

/// Bare count of dismissed rows — for a "Dismissed (N)" tab label without
/// loading every row just to measure its length.
pub fn count_dismissed_import_errors(conn: &Connection) -> Result<u32, rusqlite::Error> {
    let count: i64 = conn.query_row(
        &format!("SELECT count(*) FROM import_errors WHERE {DISMISSED}"),
        [],
        |r| r.get(0),
    )?;
    Ok(count.max(0) as u32)
}

/// Dismisses one `import_errors` row by recording the file's `(mtime,
/// size)` fingerprint at dismissal time — see the module doc's "Dismiss/
/// restore semantics" section for why this is a stat snapshot, not a
/// delete. A path with no matching row is a silent no-op (`Ok(())`, zero
/// rows updated), matching this codebase's convention for a stale/unknown
/// path (e.g. `delete_import_error`, `track_id_for_path`'s callers) — the
/// caller races against the scanner clearing the row out from under a
/// dismiss click more plausibly than most callers in this crate.
pub fn dismiss_import_error(
    conn: &Connection,
    path: &str,
    mtime: i64,
    size: i64,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE import_errors SET dismissed_mtime = ?2, dismissed_size = ?3 WHERE path = ?1",
        rusqlite::params![path, mtime, size],
    )?;
    Ok(())
}

/// Bulk "Dismiss all" for the triage UI: dismisses every currently
/// non-dismissed row it can successfully `stat`, via a caller-supplied
/// callback rather than calling `std::fs::metadata` directly.
///
/// Core must not decide *how* to stat a path in a UI-triggered bulk
/// operation — path resolution (symlinks, mounted-but-slow filesystems,
/// platform-specific stat semantics) is exactly the kind of concern
/// `reprise-core` stays free of GTK/GIO/platform dependencies for
/// elsewhere in this crate (see this crate's `cargo tree` constraint on
/// `gtk4`/`libadwaita`/`gstreamer`/`zbus`); the caller (a later GUI task)
/// already has to walk these same paths for other reasons and owns
/// whatever stat primitive is appropriate for its platform.
///
/// A path that fails to stat (`now_stat` returns `None` — e.g. the file
/// vanished between the triage list being loaded and this bulk action
/// running) is skipped, not dismissed with placeholder/`NULL` values: this
/// function only ever calls [`dismiss_import_error`] with a real `(mtime,
/// size)` pair, so a skipped row is left in EXACTLY the state it was in
/// before this call — still non-dismissed, still visible, still eligible
/// for a future dismiss (single or bulk) once it can be stat-ed again.
/// Writing `NULL`/sentinel stat values for an unstat-able path was
/// considered and rejected: `NULL` in both columns is indistinguishable
/// from "never dismissed" (see [`NOT_DISMISSED`]'s doc comment) — the row
/// would look untouched to the scanner's dismissed-path skip check, which
/// is harmless, but ALSO look untouched to this very function on its next
/// run, so nothing is gained by writing it; a sentinel value in only one
/// column, meanwhile, would violate the schema's documented "both NULL or
/// both set" invariant and risk exactly the "permanently un-reactivatable"
/// failure mode the brief warns about, for zero benefit over simply
/// skipping.
///
/// Returns the number of rows actually dismissed (a subset of the
/// non-dismissed rows that existed when this call started).
pub fn dismiss_all_import_errors(
    conn: &Connection,
    now_stat: &dyn Fn(&str) -> Option<(i64, i64)>,
) -> Result<u32, rusqlite::Error> {
    let paths: Vec<String> = {
        let mut stmt = conn.prepare(&format!(
            "SELECT path FROM import_errors WHERE {NOT_DISMISSED}"
        ))?;
        let paths = stmt
            .query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        paths
    };
    let mut dismissed_count = 0u32;
    for path in paths {
        let Some((mtime, size)) = now_stat(&path) else {
            continue;
        };
        dismiss_import_error(conn, &path, mtime, size)?;
        dismissed_count += 1;
    }
    Ok(dismissed_count)
}

/// Reverses [`dismiss_import_error`]: nulls `dismissed_mtime`/`dismissed_
/// size` on the row at `path`, moving it back into [`query_import_errors_
/// grouped`]'s active list with its `first_seen`/`last_seen`/`seen_count`
/// episode history untouched. Deliberately does NOT re-scan the file — see
/// the module doc's "Dismiss/restore semantics" section for why an
/// immediate retry is the UI's job, not this query's. A path that isn't
/// currently dismissed (or doesn't exist at all) is a silent no-op, same
/// convention as [`dismiss_import_error`].
pub fn restore_import_error(conn: &Connection, path: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE import_errors SET dismissed_mtime = NULL, dismissed_size = NULL WHERE path = ?1",
        rusqlite::params![path],
    )?;
    Ok(())
}
