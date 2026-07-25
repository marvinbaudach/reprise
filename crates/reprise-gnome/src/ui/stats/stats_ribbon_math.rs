use chrono::{Datelike, NaiveDate, Weekday};
use reprise_core::library::stats_period::Granularity;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::ui) struct RibbonLayout {
    pub points: Vec<Point>,
    pub open_index: Option<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::ui) struct MonthTick {
    pub index: usize,
    pub label: String,
}

pub(in crate::ui) fn ribbon_layout(
    values: &[i64],
    width: f64,
    height: f64,
    open_index: Option<usize>,
) -> RibbonLayout {
    let maximum = values.iter().copied().max().unwrap_or(0).max(0);
    let points = values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let x = if values.len() <= 1 {
                width / 2.0
            } else {
                index as f64 * width / (values.len() - 1) as f64
            };
            let magnitude = if maximum == 0 {
                0.0
            } else {
                (*value).max(0) as f64 / maximum as f64
            };
            Point {
                x,
                y: height - magnitude * height,
            }
        })
        .collect();
    RibbonLayout {
        points,
        open_index: open_index.filter(|index| *index < values.len()),
    }
}

pub(in crate::ui) fn best_week_bucket_index(
    bucket_starts: &[Option<NaiveDate>],
    granularity: Granularity,
    best_week_start: Option<NaiveDate>,
) -> Option<usize> {
    if granularity != Granularity::Week {
        return None;
    }
    let best_week_start = best_week_start?;
    bucket_starts.iter().position(|start| {
        start.is_some_and(|start| start.week(Weekday::Mon).first_day() == best_week_start)
    })
}

pub(in crate::ui) fn month_ticks(bucket_starts: &[Option<NaiveDate>]) -> Vec<MonthTick> {
    let mut previous = None;
    let mut ticks = Vec::new();
    for (index, date) in bucket_starts.iter().enumerate() {
        let Some(date) = date else { continue };
        if previous.is_none_or(|previous: NaiveDate| previous.month() != date.month()) {
            ticks.push(MonthTick {
                index,
                label: date.format("%b").to_string().to_uppercase(),
            });
        }
        previous = Some(*date);
    }
    ticks
}

pub(in crate::ui) fn reveal_clip_width(width: f64, reveal_fraction: f64) -> f64 {
    width.max(0.0) * reveal_fraction.clamp(0.0, 1.0)
}

pub(in crate::ui) fn bucket_at_x(x: f64, width: f64, bucket_count: usize) -> Option<usize> {
    if bucket_count == 0 || width <= 0.0 || x < 0.0 || x >= width {
        return None;
    }
    Some(((x / width * bucket_count as f64).floor() as usize).min(bucket_count - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ribbon_area_path_spans_every_bucket() {
        let layout = ribbon_layout(&[10, 20, 5], 300.0, 100.0, None);

        assert_eq!(layout.points.len(), 3);
        assert_eq!(layout.points[0].x, 0.0);
        assert_eq!(layout.points[1].x, 150.0);
        assert_eq!(layout.points[2].x, 300.0);
    }

    #[test]
    fn ribbon_marks_the_open_bucket_and_the_peak() {
        let layout = ribbon_layout(&[10, 30, 20], 300.0, 100.0, Some(2));

        assert_eq!(layout.open_index, Some(2));
    }

    #[test]
    fn ribbon_with_all_zero_values_draws_a_flat_baseline() {
        let layout = ribbon_layout(&[0, 0, 0], 300.0, 100.0, None);

        assert!(layout.points.iter().all(|point| point.y == 100.0));
    }

    #[test]
    fn ribbon_hover_maps_x_to_the_bucket_under_the_cursor() {
        assert_eq!(bucket_at_x(0.0, 300.0, 3), Some(0));
        assert_eq!(bucket_at_x(99.0, 300.0, 3), Some(0));
        assert_eq!(bucket_at_x(100.0, 300.0, 3), Some(1));
        assert_eq!(bucket_at_x(299.0, 300.0, 3), Some(2));
        assert_eq!(bucket_at_x(-1.0, 300.0, 3), None);
        assert_eq!(bucket_at_x(300.0, 300.0, 3), None);
        assert_eq!(bucket_at_x(529.0, 530.0, 53), Some(52));
    }

    #[test]
    fn stats_12_marker_sits_on_the_best_week_bucket() {
        let starts = [
            Some(NaiveDate::from_ymd_opt(2026, 3, 2).unwrap()),
            Some(NaiveDate::from_ymd_opt(2026, 3, 9).unwrap()),
            Some(NaiveDate::from_ymd_opt(2026, 3, 16).unwrap()),
        ];

        assert_eq!(
            best_week_bucket_index(&starts, Granularity::Week, starts[1]),
            Some(1)
        );
        assert_eq!(
            best_week_bucket_index(&starts, Granularity::Day, starts[1]),
            None
        );
    }

    #[test]
    fn stats_12_marker_matches_a_week_clipped_by_the_period_start() {
        let clipped_period_start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let next_week = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
        let aligned_best_week = NaiveDate::from_ymd_opt(2025, 12, 29).unwrap();

        assert_eq!(
            best_week_bucket_index(
                &[Some(clipped_period_start), Some(next_week)],
                Granularity::Week,
                Some(aligned_best_week)
            ),
            Some(0)
        );
    }

    #[test]
    fn month_ticks_derive_from_bucket_starts() {
        let starts = [
            Some(NaiveDate::from_ymd_opt(2026, 1, 26).unwrap()),
            Some(NaiveDate::from_ymd_opt(2026, 2, 2).unwrap()),
            Some(NaiveDate::from_ymd_opt(2026, 2, 9).unwrap()),
            Some(NaiveDate::from_ymd_opt(2026, 3, 2).unwrap()),
        ];

        assert_eq!(
            month_ticks(&starts),
            vec![
                MonthTick {
                    index: 0,
                    label: "JAN".into()
                },
                MonthTick {
                    index: 1,
                    label: "FEB".into()
                },
                MonthTick {
                    index: 3,
                    label: "MAR".into()
                }
            ]
        );
    }

    #[test]
    fn missing_local_dates_keep_month_tick_indices_aligned_with_values() {
        let starts = [
            Some(NaiveDate::from_ymd_opt(2026, 1, 26).unwrap()),
            None,
            Some(NaiveDate::from_ymd_opt(2026, 2, 9).unwrap()),
        ];

        assert_eq!(
            month_ticks(&starts),
            vec![
                MonthTick {
                    index: 0,
                    label: "JAN".into()
                },
                MonthTick {
                    index: 2,
                    label: "FEB".into()
                }
            ]
        );
    }

    #[test]
    fn reveal_fraction_clips_the_area_path() {
        assert_eq!(reveal_clip_width(320.0, 0.0), 0.0);
        assert_eq!(reveal_clip_width(320.0, 0.4), 128.0);
        assert_eq!(reveal_clip_width(320.0, 2.0), 320.0);
    }
}
