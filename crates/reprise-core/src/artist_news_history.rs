//! Persistent history for the Releases full view, plus the hard retention
//! that keeps the underlying `new_releases` table bounded.

use std::cmp::Ordering;

use chrono::{NaiveDate, TimeZone};
use rusqlite::Connection;

/// 6 months, approximated as flat 30-day months so the constant is a plain
/// number instead of a calendar-walking calculation. The count cap
/// (`HISTORY_MAX_ENTRIES`) is the tighter bound in practice for any actively
/// used library, so a few days of slack here does not matter.
const HISTORY_RETENTION_SECONDS: i64 = 6 * 30 * 24 * 60 * 60;
const HISTORY_MAX_ENTRIES: usize = 200;
/// Mirrors `NEWS_WINDOW_DAYS` in `artist_news.rs`: the fetch pipeline only
/// ever looks 90 days into the past for "new" releases. A history row whose
/// release date still falls inside that window must never be purged here —
/// otherwise the next refresh re-inserts it via `upsert_releases` and it
/// badges as new again (FB-4). Kept as its own constant (rather than
/// reaching into `artist_news`'s private one) since it documents a
/// deliberately-duplicated invariant, not a shared implementation detail.
const FETCH_WINDOW_PROTECTION_DAYS: i64 = 90;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryStatus {
    New,
    Seen,
    Hidden,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub release_group_mbid: String,
    pub artist_name: String,
    pub title: String,
    pub release_type: String,
    pub first_release_date: String,
    pub first_seen: Option<i64>,
    pub seen_at: Option<i64>,
    pub hidden: bool,
    pub hidden_at: Option<i64>,
    pub presence: crate::artist_news::LibraryPresence,
    pub announce_url: Option<String>,
}

/// One complete row from the durable New Releases history, plus its derived
/// local-library presence.
///
/// Unlike [`HistoryEntry`], this record retains every stored column so
/// headless read surfaces can expose the complete cache contract without SQL
/// outside `reprise-core`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseHistoryRecord {
    pub release_group_mbid: String,
    pub artist_name: String,
    pub artist_mbid: String,
    pub title: String,
    pub release_type: String,
    pub first_release_date: String,
    pub fetched_at: i64,
    pub seen_at: Option<i64>,
    pub hidden: bool,
    pub fallback_accent: String,
    pub first_seen: Option<i64>,
    pub hidden_at: Option<i64>,
    pub announce_url: Option<String>,
    pub presence: crate::artist_news::LibraryPresence,
}

impl ReleaseHistoryRecord {
    pub fn history_status(&self) -> HistoryStatus {
        if self.hidden {
            HistoryStatus::Hidden
        } else if self.seen_at.is_some() {
            HistoryStatus::Seen
        } else {
            HistoryStatus::New
        }
    }
}

impl HistoryEntry {
    pub fn status(&self) -> HistoryStatus {
        if self.hidden {
            HistoryStatus::Hidden
        } else if self.seen_at.is_some() {
            HistoryStatus::Seen
        } else {
            HistoryStatus::New
        }
    }
}

/// All rows ever recorded, newest first. Sorted by `first_seen` (rows
/// without one — should not happen once A3's insert path runs, but the
/// column is nullable — sort last), with `first_release_date` as the
/// tie-breaker for rows whose `first_seen` collides (e.g. a batch fetch).
/// The tie-break reuses `parse_partial_date`'s fallback-to-`today` pattern
/// from `compare_stored_releases` rather than a raw string compare.
pub fn query_history(
    conn: &Connection,
    today: NaiveDate,
) -> Result<Vec<HistoryEntry>, rusqlite::Error> {
    Ok(query_complete_history(conn, today)?
        .into_iter()
        .map(|record| HistoryEntry {
            release_group_mbid: record.release_group_mbid,
            artist_name: record.artist_name,
            title: record.title,
            release_type: record.release_type,
            first_release_date: record.first_release_date,
            first_seen: record.first_seen,
            seen_at: record.seen_at,
            hidden: record.hidden,
            hidden_at: record.hidden_at,
            presence: record.presence,
            announce_url: record.announce_url,
        })
        .collect())
}

