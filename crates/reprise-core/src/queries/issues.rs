//! Missing-file group queries for the 18a "self-healing" card list (Task
//! 2.1). The Missing source used to be one flat, ungrouped list; these two
//! queries are what let the GUI draw it as separate cards instead — one per
//! unavailable drive, one for rows with no evidence either way, and one for
//! rows confirmed deleted. Split into its own sibling file rather than
//! folded into `maintenance.rs` — that file is already close to the
//! project's 800-line rule with its own tests, and this is a cohesive unit
//! on its own (grouping/paginating one `missing_reason` taxonomy), not a
//! grab-bag of unrelated maintenance helpers.
//!
//! ## The three group kinds, and why `unknown` never joins `Deleted`
//!
//! `tracks.missing_reason` (schema v10/v11, see `models::MissingReason`) has
//! three values, and each maps to exactly one card:
//!
//! - `unmounted` rows are grouped by `mount_point` — the scanner records
//!   `mount_point` alongside `device` on every successful `stat` (`library::
//!   scanner`), so a row classified `Unmounted` (`library::mounts::
//!   classify_missing`) always carries the mount it was last seen under.
//!   `N` distinct mount points among these rows becomes `N` separate
//!   [`MissingGroup`]s — never one card mixing tracks from two different
//!   drives, since "the drive is plugged back in" is a per-mount event.
//! - `unknown` rows — the v10 migration's backfill for pre-v2 rows that
//!   predate the `device` column, with no way to tell "deleted" from
//!   "unmounted" apart — get their own group, `MissingGroupKind::
//!   Unavailable { mount_point: None }`. It shares the `Unavailable` kind
//!   (both are "wait and see", not "act now") but carries no mount to wait
//!   on, which is exactly why the GUI card for this group must say "will be
//!   verified on next scan" rather than the per-mount card's "returns
//!   automatically when the drive is mounted" — this group can't honestly
//!   promise that.
//! - `deleted` rows — confirmed gone from a reachable filesystem — are the
//!   ONLY rows in `MissingGroupKind::Deleted`. This distinction is load-
//!   bearing: the Deleted card's bulk action hard-deletes library rows
//!   (ratings, play history, playlist membership — all gone via `ON DELETE
//!   CASCADE`), so a row this crate cannot actually prove is deleted must
//!   never be swept into that action by being miscounted here. `query_
//!   missing_groups` filters on `missing_reason = 'deleted'` alone for this
//!   group — never a catch-all "everything that isn't unmounted".
//!
//! A group with zero matching rows is simply absent from the returned
//! `Vec` — there is no "empty card" concept; the GUI shows exactly the
//! cards this function returns, in the fixed order above (unmounted groups,
//! each sorted by `mount_point`, then unknown, then deleted).

use rusqlite::Connection;

use crate::library::settings::{self, AutoCleanSetting};
use crate::models::{MissingReason, Track};

use super::clauses::{row_to_track, MISSING};

/// Which card a [`MissingGroup`] represents. See the module doc for why
/// `Unavailable { mount_point: None }` (the `unknown` reason) is kept
/// distinct from every per-mount `Unavailable` group and from `Deleted`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissingGroupKind {
    /// A wait-state card: the file may well still exist, we just can't see
    /// it right now. `Some(mount_point)` for a confirmed-unmounted drive
    /// (`missing_reason = 'unmounted'`); `None` for the `unknown` reason —
    /// no mount to report, because no mount was ever recorded for these
    /// rows.
    Unavailable { mount_point: Option<String> },
    /// An actionable card: `missing_reason = 'deleted'` rows only — see the
    /// module doc's "why `unknown` never joins `Deleted`" section.
    Deleted,
}

/// One card's worth of aggregate state: which kind, and how many tracks it
/// covers. Deliberately just a count, not the rows themselves — the GUI
/// only needs `track_count` to render a card's header ("N tracks") and
/// decide whether to render it at all; the rows are a separate, paginated
/// fetch via [`query_missing_rows`] once a card is expanded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingGroup {
    pub kind: MissingGroupKind,
    pub track_count: u32,
}

