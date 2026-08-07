//! Full-page Releases filtering, status, and sorting.
//!
//! The UI consumes this module through the `artist_news` facade. Keeping the
//! decisions here makes sidebar counts and the visible table share exactly
//! one definition.

use std::cmp::Ordering;

use chrono::NaiveDate;
use rusqlite::Connection;

use crate::artist_news::{normalize, parse_partial_date, LibraryPresence};
use crate::artist_news_history::{query_history_in, HistoryEntry};
use crate::artist_news_scope::{
    catalog_type, collapse_duplicates, counts_as_owned, ReleaseTypeSelection, ReleaseWindow,
    ReleasesFilter, ScopedRelease, RELEASES_FILTER_HIDDEN_KEY, RELEASES_FILTER_TYPE_KEY,
    RELEASES_FILTER_WINDOW_KEY,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseStatus {
    InLibrary,
    Upcoming,
    Incomplete,
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseSortDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleasesViewResult {
    pub rows: Vec<HistoryEntry>,
    pub widest_total: usize,
}

impl ScopedRelease for HistoryEntry {
    fn artist_name(&self) -> &str {
        &self.artist_name
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn first_release_date(&self) -> &str {
        &self.first_release_date
    }

    fn release_type(&self) -> &str {
        &self.release_type
    }

    fn track_count(&self) -> Option<i64> {
        self.track_count
    }

    fn release_group_mbid(&self) -> &str {
        &self.release_group_mbid
    }
}

pub fn persisted_releases_filter(db: &crate::db::Db) -> Result<ReleasesFilter, rusqlite::Error> {
    let conn = db.conn();
    Ok(ReleasesFilter {
        release_types: crate::library::settings::get_setting_in(conn, RELEASES_FILTER_TYPE_KEY)?
            .map_or_else(ReleaseTypeSelection::default, |value| {
                ReleaseTypeSelection::parse(&value).unwrap_or_default()
            }),
        window: crate::library::settings::get_setting_in(conn, RELEASES_FILTER_WINDOW_KEY)?
            .as_deref()
            .and_then(ReleaseWindow::parse)
            .unwrap_or_default(),
        hidden: crate::library::settings::get_bool_in(conn, RELEASES_FILTER_HIDDEN_KEY, false)?,
    })
}

pub fn release_status(entry: &HistoryEntry, today: NaiveDate) -> ReleaseStatus {
    if entry.presence == LibraryPresence::Complete {
        return ReleaseStatus::InLibrary;
    }
    if parse_partial_date(&entry.first_release_date).is_some_and(|date| date > today) {
        ReleaseStatus::Upcoming
    } else if entry.presence == LibraryPresence::Partial {
        ReleaseStatus::Incomplete
    } else {
        ReleaseStatus::Missing
    }
}

pub fn filter_rows(
    rows: Vec<HistoryEntry>,
    filter: &ReleasesFilter,
    today: NaiveDate,
) -> Vec<HistoryEntry> {
    let cutoff = filter.window.cutoff(today);
    let rows = rows
        .into_iter()
        .filter(|entry| entry.hidden == filter.hidden)
        .filter(|entry| {
            !counts_as_owned(
                entry.presence,
                &entry.release_type,
                &entry.first_release_date,
                entry.track_count,
                entry.local_track_count,
                today,
            )
        })
        .filter(|entry| catalog_type(&entry.release_type))
        .filter(|entry| filter.release_types.includes(&entry.release_type))
        .filter(|entry| {
            cutoff.is_none_or(|cutoff| {
                parse_partial_date(&entry.first_release_date).is_none_or(|date| date >= cutoff)
            })
        })
        .collect();
    collapse_duplicates(rows)
}

pub fn sort_rows(
    mut rows: Vec<HistoryEntry>,
    direction: ReleaseSortDirection,
) -> Vec<HistoryEntry> {
    rows.sort_by(|left, right| {
        compare_release_dates(left, right, direction).then_with(|| {
            left.title
                .to_ascii_lowercase()
                .cmp(&right.title.to_ascii_lowercase())
                .then_with(|| left.title.cmp(&right.title))
        })
    });
    rows
}

fn compare_release_dates(
    left: &HistoryEntry,
    right: &HistoryEntry,
    direction: ReleaseSortDirection,
) -> Ordering {
    let left_date = parse_partial_date(&left.first_release_date);
    let right_date = parse_partial_date(&right.first_release_date);
    match (left_date, right_date) {
        (Some(left), Some(right)) => match direction {
            ReleaseSortDirection::Ascending => left.cmp(&right),
            ReleaseSortDirection::Descending => right.cmp(&left),
        },
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

pub fn query_releases_view(
    db: &crate::db::Db,
    filter: &ReleasesFilter,
    today: NaiveDate,
) -> Result<Vec<HistoryEntry>, rusqlite::Error> {
    Ok(query_releases_view_scope(db, filter, today)?.rows)
}

pub fn query_releases_view_scope(
    db: &crate::db::Db,
    filter: &ReleasesFilter,
    today: NaiveDate,
) -> Result<ReleasesViewResult, rusqlite::Error> {
    let conn = db.conn();
    query_releases_view_scope_in(conn, filter, today)
}

fn query_releases_view_scope_in(
    conn: &Connection,
    filter: &ReleasesFilter,
    today: NaiveDate,
) -> Result<ReleasesViewResult, rusqlite::Error> {
    let artists = current_library_artist_keys(conn)?;
    let rows = query_history_in(conn, today)?
        .into_iter()
        .filter(|entry| artists.contains(&normalize(&entry.artist_name)))
        .collect::<Vec<_>>();
    let widest_total =
        filter_rows(rows.clone(), &ReleasesFilter::widest(filter.hidden), today).len();
    let rows = sort_rows(
        filter_rows(rows, filter, today),
        ReleaseSortDirection::Descending,
    );
    debug_assert!(rows.len() <= widest_total);
    Ok(ReleasesViewResult { rows, widest_total })
}

fn current_library_artist_keys(
    conn: &Connection,
) -> Result<std::collections::HashSet<String>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT artist, album_artist
         FROM tracks
         WHERE removed_at IS NULL AND missing_since IS NULL",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut artists = std::collections::HashSet::new();
    for row in rows {
        let (artist, album_artist) = row?;
        for name in [artist, album_artist] {
            if !name.trim().is_empty() {
                artists.insert(normalize(&name));
            }
        }
    }
    Ok(artists)
}

pub fn count_releases_view(
    db: &crate::db::Db,
    filter: &ReleasesFilter,
    today: NaiveDate,
) -> Result<i64, rusqlite::Error> {
    let conn = db.conn();
    Ok(query_releases_view_scope_in(conn, filter, today)?
        .rows
        .len() as i64)
}
