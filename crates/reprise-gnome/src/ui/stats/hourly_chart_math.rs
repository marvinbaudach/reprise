//! Pure math behind the 24-hour listening clock. Kept free of GTK so the
//! bar-height normalization is unit-testable without a display.

/// Normalizes a slice of non-negative values into the 0.0..=1.0 range,
/// dividing each by the maximum. An all-zero (or empty) input returns all
/// zeros — the chart simply draws nothing.
pub(in crate::ui) fn normalize_bars(values: &[i64]) -> Vec<f64> {
    let max = values.iter().copied().max().unwrap_or(0);
    if max == 0 {
        return vec![0.0; values.len()];
    }
    values.iter().map(|&v| (v as f64) / (max as f64)).collect()
}

/// Floor fraction so every bar — even a near-zero month — draws a visible
/// sliver (matches `waveform_seek`'s `MIN_BAR_HEIGHT_FRACTION` convention).
pub(in crate::ui) const MIN_BAR_FRACTION: f64 = 0.06;

/// Gap between bars as a fraction of each bar's horizontal slot.
pub(in crate::ui) const BAR_GAP_FRACTION: f64 = 0.30;

/// Expands sparse hourly data (only hours with events) into a full 24-slot
/// array (hours 0-23), filling missing hours with zero.
pub(in crate::ui) fn expand_hourly(sparse: &[(u8, i64)]) -> [i64; 24] {
    let mut full = [0i64; 24];
    for &(hour, listens) in sparse {
        if (hour as usize) < 24 {
            full[hour as usize] = listens;
        }
    }
    full
}

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
}
