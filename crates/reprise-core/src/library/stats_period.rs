use chrono::{Datelike, Duration, NaiveDate, TimeZone, Timelike};
use rusqlite::Connection;

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
                let Some(start) = local_midnight(tz, date(year, 1, 1)) else {
                    return empty_range();
                };
                let Some(year_end) = local_midnight(tz, date(year + 1, 1, 1)) else {
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
                let Some(start) = local_midnight(tz, date(year, 1, 1)) else {
                    return empty_range();
                };
                let Some(end) = local_midnight(tz, date(year + 1, 1, 1)) else {
                    return empty_range();
                };
                (start, end, false, false)
            }
            Self::Last30Days => {
                let start_date = now_date - Duration::days(29);
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
            buckets: build_buckets(tz, start_unix, end_unix, granularity, open),
        }
    }

    /// Dropdown contents in their fixed editorial display order.
    pub fn available(
        _conn: &Connection,
        now_year: i32,
    ) -> Result<Vec<StatsPeriod>, rusqlite::Error> {
        Ok(vec![
            Self::YearToDate(now_year),
            Self::Year(now_year - 1),
            Self::AllTime,
            Self::Last30Days,
        ])
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
    } else if span_days <= 120 || distinct_active_days < 24 {
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

pub(crate) fn apply_activity_granularity<Tz: TimeZone>(
    range: &mut PeriodRange,
    tz: &Tz,
    distinct_active_days: i64,
) {
    if range.start_unix >= range.end_unix {
        range.buckets.clear();
        return;
    }
    let open = range.buckets.last().is_some_and(|bucket| bucket.open);
    let granularity = granularity_for(
        span_days(tz, range.start_unix, range.end_unix),
        distinct_active_days,
    );
    range.granularity = granularity;
    range.buckets = build_buckets(tz, range.start_unix, range.end_unix, granularity, open);
}

fn empty_range() -> PeriodRange {
    PeriodRange {
        start_unix: 0,
        end_unix: 0,
        granularity: Granularity::Day,
        buckets: Vec::new(),
    }
}

fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).expect("fixed calendar date is valid")
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
    if granularity == Granularity::Month {
        cursor_date = date(cursor_date.year(), cursor_date.month(), 1);
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
                    date(cursor_date.year() + 1, 1, 1)
                } else {
                    date(cursor_date.year(), cursor_date.month() + 1, 1)
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
