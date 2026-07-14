//! Pure math/formatting functions behind the 12-month listening activity
//! chart. Kept free of GTK so the bar-height normalization and label
//! extraction are unit-testable without a display.

/// Normalizes a slice of non-negative values into the 0.0..=1.0 range,
/// dividing each by the maximum. An all-zero (or empty) input returns all
/// zeros — the chart simply draws nothing.
pub(super) fn normalize_bars(values: &[i64]) -> Vec<f64> {
    let max = values.iter().copied().max().unwrap_or(0);
    if max == 0 {
        return vec![0.0; values.len()];
    }
    values.iter().map(|&v| (v as f64) / (max as f64)).collect()
}

/// Extracts a short month label from a `"YYYY-MM"` string (the format
/// `MonthlyListens::year_month` uses). Returns the 3-letter English
/// abbreviation (`"Jan"`, `"Feb"`, …) or the raw input if parsing fails.
pub(super) fn short_month_label(year_month: &str) -> &str {
    let month_part = year_month.get(5..7).unwrap_or(year_month);
    match month_part {
        "01" => "Jan",
        "02" => "Feb",
        "03" => "Mar",
        "04" => "Apr",
        "05" => "May",
        "06" => "Jun",
        "07" => "Jul",
        "08" => "Aug",
        "09" => "Sep",
        "10" => "Oct",
        "11" => "Nov",
        "12" => "Dec",
        _ => year_month,
    }
}

/// Converts milliseconds into a human-readable hours string (rounded down).
pub(super) fn ms_to_hours(ms: i64) -> i64 {
    ms / 3_600_000
}

/// Whether bar `index` of `count` is within the highlighted (most recent)
/// portion. Currently the last bar (the current month) gets full accent
/// alpha; the rest are dimmed.
pub(super) fn is_current_month(index: usize, count: usize) -> bool {
    count > 0 && index == count - 1
}

/// Floor fraction so every bar — even a near-zero month — draws a visible
/// sliver (matches `waveform_seek`'s `MIN_BAR_HEIGHT_FRACTION` convention).
pub(super) const MIN_BAR_FRACTION: f64 = 0.06;

/// Alpha applied to past-month bars (the current month uses full alpha).
pub(super) const PAST_MONTH_ALPHA: f64 = 0.55;

/// Gap between bars as a fraction of each bar's horizontal slot.
pub(super) const BAR_GAP_FRACTION: f64 = 0.30;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_bars_scales_to_unit_range() {
        let bars = normalize_bars(&[100, 200, 50, 0]);
        assert_eq!(bars, vec![0.5, 1.0, 0.25, 0.0]);
    }

    #[test]
    fn normalize_bars_all_zero_returns_zeros() {
        assert_eq!(normalize_bars(&[0, 0, 0]), vec![0.0, 0.0, 0.0]);
    }

    #[test]
    fn normalize_bars_empty_returns_empty() {
        let empty: Vec<f64> = vec![];
        assert_eq!(normalize_bars(&[]), empty);
    }

    #[test]
    fn normalize_bars_single_value() {
        assert_eq!(normalize_bars(&[42]), vec![1.0]);
    }

    #[test]
    fn short_month_label_maps_all_months() {
        let expected = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        for (i, &label) in expected.iter().enumerate() {
            let ym = format!("2026-{:02}", i + 1);
            assert_eq!(short_month_label(&ym), label);
        }
    }

    #[test]
    fn short_month_label_returns_raw_on_bad_input() {
        assert_eq!(short_month_label("bad"), "bad");
    }

    #[test]
    fn ms_to_hours_rounds_down() {
        assert_eq!(ms_to_hours(3_600_000), 1);
        assert_eq!(ms_to_hours(7_199_999), 1);
        assert_eq!(ms_to_hours(7_200_000), 2);
        assert_eq!(ms_to_hours(0), 0);
    }

    #[test]
    fn is_current_month_only_last_bar() {
        assert!(is_current_month(11, 12));
        assert!(!is_current_month(10, 12));
        assert!(!is_current_month(0, 12));
        assert!(!is_current_month(0, 0));
    }
}
