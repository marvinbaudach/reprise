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
