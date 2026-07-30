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

pub const RELEASES_FILTER_TYPE_KEY: &str = "releases.filter.type";
pub const RELEASES_FILTER_HIDDEN_KEY: &str = "releases.filter.hidden";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseTypeFilter {
    Album,
    Ep,
}

impl ReleaseTypeFilter {
    pub const fn setting_value(self) -> &'static str {
        match self {
            Self::Album => "album",
            Self::Ep => "ep",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "album" => Some(Self::Album),
            "ep" => Some(Self::Ep),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReleasesFilter {
    pub release_type: Option<ReleaseTypeFilter>,
    pub hidden: bool,
}

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

pub fn persisted_releases_filter(db: &crate::db::Db) -> Result<ReleasesFilter, rusqlite::Error> {
    let conn = db.conn();
    Ok(ReleasesFilter {
        release_type: crate::library::settings::get_setting_in(conn, RELEASES_FILTER_TYPE_KEY)?
            .as_deref()
            .and_then(ReleaseTypeFilter::parse),
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

pub fn filter_rows(rows: Vec<HistoryEntry>, filter: &ReleasesFilter) -> Vec<HistoryEntry> {
    rows.into_iter()
        .filter(|entry| entry.hidden == filter.hidden)
        .filter(|entry| entry.presence != LibraryPresence::Complete)
        .filter(|entry| {
            matches!(
                ReleaseTypeFilter::parse(&entry.release_type),
                Some(ReleaseTypeFilter::Album | ReleaseTypeFilter::Ep)
            )
        })
        .filter(|entry| {
            filter
                .release_type
                .is_none_or(|wanted| ReleaseTypeFilter::parse(&entry.release_type) == Some(wanted))
        })
        .collect()
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
    let conn = db.conn();
    query_releases_view_in(conn, filter, today)
}

fn query_releases_view_in(
    conn: &Connection,
    filter: &ReleasesFilter,
    today: NaiveDate,
) -> Result<Vec<HistoryEntry>, rusqlite::Error> {
    let artists = current_library_artist_keys(conn)?;
    let rows = query_history_in(conn, today)?
        .into_iter()
        .filter(|entry| artists.contains(&normalize(&entry.artist_name)))
        .collect();
    Ok(sort_rows(
        filter_rows(rows, filter),
        ReleaseSortDirection::Descending,
    ))
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
    Ok(query_releases_view_in(conn, filter, today)?.len() as i64)
}