/// The full projection [`query_missing_rows`] needs — identical 22-column
/// shape to `clauses::row_to_track`'s expectations (see that function's own
/// doc comment), kept as one `const` so the three branches of `query_
/// missing_rows` can never drift apart on which columns they select.
const MISSING_ROWS_SELECT: &str = "SELECT id, path, title, artist, album, album_artist, year, \
     track_no, genre, duration_ms, bitrate_kbps, rating, play_count, last_played_at, added_at, \
     file_mtime, missing_since, missing_reason, untagged, file_size, device, inode FROM tracks";

/// Returns every non-empty missing-file card, in the fixed 18a order: one
/// `Unavailable { mount_point: Some(_) }` group per distinct mount point
/// among `unmounted` rows (sorted by mount point, case-insensitively), then
/// the single `Unavailable { mount_point: None }` group for `unknown` rows
/// if any exist, then the single `Deleted` group if any exist. See the
/// module doc for why `unknown` and `deleted` can never be merged.
pub fn query_missing_groups(conn: &Connection) -> Result<Vec<MissingGroup>, rusqlite::Error> {
    let mut groups = query_unavailable_groups(conn)?;
    if let Some(unknown) = query_reason_count_group(
        conn,
        MissingReason::Unknown,
        MissingGroupKind::Unavailable { mount_point: None },
    )? {
        groups.push(unknown);
    }
    if let Some(deleted) =
        query_reason_count_group(conn, MissingReason::Deleted, MissingGroupKind::Deleted)?
    {
        groups.push(deleted);
    }
    Ok(groups)
}

/// The per-mount half of [`query_missing_groups`]: one row per distinct
/// `mount_point` among `unmounted` rows, via `GROUP BY` — a mount with zero
/// matching rows simply never appears as a group row, so no separate empty-
/// group filtering is needed here (unlike the single-count `unknown`/
/// `deleted` groups, which need an explicit `count > 0` check since a plain
/// `COUNT(*)` always returns one row, even when it's zero).
fn query_unavailable_groups(conn: &Connection) -> Result<Vec<MissingGroup>, rusqlite::Error> {
    let mut stmt = conn.prepare(&format!(
        "SELECT mount_point, count(*) FROM tracks WHERE {MISSING} AND missing_reason = ?1 \
         GROUP BY mount_point ORDER BY mount_point COLLATE NOCASE"
    ))?;
    let rows = stmt.query_map(rusqlite::params![MissingReason::Unmounted.as_str()], |r| {
        Ok((r.get::<_, Option<String>>(0)?, r.get::<_, i64>(1)?))
    })?;
    rows.map(|row| {
        let (mount_point, count) = row?;
        Ok(MissingGroup {
            kind: MissingGroupKind::Unavailable { mount_point },
            track_count: count as u32,
        })
    })
    .collect()
}

/// Counts `MISSING` rows matching one exact `reason`, returning `None`
/// rather than a zero-count `Some` when there are none — the caller only
/// ever wants to push a group that actually has tracks in it (see the
/// module doc: "a group with zero matching rows is simply absent").
fn query_reason_count_group(
    conn: &Connection,
    reason: MissingReason,
    kind: MissingGroupKind,
) -> Result<Option<MissingGroup>, rusqlite::Error> {
    let count: i64 = conn.query_row(
        &format!("SELECT count(*) FROM tracks WHERE {MISSING} AND missing_reason = ?1"),
        rusqlite::params![reason.as_str()],
        |r| r.get(0),
    )?;
    Ok((count > 0).then_some(MissingGroup {
        kind,
        track_count: count as u32,
    }))
}

