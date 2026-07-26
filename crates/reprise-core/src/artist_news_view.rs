//! Full-page Releases filtering, status, and sorting.
//!
//! The UI consumes this module through the `artist_news` facade. Keeping the
//! decisions here makes sidebar counts and the visible table share exactly
//! one definition.

use std::cmp::Ordering;

use chrono::NaiveDate;
use rusqlite::Connection;

use crate::artist_news::{parse_partial_date, LibraryPresence};
use crate::artist_news_history::{query_history, HistoryEntry};
use crate::library::settings::{get_bool, get_setting};

pub const RELEASES_FILTER_NOT_IN_LIBRARY_KEY: &str = "releases.filter.not_in_library";
pub const RELEASES_FILTER_TYPE_KEY: &str = "releases.filter.type";
pub const RELEASES_FILTER_HIDDEN_KEY: &str = "releases.filter.hidden";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseTypeFilter {
    Album,
    Ep,
    Single,
}

impl ReleaseTypeFilter {
    pub const fn setting_value(self) -> &'static str {
        match self {
            Self::Album => "album",
            Self::Ep => "ep",
            Self::Single => "single",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "album" => Some(Self::Album),
            "ep" => Some(Self::Ep),
            "single" => Some(Self::Single),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReleasesFilter {
    pub not_in_library: bool,
    pub release_type: Option<ReleaseTypeFilter>,
    pub hidden: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseStatus {
    InLibrary,
    Upcoming,
    Released,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseSortDirection {
    Ascending,
    Descending,
}

pub fn persisted_releases_filter(conn: &Connection) -> Result<ReleasesFilter, rusqlite::Error> {
    Ok(ReleasesFilter {
        not_in_library: get_bool(conn, RELEASES_FILTER_NOT_IN_LIBRARY_KEY, false)?,
        release_type: get_setting(conn, RELEASES_FILTER_TYPE_KEY)?
            .as_deref()
            .and_then(ReleaseTypeFilter::parse),
        hidden: get_bool(conn, RELEASES_FILTER_HIDDEN_KEY, false)?,
    })
}

pub fn release_status(entry: &HistoryEntry, today: NaiveDate) -> ReleaseStatus {
    if entry.presence == LibraryPresence::Complete {
        return ReleaseStatus::InLibrary;
    }
    if parse_partial_date(&entry.first_release_date).is_some_and(|date| date > today) {
        ReleaseStatus::Upcoming
    } else {
        ReleaseStatus::Released
    }
}

pub fn filter_rows(rows: Vec<HistoryEntry>, filter: &ReleasesFilter) -> Vec<HistoryEntry> {
    rows.into_iter()
        .filter(|entry| entry.hidden == filter.hidden)
        .filter(|entry| !filter.not_in_library || entry.presence != LibraryPresence::Complete)
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
    conn: &Connection,
    filter: &ReleasesFilter,
    today: NaiveDate,
) -> Result<Vec<HistoryEntry>, rusqlite::Error> {
    let rows = query_history(conn, today)?;
    Ok(sort_rows(
        filter_rows(rows, filter),
        ReleaseSortDirection::Descending,
    ))
}

pub fn count_releases_view(
    conn: &Connection,
    filter: &ReleasesFilter,
    today: NaiveDate,
) -> Result<i64, rusqlite::Error> {
    Ok(query_releases_view(conn, filter, today)?.len() as i64)
}
