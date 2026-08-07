//! Persisted scope values shared by the Releases table, badge, and popover.

use chrono::{Months, NaiveDate};
use std::collections::HashMap;

use crate::artist_news::normalize;
use crate::artist_news_parsing::parse_partial_date;
use crate::artist_news_query::LibraryPresence;

pub const RELEASES_FILTER_TYPE_KEY: &str = "releases.filter.type";
pub const RELEASES_FILTER_WINDOW_KEY: &str = "releases.filter.window";
pub const RELEASES_FILTER_HIDDEN_KEY: &str = "releases.filter.hidden";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReleaseWindow {
    OneYear,
    #[default]
    FiveYears,
    TenYears,
    All,
}

impl ReleaseWindow {
    pub const fn setting_value(self) -> &'static str {
        match self {
            Self::OneYear => "1y",
            Self::FiveYears => "5y",
            Self::TenYears => "10y",
            Self::All => "all",
        }
    }

    pub fn cutoff(self, today: NaiveDate) -> Option<NaiveDate> {
        let months = match self {
            Self::OneYear => 12,
            Self::FiveYears => 60,
            Self::TenYears => 120,
            Self::All => return None,
        };
        today.checked_sub_months(Months::new(months))
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "1y" => Some(Self::OneYear),
            "5y" => Some(Self::FiveYears),
            "10y" => Some(Self::TenYears),
            "all" => Some(Self::All),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReleaseTypeSelection {
    pub album: bool,
    pub ep: bool,
    pub single: bool,
}

impl Default for ReleaseTypeSelection {
    fn default() -> Self {
        Self {
            album: true,
            ep: true,
            single: false,
        }
    }
}

impl ReleaseTypeSelection {
    pub const fn all() -> Self {
        Self {
            album: true,
            ep: true,
            single: true,
        }
    }

    pub const fn empty() -> Self {
        Self {
            album: false,
            ep: false,
            single: false,
        }
    }

    pub const fn is_empty(self) -> bool {
        !self.album && !self.ep && !self.single
    }

    pub const fn is_all(self) -> bool {
        self.album && self.ep && self.single
    }

    pub fn includes(self, release_type: &str) -> bool {
        if self.is_empty() {
            return catalog_type(release_type);
        }
        match release_type.trim().to_ascii_lowercase().as_str() {
            "album" => self.album,
            "ep" => self.ep,
            "single" => self.single,
            _ => false,
        }
    }

    pub fn setting_value(self) -> String {
        [
            (self.album, "album"),
            (self.ep, "ep"),
            (self.single, "single"),
        ]
        .into_iter()
        .filter_map(|(selected, value)| selected.then_some(value))
        .collect::<Vec<_>>()
        .join(",")
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        if value.trim().is_empty() {
            return Some(Self::empty());
        }
        let mut selection = Self::empty();
        for value in value.split(',').map(str::trim) {
            match value.to_ascii_lowercase().as_str() {
                "album" => selection.album = true,
                "ep" => selection.ep = true,
                "single" => selection.single = true,
                _ => return None,
            }
        }
        Some(selection)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReleasesFilter {
    pub release_types: ReleaseTypeSelection,
    pub window: ReleaseWindow,
    pub hidden: bool,
}

impl ReleasesFilter {
    pub const fn widest(hidden: bool) -> Self {
        Self {
            release_types: ReleaseTypeSelection::empty(),
            window: ReleaseWindow::All,
            hidden,
        }
    }

    pub const fn is_widest(&self) -> bool {
        (self.release_types.is_empty() || self.release_types.is_all())
            && matches!(self.window, ReleaseWindow::All)
            && !self.hidden
    }
}

pub(crate) fn catalog_type(release_type: &str) -> bool {
    matches!(
        release_type.trim().to_ascii_lowercase().as_str(),
        "album" | "ep" | "single"
    )
}

pub fn counts_as_owned(
    presence: LibraryPresence,
    release_type: &str,
    first_release_date: &str,
    track_count: Option<i64>,
    local_track_count: i64,
    today: NaiveDate,
) -> bool {
    if parse_partial_date(first_release_date).is_some_and(|date| date > today) {
        return false;
    }
    if presence == LibraryPresence::Complete {
        return true;
    }
    if release_type.eq_ignore_ascii_case("single") {
        return local_track_count > 0;
    }
    track_count
        .is_some_and(|official| official >= 2 && local_track_count.saturating_mul(2) > official)
}

pub(crate) trait ScopedRelease {
    fn artist_name(&self) -> &str;
    fn title(&self) -> &str;
    fn first_release_date(&self) -> &str;
    fn release_type(&self) -> &str;
    fn track_count(&self) -> Option<i64>;
    fn release_group_mbid(&self) -> &str;
}

pub(crate) fn collapse_duplicates<T: ScopedRelease>(rows: Vec<T>) -> Vec<T> {
    let mut positions = HashMap::new();
    let mut collapsed = Vec::with_capacity(rows.len());
    for row in rows {
        let key = (
            normalize(row.artist_name()),
            normalize(row.title()),
            row.first_release_date().to_owned(),
        );
        if let Some(index) = positions.get(&key).copied() {
            if release_precedes(&row, &collapsed[index]) {
                collapsed[index] = row;
            }
        } else {
            positions.insert(key, collapsed.len());
            collapsed.push(row);
        }
    }
    collapsed
}

fn release_precedes<T: ScopedRelease>(candidate: &T, current: &T) -> bool {
    release_type_rank(candidate.release_type())
        .cmp(&release_type_rank(current.release_type()))
        .then_with(|| {
            candidate
                .track_count()
                .is_some()
                .cmp(&current.track_count().is_some())
        })
        .then_with(|| {
            current
                .release_group_mbid()
                .cmp(candidate.release_group_mbid())
        })
        .is_gt()
}

fn release_type_rank(release_type: &str) -> u8 {
    match release_type.trim().to_ascii_lowercase().as_str() {
        "album" => 3,
        "ep" => 2,
        "single" => 1,
        _ => 0,
    }
}
