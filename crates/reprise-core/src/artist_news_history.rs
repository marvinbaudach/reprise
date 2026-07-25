//! Persistent history of every New Releases entry ever shown, plus the hard
//! retention that keeps the underlying `new_releases` table bounded.
//!
//! This is the data layer behind the popover history sub-page (NR-12,
//! `[aktiv]` in `docs/ux-rules.md`; the UI lives in
//! `crates/reprise-gnome/src/ui/new_releases/history_page.rs`). It reads
//! the same table `artist_news.rs` writes and reuses that module's
//! date-parsing and hide/show primitives rather than re-deriving them.

use std::cmp::Ordering;

use chrono::{Datelike, NaiveDate, TimeZone};
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryGroup {
    pub label: String,
    pub entries: Vec<HistoryEntry>,
}

const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

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
    let mut statement = conn.prepare(
        "SELECT release_group_mbid, artist_name, title, release_type,
                first_release_date, first_seen, seen_at, hidden, hidden_at,
                announce_url
         FROM new_releases",
    )?;
    let mut entries = statement
        .query_map([], |row| {
            Ok(HistoryEntry {
                release_group_mbid: row.get(0)?,
                artist_name: row.get(1)?,
                title: row.get(2)?,
                release_type: row.get(3)?,
                first_release_date: row.get(4)?,
                first_seen: row.get(5)?,
                seen_at: row.get(6)?,
                hidden: row.get::<_, i64>(7)? != 0,
                hidden_at: row.get(8)?,
                announce_url: row.get(9)?,
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

/// Pure grouping of already-sorted (newest first) history entries into
/// time-based buckets for the popover sub-page. Entries whose `first_seen`
/// is `None` cannot be placed on the timeline at all, so — rather than
/// guessing a date for them — they are collected into one trailing group
/// labeled "Earlier", after every dated group.
pub fn group_history(entries: Vec<HistoryEntry>, today: NaiveDate) -> Vec<HistoryGroup> {
    let this_week = today.iso_week();
    let mut groups: Vec<HistoryGroup> = Vec::new();
    let mut undated: Vec<HistoryEntry> = Vec::new();

    for entry in entries {
        let Some(local_date) = local_date_of_first_seen(entry.first_seen) else {
            undated.push(entry);
            continue;
        };
        let label = history_group_label(local_date, today, this_week);
        match groups.last_mut() {
            Some(group) if group.label == label => group.entries.push(entry),
            _ => groups.push(HistoryGroup {
                label,
                entries: vec![entry],
            }),
        }
    }

    if !undated.is_empty() {
        groups.push(HistoryGroup {
            label: "Earlier".to_string(),
            entries: undated,
        });
    }

    groups
}

fn local_date_of_first_seen(first_seen: Option<i64>) -> Option<NaiveDate> {
    let timestamp = first_seen?;
    Some(
        chrono::Local
            .timestamp_opt(timestamp, 0)
            .single()
            .map_or_else(|| chrono::Utc::now().date_naive(), |dt| dt.date_naive()),
    )
}

fn history_group_label(date: NaiveDate, today: NaiveDate, this_week: chrono::IsoWeek) -> String {
    let date_week = date.iso_week();
    if date_week.year() == this_week.year() && date_week.week() == this_week.week() {
        return "This week".to_string();
    }
    let month_name = MONTH_NAMES[date.month0() as usize];
    if date.year() == today.year() {
        month_name.to_string()
    } else {
        format!("{month_name} {}", date.year())
    }
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
/// defines what "un-hidden" means. Replaces the former blanket
/// `show_hidden_releases` (removed; see the history sub-page's Restore
/// action in `history_page.rs`, its only real caller).
pub fn restore_release(conn: &Connection, release_group_mbid: &str) -> Result<(), rusqlite::Error> {
    crate::artist_news::set_release_hidden(conn, release_group_mbid, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Weekday;

    fn migrated_conn() -> Connection {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn
    }

    fn history_entry(mbid: &str, first_seen: Option<i64>) -> HistoryEntry {
        HistoryEntry {
            release_group_mbid: mbid.to_string(),
            artist_name: "Artist".to_string(),
            title: "Title".to_string(),
            release_type: "Album".to_string(),
            first_release_date: "2026-01-01".to_string(),
            first_seen,
            seen_at: None,
            hidden: false,
            hidden_at: None,
            presence: crate::artist_news::LibraryPresence::Absent,
            announce_url: None,
        }
    }

    /// Round-trips a calendar date through the same `chrono::Local` zone
    /// `local_date_of_first_seen` uses, at noon to stay well clear of any
    /// DST transition — so this is stable regardless of the test runner's
    /// timezone.
    fn local_timestamp(date: NaiveDate) -> i64 {
        date.and_hms_opt(12, 0, 0)
            .unwrap()
            .and_local_timezone(chrono::Local)
            .single()
            .unwrap()
            .timestamp()
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
    fn nr_12_history_groups_by_week_and_month() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 21).unwrap();
        let monday_this_week = NaiveDate::from_isoywd_opt(
            today.iso_week().year(),
            today.iso_week().week(),
            Weekday::Mon,
        )
        .unwrap();
        let last_month = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        let last_year = NaiveDate::from_ymd_opt(2025, 5, 10).unwrap();

        let entries = vec![
            history_entry("this-week-new", Some(local_timestamp(today))),
            history_entry("this-week-old", Some(local_timestamp(monday_this_week))),
            history_entry("last-month", Some(local_timestamp(last_month))),
            history_entry("last-year", Some(local_timestamp(last_year))),
        ];

        let groups = group_history(entries, today);

        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].label, "This week");
        assert_eq!(
            groups[0]
                .entries
                .iter()
                .map(|entry| entry.release_group_mbid.as_str())
                .collect::<Vec<_>>(),
            ["this-week-new", "this-week-old"],
            "entries within a group keep the order they arrived in"
        );
        assert_eq!(groups[1].label, "June");
        assert_eq!(groups[1].entries[0].release_group_mbid, "last-month");
        assert_eq!(groups[2].label, "May 2025");
        assert_eq!(groups[2].entries[0].release_group_mbid, "last-year");
    }

    #[test]
    fn nr_12_undated_entries_land_in_a_trailing_earlier_group() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 21).unwrap();
        let entries = vec![
            history_entry("dated", Some(local_timestamp(today))),
            history_entry("undated", None),
        ];

        let groups = group_history(entries, today);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].label, "This week");
        assert_eq!(groups[1].label, "Earlier");
        assert_eq!(groups[1].entries[0].release_group_mbid, "undated");
    }

    #[test]
    fn nr_12_restore_returns_a_single_hidden_entry() {
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