/// Returns one page of `kind`'s tracks, ordered `artist, album, track_no`
/// (all `COLLATE NOCASE` on the text columns) — the same shape `clauses::
/// SORT_WHITELIST`'s `"artist"` entry sorts by, minus the `year` component
/// that entry adds between artist and album; a missing-file card has no use
/// for a year-based tiebreak the way the main library view's artist sort
/// does; `id` is close enough. `offset`/`limit` are plain `LIMIT`/`OFFSET`
/// values — the caller (a card's "load more" / scroll-to-fetch) is
/// responsible for paging through a card's `track_count`.
///
/// `kind` decides both the `missing_reason` filter and, for a specific
/// mount, an additional `mount_point` filter — see [`MissingGroupKind`]'s
/// doc comment for how each variant maps to a `missing_reason` value.
pub fn query_missing_rows(
    conn: &Connection,
    kind: &MissingGroupKind,
    offset: u32,
    limit: u32,
) -> Result<Vec<Track>, rusqlite::Error> {
    let (reason, mount_point) = match kind {
        MissingGroupKind::Deleted => (MissingReason::Deleted, None),
        MissingGroupKind::Unavailable {
            mount_point: Some(mount_point),
        } => (MissingReason::Unmounted, Some(mount_point.as_str())),
        MissingGroupKind::Unavailable { mount_point: None } => (MissingReason::Unknown, None),
    };
    let mount_filter = if mount_point.is_some() {
        " AND mount_point = ?4"
    } else {
        ""
    };
    let sql = format!(
        "{MISSING_ROWS_SELECT} WHERE {MISSING} AND missing_reason = ?3{mount_filter} \
         ORDER BY artist COLLATE NOCASE, album COLLATE NOCASE, track_no LIMIT ?1 OFFSET ?2"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = match mount_point {
        Some(mount_point) => stmt.query_map(
            rusqlite::params![limit, offset, reason.as_str(), mount_point],
            row_to_track,
        )?,
        None => stmt.query_map(
            rusqlite::params![limit, offset, reason.as_str()],
            row_to_track,
        )?,
    };
    rows.collect()
}

// -- Auto-clean (Task 2.3) --------------------------------------------------
//
// The Deleted card (this module's `MissingGroupKind::Deleted`) is a self-
// healing state list, not a permanent parking lot — once a track has
// definitely been deleted, and the user has opted into automatic cleanup,
// there is no more evidence coming that will ever change that verdict.
// `auto_clean_eligible`/`run_auto_clean` are the opt-in, unattended half of
// that self-healing story: after a user-chosen grace period, a `deleted`
// row's library history (rating, play count, playlist membership, listen
// history — all cascade-deleted with it) stops being worth holding onto
// against a file that is provably never coming back.
//
// This is the one place in the crate where a background process is allowed
// to hard-delete `tracks` rows with nobody watching, so every fail-safe
// below is deliberate and non-negotiable — see each function's doc comment.

/// Seconds in one day — the unit `AutoCleanSetting::Days` counts in.
const SECONDS_PER_DAY: i64 = 86_400;

/// Every `MissingGroupKind::Deleted` track id whose auto-clean grace period
/// has elapsed as of `now` (unix seconds) — the read-only half of Task 2.3's
/// unattended cleanup. [`run_auto_clean`] is the only real caller; this
/// function is exposed separately so a preview ("N tracks would be removed")
/// can be shown without deleting anything.
///
/// Three independent fail-safes, each "return empty" rather than an error:
/// - The setting is [`AutoCleanSetting::Off`] — its default, including for a
///   never-written or corrupt `missing_auto_clean` value (see that type's
///   doc comment).
/// - Auto-clean has never been armed (`auto_clean_armed_at` unset) — a
///   duration alone must never run; see `settings::get_auto_clean_armed_at`'s
///   doc comment for why arming is the safety catch against an instant mass
///   deletion the moment a user turns the setting on over an existing
///   backlog of old missing rows.
/// - No `deleted` row's deadline has actually passed yet.
///
/// A row only ever qualifies via `missing_reason = 'deleted'` (the shared
/// [`MISSING`] predicate plus this exact reason, mirroring `query_missing_
/// groups`'s own `Deleted` filter above) — `unmounted`/`unknown` rows are
/// NEVER eligible, no matter how long they've sat missing: an unmounted
/// drive's files are almost certainly fine and will return the moment the
/// drive is remounted, and an `unknown` row (the v10 migration's backfill
/// for pre-v2 rows, predating the `device` column) carries no evidence at
/// all — auto-clean only ever acts on rows this crate can actually prove
/// are gone.
///
/// The deadline itself is `max(missing_since, armed_at) + days*86400 <=
/// now`: the grace period starts at whichever is LATER, the file going
/// missing or the feature being armed — never purely `missing_since` alone,
/// which is exactly what would let arming the setting over months-old
/// missing rows delete them the instant it's turned on. See `settings::
/// set_auto_clean_armed_at`'s doc comment for the "start counting from
/// today" flow this protects.
pub fn auto_clean_eligible(conn: &Connection, now: i64) -> Result<Vec<i64>, rusqlite::Error> {
    let AutoCleanSetting::Days(days) = settings::get_missing_auto_clean(conn) else {
        return Ok(Vec::new());
    };
    let Some(armed_at) = settings::get_auto_clean_armed_at(conn)? else {
        return Ok(Vec::new());
    };
    let grace_period_seconds = i64::from(days) * SECONDS_PER_DAY;
    let mut statement = conn.prepare(&format!(
        "SELECT id FROM tracks WHERE {MISSING} AND missing_reason = ?1 \
         AND max(missing_since, ?2) + ?3 <= ?4 ORDER BY id"
    ))?;
    let ids = statement
        .query_map(
            rusqlite::params![
                MissingReason::Deleted.as_str(),
                armed_at,
                grace_period_seconds,
                now
            ],
            |row| row.get(0),
        )?
        .collect::<Result<_, _>>()?;
    Ok(ids)
}

/// Runs Task 2.3's unattended cleanup: every id [`auto_clean_eligible`]
/// returns is hard-deleted via `maintenance::remove_auto_clean_eligible_
/// tracks` — the same transactional, playlist-position-compacting delete
/// path every other real removal in this crate funnels through, guarded by
/// `RemoveGuard::AutoCleanEligible` so a resurrection racing the delete
/// itself can never be swept away — returning the exact ids the caller must
/// purge from its own in-memory playback queue.
///
/// Finding 1 (review pass): this function's own call to [`auto_clean_
/// eligible`] and the per-id `DELETE` inside `remove_auto_clean_eligible_
/// tracks` are not one atomic transaction, so the scanner/watcher — its own
/// OS thread, its own `rusqlite::Connection`, a genuine concurrent writer
/// under this database's WAL mode, not a hypothetical (see `maintenance.rs`'s
/// tombstone section header comment for the same race on the other
/// deletion path) — can resurrect a selected id in the window between that
/// `SELECT` and this loop reaching that id's `DELETE`: the file reappeared,
/// so the row is legitimately live again. `remove_auto_clean_eligible_
/// tracks` re-checks eligibility (still missing, still `missing_reason =
/// 'deleted'`) at delete time instead of trusting this snapshot, so a
/// resurrected row survives with its rating, playlist membership and
/// listening history intact rather than being hard-deleted out from under a
/// track the scanner just proved is live again. The guard does NOT re-run
/// the `days`/`armed_at` deadline arithmetic — time only moves forward, so
/// an id already past its deadline at selection time is still past it at
/// delete time; only the missing state and reason can realistically change
/// under the race, so only those are re-checked (see `maintenance::
/// remove_auto_clean_eligible_tracks`'s doc comment). See `tests_auto_
/// clean.rs`'s `run_auto_clean_survives_a_resurrection_racing_the_delete_
/// itself` for the regression test this guards against.
///
/// Deliberately NOT routed through the tombstone/10-second-undo mechanism
/// (`maintenance::tombstone_tracks`) despite deleting the same shape of row
/// a "Remove all" flow does: that mechanism exists so a user who clicks
/// "Remove" gets a toast they can act on immediately. Auto-clean fires
/// unattended, at least `days` days after the fact, with nobody watching a
/// toast — a tombstone with no observer to click "Undo" is just a second,
/// silent grace period bolted onto the one that already elapsed, not a real
/// safety net. The one real safety net this feature has is [`auto_clean_
/// eligible`]'s own three fail-safes (see its doc comment): get the deadline
/// right before deleting, because there is no undo once this function has
/// run — the whole point of a hard delete, made deliberately.
pub fn run_auto_clean(conn: &mut Connection, now: i64) -> Result<Vec<i64>, rusqlite::Error> {
    let ids = auto_clean_eligible(conn, now)?;
    super::maintenance::remove_auto_clean_eligible_tracks(conn, &ids)
}

// -- Badge counts (Task 2.5) -------------------------------------------------
//
// The sidebar's ISSUES section needs two different questions answered about
// the Missing-files state, and conflating them is exactly the Rhythmbox
// failure mode this rebuild exists to avoid: a badge that always shows the
// full backlog trains the user to ignore it, because it never goes away no
// matter how much they've already looked at. So the two are kept as two
// separate functions with two separate contracts:
//
// - [`count_missing`] answers "does the Missing files row exist at all?" —
//   the full `MISSING` total, unconditional on anything the user has seen.
//   Zero here means the row (and the ISSUES section itself, if this and the
//   import-errors sibling are both zero) simply isn't shown.
// - [`count_new_missing`] answers "what's the badge number?" — only rows
//   that went missing strictly after `last_viewed`, a unix-seconds
//   timestamp the caller reads via `library::settings::get_last_viewed_
//   missing` and passes in explicitly (kept a parameter, not read inside,
//   for the same testability reason `auto_clean_eligible`'s `now: i64`
//   parameter above is). Opening the Missing files view calls `library::
//   settings::set_last_viewed_missing(conn, now)`, which clears this count
//   back to whatever goes missing next — the total itself never moves, it
//   just stops being new.

/// Total count of `MISSING` tracks (`missing_since IS NOT NULL AND
/// removed_at IS NULL`), across every `missing_reason` — see this section's
/// header comment for why this is a different question from [`count_new_
/// missing`]. Tombstoned rows (`removed_at` set) are excluded by [`MISSING`]
/// itself: the user already asked for those to be gone, so they can't be
/// what makes the sidebar row exist.
pub fn count_missing(conn: &Connection) -> Result<u32, rusqlite::Error> {
    let count: i64 = conn.query_row(
        &format!("SELECT count(*) FROM tracks WHERE {MISSING}"),
        [],
        |r| r.get(0),
    )?;
    Ok(count.max(0) as u32)
}

/// The Missing-files sidebar badge: `MISSING` tracks whose `missing_since`
/// is strictly AFTER `last_viewed` — see this section's header comment for
/// why "strictly after", not the full total, is the badge's definition.
/// `missing_since > last_viewed` already implies `missing_since IS NOT
/// NULL` on its own (SQLite's `>` against `NULL` is never true), but this
/// still composes the shared [`MISSING`] predicate rather than relying on
/// that implication implicitly — the `removed_at IS NULL` half has no such
/// free ride, and a reader should never have to re-derive "why is this
/// query safe" from a comparison operator's NULL semantics.
///
/// Boundary is exclusive (`>`, not `>=`) on purpose: a row whose
/// `missing_since` equals `last_viewed` exactly went missing in the very
/// scan/second the user's last view already covered, so it must not
/// re-badge as new.
pub fn count_new_missing(conn: &Connection, last_viewed: i64) -> Result<u32, rusqlite::Error> {
    let count: i64 = conn.query_row(
        &format!("SELECT count(*) FROM tracks WHERE {MISSING} AND missing_since > ?1"),
        rusqlite::params![last_viewed],
        |r| r.get(0),
    )?;
    Ok(count.max(0) as u32)
}