/// Reads every durable New Releases field, including hidden history, without
/// applying the current Releases UI filters.
pub fn query_complete_history(
    conn: &Connection,
    today: NaiveDate,
) -> Result<Vec<ReleaseHistoryRecord>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT release_group_mbid, artist_name, artist_mbid, title,
                release_type, first_release_date, fetched_at, seen_at, hidden,
                fallback_accent, first_seen, hidden_at, announce_url
         FROM new_releases",
    )?;
    let mut entries = statement
        .query_map([], |row| {
            Ok(ReleaseHistoryRecord {
                release_group_mbid: row.get(0)?,
                artist_name: row.get(1)?,
                artist_mbid: row.get(2)?,
                title: row.get(3)?,
                release_type: row.get(4)?,
                first_release_date: row.get(5)?,
                fetched_at: row.get(6)?,
                seen_at: row.get(7)?,
                hidden: row.get::<_, i64>(8)? != 0,
                fallback_accent: row.get(9)?,
                first_seen: row.get(10)?,
                hidden_at: row.get(11)?,
                announce_url: row.get(12)?,
                presence: crate::artist_news::LibraryPresence::Absent,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let counts = crate::artist_news::local_album_track_counts(conn)?;
    for entry in &mut entries {
        entry.presence =
            crate::artist_news::presence_for(&counts, &entry.artist_name, &entry.title);
    }

    entries.sort_by(|left, right| {
        match (left.first_seen, right.first_seen) {
            (Some(left_seen), Some(right_seen)) => right_seen.cmp(&left_seen),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        }
        .then_with(|| {
            let left_date =
                crate::artist_news::parse_partial_date(&left.first_release_date).unwrap_or(today);
            let right_date =
                crate::artist_news::parse_partial_date(&right.first_release_date).unwrap_or(today);
            right_date.cmp(&left_date)
        })
    });

    Ok(entries)
}

/// Derives "today" in local time from a fetch-time Unix timestamp, the same
/// way `refresh_with`'s caller derives `today` for the fetch window. Shared
/// by `enforce_retention` and its tests so both reason about the exact same
/// calendar date for a given `now`.
fn local_today(now: i64) -> NaiveDate {
    chrono::Local
        .timestamp_opt(now, 0)
        .single()
        .map_or_else(|| chrono::Utc::now().date_naive(), |dt| dt.date_naive())
}

/// Hard-deletes history rows that are beyond retention, protecting anything
/// still inside the fetch window (critical: see `FETCH_WINDOW_PROTECTION_DAYS`
/// and module docs). A row is deleted when it is NOT protected AND (it is
/// older than `HISTORY_RETENTION_SECONDS` by `first_seen`, OR it falls beyond
/// the `HISTORY_MAX_ENTRIES` newest rows by `first_seen`).
pub fn enforce_retention(conn: &Connection, now: i64) -> Result<(), rusqlite::Error> {
    let today = local_today(now);
    let cutoff = now - HISTORY_RETENTION_SECONDS;
    let window_start = today - chrono::Duration::days(FETCH_WINDOW_PROTECTION_DAYS);

    let mut statement = conn
        .prepare("SELECT release_group_mbid, first_seen, first_release_date FROM new_releases")?;
    let mut rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    // Newest-first by `first_seen` (NULLs treated as oldest) determines which
    // rows survive the count cap below.
    rows.sort_by(|left, right| match (left.1, right.1) {
        (Some(left_seen), Some(right_seen)) => right_seen.cmp(&left_seen),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    });

    let mut to_delete = Vec::new();
    for (index, (mbid, first_seen, first_release_date)) in rows.into_iter().enumerate() {
        if is_protected_by_fetch_window(&first_release_date, window_start) {
            continue;
        }
        let too_old = first_seen.is_some_and(|value| value < cutoff);
        let beyond_cap = index >= HISTORY_MAX_ENTRIES;
        if too_old || beyond_cap {
            to_delete.push(mbid);
        }
    }

    if to_delete.is_empty() {
        return Ok(());
    }

    let transaction = conn.unchecked_transaction()?;
    for mbid in &to_delete {
        transaction.execute(
            "DELETE FROM new_releases WHERE release_group_mbid = ?1",
            [mbid.as_str()],
        )?;
    }
    transaction.commit()
}

/// "Still inside the fetch window" = the release date, as a real calendar
/// date, is on or after `window_start` (which already covers "in the
/// future" since any future date is >= `window_start`), OR the date could
/// not be parsed at all — an incomplete/garbage date is protected
/// conservatively rather than risk deleting a live release.
fn is_protected_by_fetch_window(first_release_date: &str, window_start: NaiveDate) -> bool {
    match crate::artist_news::parse_partial_date(first_release_date) {
        Some(date) => date >= window_start,
        None => true,
    }
}

/// Un-hides exactly one release. Reuses `set_release_hidden`'s `hidden = 0`
/// path (which also nulls `hidden_at`) so there is a single place that
/// defines what "un-hidden" means. The Releases full view calls this for
/// its per-row "Show again" action.
pub fn restore_release(conn: &Connection, release_group_mbid: &str) -> Result<(), rusqlite::Error> {
    crate::artist_news::set_release_hidden(conn, release_group_mbid, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn migrated_conn() -> Connection {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn
    }

    fn insert_history_row(
        conn: &Connection,
        mbid: &str,
        first_seen: i64,
        first_release_date: &str,
    ) {
        conn.execute(
            "INSERT INTO new_releases (
               release_group_mbid, artist_name, artist_mbid, title, release_type,
               first_release_date, fetched_at, fallback_accent, first_seen
             ) VALUES (?1, 'Artist', 'artist-mbid', 'Title', 'Album', ?2, ?3, '#123456', ?3)",
            rusqlite::params![mbid, first_release_date, first_seen],
        )
        .unwrap();
    }

    fn release_exists(conn: &Connection, mbid: &str) -> bool {
        conn.query_row(
            "SELECT COUNT(*) FROM new_releases WHERE release_group_mbid = ?1",
            [mbid],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
            > 0
    }

    #[test]
    fn nr_12a_restore_returns_a_single_hidden_entry() {
        let conn = migrated_conn();
        insert_history_row(&conn, "one", 1_000, "2026-01-01");
        insert_history_row(&conn, "two", 1_000, "2026-01-01");
        crate::artist_news::set_release_hidden(&conn, "one", true).unwrap();
        crate::artist_news::set_release_hidden(&conn, "two", true).unwrap();

        restore_release(&conn, "one").unwrap();

        let today = NaiveDate::from_ymd_opt(2026, 7, 21).unwrap();
        let entries = query_history(&conn, today).unwrap();
        let one = entries
            .iter()
            .find(|entry| entry.release_group_mbid == "one")
            .unwrap();
        let two = entries
            .iter()
            .find(|entry| entry.release_group_mbid == "two")
            .unwrap();
        assert!(!one.hidden, "restored entry is visible again");
        assert!(one.hidden_at.is_none(), "restoring clears hidden_at too");
        assert!(two.hidden, "the other hidden entry is untouched");
        assert!(two.hidden_at.is_some());
    }

    #[test]
    fn retention_deletes_the_201st_newest_entry_and_keeps_the_cap() {
        let conn = migrated_conn();
        let now = 1_752_000_000_i64;
        // Well within the retention window either way, and far outside the
        // 90-day fetch-protection window, so only the count cap is at play.
        for i in 1..=201_i64 {
            insert_history_row(&conn, &format!("row-{i}"), now - i * 10, "2000-01-01");
        }

        enforce_retention(&conn, now).unwrap();

        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM new_releases", [], |row| row.get(0))
            .unwrap();
        assert_eq!(remaining, 200);
        assert!(
            !release_exists(&conn, "row-201"),
            "the 201st newest is purged"
        );
        assert!(
            release_exists(&conn, "row-200"),
            "the 200th newest survives"
        );
        assert!(release_exists(&conn, "row-1"), "the newest survives");
    }

    #[test]
    fn retention_deletes_entries_older_than_six_months_but_keeps_newer_ones() {
        let conn = migrated_conn();
        let now = 1_752_000_000_i64;
        let cutoff = now - HISTORY_RETENTION_SECONDS;
        // Both rows share an ancient, well-outside-the-window release date so
        // only the `first_seen` age criterion can explain the outcome.
        insert_history_row(&conn, "just-too-old", cutoff - 1, "2000-01-01");
        insert_history_row(&conn, "just-young-enough", cutoff + 1, "2000-01-01");

        enforce_retention(&conn, now).unwrap();

        assert!(!release_exists(&conn, "just-too-old"));
        assert!(release_exists(&conn, "just-young-enough"));
    }

    #[test]
    fn retention_protects_entries_still_inside_the_ninety_day_fetch_window() {
        let conn = migrated_conn();
        let now = 1_752_000_000_i64;
        let today = local_today(now);
        let ancient_first_seen = now - HISTORY_RETENTION_SECONDS - 1;
        let inside_window = (today - chrono::Duration::days(89))
            .format("%Y-%m-%d")
            .to_string();
        let outside_window = (today - chrono::Duration::days(91))
            .format("%Y-%m-%d")
            .to_string();
        insert_history_row(&conn, "inside-window", ancient_first_seen, &inside_window);
        insert_history_row(&conn, "outside-window", ancient_first_seen, &outside_window);

        enforce_retention(&conn, now).unwrap();

        assert!(
            release_exists(&conn, "inside-window"),
            "a release whose date is still within the 90-day fetch window is never purged, \
             even though its first_seen is old — a re-fetch would just re-insert and re-badge it"
        );
        assert!(
            !release_exists(&conn, "outside-window"),
            "control: the same old first_seen with a release date safely outside the window is purged"
        );
    }

    #[test]
    fn retention_protects_entries_with_future_or_unparsable_release_dates() {
        let conn = migrated_conn();
        let now = 1_752_000_000_i64;
        let today = local_today(now);
        let ancient_first_seen = now - HISTORY_RETENTION_SECONDS - 1;
        let future_date = (today + chrono::Duration::days(200))
            .format("%Y-%m-%d")
            .to_string();
        insert_history_row(&conn, "future", ancient_first_seen, &future_date);
        insert_history_row(&conn, "unparsable", ancient_first_seen, "not-a-date");
        insert_history_row(&conn, "stale-control", ancient_first_seen, "2000-01-01");

        enforce_retention(&conn, now).unwrap();

        assert!(
            release_exists(&conn, "future"),
            "a future release date is protected"
        );
        assert!(
            release_exists(&conn, "unparsable"),
            "an unparsable date is protected conservatively"
        );
        assert!(
            !release_exists(&conn, "stale-control"),
            "control: an old, parseable, out-of-window date is still purged"
        );
    }
}
