use std::collections::BTreeSet;

use crate::db::Db;
use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Timelike, Weekday};

/// Length of the rolling window behind [`StatsPeriod::Last30Days`], counted in
/// whole local calendar days including today.
pub const ROLLING_WINDOW_DAYS: i64 = 30;
/// Fewer active weeks than this use a cropped weekly bar axis.
pub const SPARSE_WEEK_THRESHOLD: usize = 8;

/// A user-selectable local listening-history period.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StatsPeriod {
    YearToDate(i32),
    Year(i32),
    Last30Days,
    AllTime,
}

/// Inclusive-start, exclusive-end range and its display buckets.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PeriodRange {
    pub start_unix: i64,
    pub end_unix: i64,
    pub granularity: Granularity,
    pub sparse_weeks: bool,
    pub buckets: Vec<Bucket>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Granularity {
    Day,
    Week,
    Month,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bucket {
    pub label: String,
    pub start_unix: i64,
    pub end_unix: i64,
    pub open: bool,
}

impl StatsPeriod {
    pub fn resolve<Tz: TimeZone>(
        self,
        now_unix: i64,
        tz: &Tz,
        first_event_unix: Option<i64>,
    ) -> PeriodRange {
        let Some(now) = tz.timestamp_opt(now_unix, 0).earliest() else {
            return empty_range();
        };
        let now_date = now.date_naive();
        let now_exclusive = now_unix.saturating_add(1);
        let (start_unix, end_unix, force_day, open) = match self {
            Self::YearToDate(year) => {
                let Some(start) = january_first(tz, year) else {
                    return empty_range();
                };
                let Some(year_end) = year.checked_add(1).and_then(|next| january_first(tz, next))
                else {
                    return empty_range();
                };
                if now.year() < year {
                    (start, start, false, false)
                } else if now.year() > year {
                    (start, year_end, false, false)
                } else {
                    (start, now_exclusive.min(year_end), false, true)
                }
            }
            Self::Year(year) => {
                let Some(start) = january_first(tz, year) else {
                    return empty_range();
                };
                let Some(end) = year.checked_add(1).and_then(|next| january_first(tz, next)) else {
                    return empty_range();
                };
                (start, end, false, false)
            }
            Self::Last30Days => {
                let start_date = now_date - Duration::days(ROLLING_WINDOW_DAYS - 1);
                let Some(start) = local_midnight(tz, start_date) else {
                    return empty_range();
                };
                (start, now_exclusive, true, true)
            }
            Self::AllTime => {
                let Some(first) = first_event_unix else {
                    return empty_range();
                };
                if first > now_unix {
                    (first, first, false, false)
                } else {
                    (first, now_exclusive, false, true)
                }
            }
        };

        if start_unix >= end_unix {
            return PeriodRange {
                start_unix,
                end_unix,
                ..empty_range()
            };
        }
        let span_days = span_days(tz, start_unix, end_unix);
        let granularity = if force_day {
            Granularity::Day
        } else {
            granularity_for(span_days, span_days)
        };
        PeriodRange {
            start_unix,
            end_unix,
            granularity,
            sparse_weeks: false,
            buckets: build_buckets(tz, start_unix, end_unix, granularity, open),
        }
    }

    /// The span the hero pill compares the selection against, or `None` for a
    /// period that has no meaningful predecessor.
    ///
    /// The compared span is equally long **and** seasonally congruent — the
    /// equally long stretch immediately before a year to date would run from
    /// the previous summer into winter, and listening time is seasonal, so
    /// that stretch would compare summer against winter. A year to date is
    /// therefore measured against the same calendar stretch of the previous
    /// year (Jan–Jul 2025 for "2026 so far"), and a full year against the
    /// whole year before it. Only the rolling 30-day window keeps the stretch
    /// immediately before it: it has no calendar counterpart a year back that
    /// a reader would recognise, and one month carries far less seasonal drift
    /// than half a year.
    ///
    /// Like [`Self::resolve`] this takes untrusted years and never panics on
    /// one; a year outside chrono's calendar yields `None`.
    pub fn previous_range<Tz: TimeZone>(self, now_unix: i64, tz: &Tz) -> Option<(i64, i64)> {
        let now = tz.timestamp_opt(now_unix, 0).earliest()?;
        let (start, end) = match self {
            Self::YearToDate(year) => {
                if now.year() < year {
                    return None;
                }
                let start = january_first(tz, year.checked_sub(1)?)?;
                // Past its own year a year to date *is* that full calendar
                // year, and so is the span it is compared against.
                let end = if now.year() > year {
                    january_first(tz, year)?
                } else {
                    same_moment_previous_year(tz, &now)?.saturating_add(1)
                };
                (start, end)
            }
            Self::Year(year) => (
                january_first(tz, year.checked_sub(1)?)?,
                january_first(tz, year)?,
            ),
            Self::Last30Days => {
                let end_date = now.date_naive() - Duration::days(ROLLING_WINDOW_DAYS - 1);
                (
                    local_midnight(tz, end_date - Duration::days(ROLLING_WINDOW_DAYS))?,
                    local_midnight(tz, end_date)?,
                )
            }
            Self::AllTime => return None,
        };
        (start < end).then_some((start, end))
    }

    /// Dropdown contents in editorial display order, limited to calendar
    /// years that contain detailed listening history.
    ///
    /// Imported `tracks.play_count` values have no trustworthy timestamp, so
    /// they cannot make a calendar year selectable. The current year remains
    /// available even before the first Reprise listen event.
    pub fn available<Tz: TimeZone>(
        db: &Db,
        now_year: i32,
        tz: &Tz,
    ) -> Result<Vec<StatsPeriod>, rusqlite::Error> {
        let conn = db.conn();
        let mut statement = conn.prepare("SELECT played_at FROM listen_events")?;
        let timestamps = statement.query_map([], |row| row.get::<_, i64>(0))?;
        let mut historical_years = BTreeSet::new();
        for timestamp in timestamps {
            let timestamp = timestamp?;
            let Some(local) = tz.timestamp_opt(timestamp, 0).earliest() else {
                continue;
            };
            if local.year() < now_year {
                historical_years.insert(local.year());
            }
        }

        let mut periods = Vec::with_capacity(historical_years.len() + 3);
        periods.push(Self::YearToDate(now_year));
        periods.extend(historical_years.into_iter().rev().map(Self::Year));
        periods.push(Self::AllTime);
        periods.push(Self::Last30Days);
        Ok(periods)
    }

    pub fn label(self) -> String {
        match self {
            Self::YearToDate(year) => format!("{year} so far"),
            Self::Year(year) => year.to_string(),
            Self::Last30Days => "Last 30 days".to_string(),
            Self::AllTime => "All time".to_string(),
        }
    }
}

/// Chooses the finest useful axis for the selected span and active-day count.
pub fn granularity_for(span_days: i64, distinct_active_days: i64) -> Granularity {
    if span_days <= 45 || distinct_active_days < 8 {
        Granularity::Day
    } else if span_days <= 730 || distinct_active_days < 24 {
        Granularity::Week
    } else {
        Granularity::Month
    }
}

/// Local calendar day and local hour of a stored Unix timestamp.
pub fn local_parts<Tz: TimeZone>(tz: &Tz, unix: i64) -> Option<(NaiveDate, u32)> {
    tz.timestamp_opt(unix, 0)
        .earliest()
        .map(|value| (value.date_naive(), value.hour()))
}

/// Monday that begins the local calendar week containing `unix`.
pub fn week_start<Tz: TimeZone>(tz: &Tz, unix: i64) -> Option<NaiveDate> {
    local_parts(tz, unix).map(|(date, _)| date.week(Weekday::Mon).first_day())
}

pub(crate) fn apply_activity_granularity<Tz: TimeZone>(
    range: &mut PeriodRange,
    tz: &Tz,
    distinct_active_days: i64,
    distinct_active_weeks: usize,
    first_active_unix: i64,
) {
    if range.start_unix >= range.end_unix {
        range.sparse_weeks = false;
        range.buckets.clear();
        return;
    }
    let open = range.buckets.last().is_some_and(|bucket| bucket.open);
    let days = span_days(tz, range.start_unix, range.end_unix);
    let sparse_weeks = days > 45 && days <= 730 && distinct_active_weeks < SPARSE_WEEK_THRESHOLD;
    let granularity = if sparse_weeks {
        Granularity::Week
    } else {
        granularity_for(days, distinct_active_days)
    };
    let bucket_start = if sparse_weeks {
        first_active_unix.max(range.start_unix)
    } else {
        range.start_unix
    };
    range.granularity = granularity;
    range.sparse_weeks = sparse_weeks;
    range.buckets = build_buckets(tz, bucket_start, range.end_unix, granularity, open);
}

fn empty_range() -> PeriodRange {
    PeriodRange {
        start_unix: 0,
        end_unix: 0,
        granularity: Granularity::Day,
        sparse_weeks: false,
        buckets: Vec::new(),
    }
}

/// Local midnight of January 1st, or `None` for a year chrono has no calendar
/// for. `resolve` is public, so the year is untrusted input — this module never
/// panics on it.
fn january_first<Tz: TimeZone>(tz: &Tz, year: i32) -> Option<i64> {
    local_midnight(tz, NaiveDate::from_ymd_opt(year, 1, 1)?)
}

/// The same local wall-clock moment one year earlier, so a year to date is
/// compared against a span that ends at the same point of the same calendar
/// day it does.
///
/// February 29th has no counterpart in a common year. It clamps to the 28th
/// rather than sliding into March: the compared span stays inside the same
/// month, one day short at most, which is a far smaller distortion than
/// letting a leap year borrow a day of the following season — and it cannot
/// panic on a date chrono refuses to build. A local time a DST transition
/// skips falls back to that day's midnight for the same reason.
fn same_moment_previous_year<Tz: TimeZone>(tz: &Tz, now: &DateTime<Tz>) -> Option<i64> {
    let local = now.naive_local();
    let date = local.date();
    let year = date.year().checked_sub(1)?;
    let shifted = NaiveDate::from_ymd_opt(year, date.month(), date.day())
        .or_else(|| NaiveDate::from_ymd_opt(year, date.month(), date.day().saturating_sub(1)))?;
    tz.from_local_datetime(&shifted.and_time(local.time()))
        .earliest()
        .map(|value| value.timestamp())
        .or_else(|| local_midnight(tz, shifted))
}

fn local_midnight<Tz: TimeZone>(tz: &Tz, date: NaiveDate) -> Option<i64> {
    let local = date.and_hms_opt(0, 0, 0)?;
    tz.from_local_datetime(&local)
        .earliest()
        .map(|value| value.timestamp())
}

fn span_days<Tz: TimeZone>(tz: &Tz, start_unix: i64, end_unix: i64) -> i64 {
    let Some((start, _)) = local_parts(tz, start_unix) else {
        return 0;
    };
    let Some((end, _)) = local_parts(tz, end_unix.saturating_sub(1)) else {
        return 0;
    };
    (end - start).num_days().saturating_add(1)
}

fn build_buckets<Tz: TimeZone>(
    tz: &Tz,
    start_unix: i64,
    end_unix: i64,
    granularity: Granularity,
    open: bool,
) -> Vec<Bucket> {
    let Some((mut cursor_date, _)) = local_parts(tz, start_unix) else {
        return Vec::new();
    };
    match granularity {
        Granularity::Week => {
            cursor_date = cursor_date.week(Weekday::Mon).first_day();
        }
        Granularity::Month => {
            let Some(first_of_month) =
                NaiveDate::from_ymd_opt(cursor_date.year(), cursor_date.month(), 1)
            else {
                return Vec::new();
            };
            cursor_date = first_of_month;
        }
        Granularity::Day => {}
    }

    let mut buckets = Vec::new();
    let mut bucket_start = start_unix;
    while bucket_start < end_unix {
        let (next_date, label) = match granularity {
            Granularity::Day => (
                cursor_date + Duration::days(1),
                cursor_date.format("%b %-d").to_string(),
            ),
            Granularity::Week => (
                cursor_date + Duration::days(7),
                format!("Week of {}", cursor_date.format("%b %-d")),
            ),
            Granularity::Month => {
                let next = if cursor_date.month() == 12 {
                    cursor_date
                        .year()
                        .checked_add(1)
                        .and_then(|year| NaiveDate::from_ymd_opt(year, 1, 1))
                } else {
                    NaiveDate::from_ymd_opt(cursor_date.year(), cursor_date.month() + 1, 1)
                };
                let Some(next) = next else {
                    break;
                };
                (next, cursor_date.format("%b").to_string())
            }
        };
        let Some(natural_end) = local_midnight(tz, next_date) else {
            break;
        };
        let bucket_end = natural_end.min(end_unix);
        if bucket_end <= bucket_start {
            break;
        }
        buckets.push(Bucket {
            label,
            start_unix: bucket_start,
            end_unix: bucket_end,
            open: open && bucket_end == end_unix,
        });
        bucket_start = bucket_end;
        cursor_date = next_date;
    }
    buckets
}

#[cfg(test)]
#[path = "stats_period_tests.rs"]
mod tests;
