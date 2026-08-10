//! View-neutral row geometry for virtualized GTK list widgets.
//!
//! A model swap can pair the new row count with the old adjustment for one
//! allocation frame. The adjustment quotient is therefore trusted only when
//! it agrees with an independently measured, uniform set of bound row widgets.

use std::collections::BTreeMap;

use gtk4::glib::prelude::{Cast, ObjectExt};
use gtk4::prelude::WidgetExt;

const ROW_HEIGHT_AGREEMENT_EPSILON: f64 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) struct RowHeight(f64);

impl RowHeight {
    pub(in crate::ui) fn new(value: f64) -> Option<Self> {
        (value.is_finite() && value > 0.0).then_some(Self(value))
    }

    pub(in crate::ui) fn pixels(self) -> f64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) enum ContentHeight {
    Known(f64),
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(in crate::ui) struct RowMeasurement {
    modal: Option<RowHeight>,
    uniform: bool,
}

impl RowMeasurement {
    /// Measures the unique most frequent non-zero allocated widget height.
    /// Zero-height widgets are unrealized and do not describe a bound row.
    pub(in crate::ui) fn from_widget_heights(heights: impl IntoIterator<Item = i32>) -> Self {
        let mut counts = BTreeMap::<i32, usize>::new();
        for height in heights.into_iter().filter(|height| *height > 0) {
            *counts.entry(height).or_default() += 1;
        }
        let max_count = counts.values().copied().max();
        let modes = counts
            .iter()
            .filter(|(_, count)| Some(**count) == max_count)
            .map(|(height, _)| *height)
            .collect::<Vec<_>>();
        Self {
            modal: (modes.len() == 1)
                .then(|| RowHeight::new(f64::from(modes[0])))
                .flatten(),
            uniform: counts.len() == 1,
        }
    }

    pub(in crate::ui) fn modal(self) -> Option<RowHeight> {
        self.modal
    }

    pub(in crate::ui) fn is_uniform(self) -> bool {
        self.uniform
    }
}

/// Returns the independently measured row height only when widget allocation
/// is uniform and agrees with the adjustment quotient.
pub(in crate::ui) fn settled_row_height(
    upper: f64,
    n_rows: usize,
    measurement: RowMeasurement,
) -> Option<RowHeight> {
    if n_rows == 0 || !upper.is_finite() || upper <= 0.0 || !measurement.is_uniform() {
        return None;
    }
    let widget_height = measurement.modal()?;
    let adjustment_height = upper / n_rows as f64;
    ((adjustment_height - widget_height.pixels()).abs() < ROW_HEIGHT_AGREEMENT_EPSILON)
        .then_some(widget_height)
}

pub(in crate::ui) fn is_settled(upper: f64, n_rows: usize, measurement: RowMeasurement) -> bool {
    settled_row_height(upper, n_rows, measurement).is_some()
}

pub(in crate::ui) fn content_height(
    n_rows: usize,
    n_sections: usize,
    row_height: RowHeight,
    section_header_height: Option<RowHeight>,
) -> ContentHeight {
    let rows = n_rows as f64 * row_height.pixels();
    if n_sections == 0 {
        return ContentHeight::Known(rows);
    }
    section_header_height.map_or(ContentHeight::Unknown, |header| {
        ContentHeight::Known((n_sections as f64).mul_add(header.pixels(), rows))
    })
}

/// GTK-facing handle for one list view. It owns no track-list state and can be
/// constructed for any `ColumnView`; all acceptance arithmetic above remains
/// GTK-free and directly unit tested.
#[derive(Clone)]
#[allow(dead_code)] // The G3 call-site migration consumes this service.
pub(in crate::ui) struct ListGeometry {
    view: gtk4::ColumnView,
}

#[allow(dead_code)] // The G3 call-site migration consumes these GTK adapters.
impl ListGeometry {
    pub(in crate::ui) fn for_view(view: &gtk4::ColumnView) -> Self {
        Self { view: view.clone() }
    }

    pub(in crate::ui) fn measurement(&self) -> RowMeasurement {
        fn collect(widget: &gtk4::Widget, heights: &mut Vec<i32>) {
            if widget.type_().name().contains("ColumnViewRow") {
                heights.push(widget.height());
            }
            let mut child = widget.first_child();
            while let Some(current) = child {
                collect(&current, heights);
                child = current.next_sibling();
            }
        }

        let mut heights = Vec::new();
        collect(self.view.upcast_ref(), &mut heights);
        RowMeasurement::from_widget_heights(heights)
    }

    pub(in crate::ui) fn settled_row_height(&self, upper: f64, n_rows: usize) -> Option<RowHeight> {
        settled_row_height(upper, n_rows, self.measurement())
    }

    pub(in crate::ui) fn is_settled(&self, upper: f64, n_rows: usize) -> bool {
        is_settled(upper, n_rows, self.measurement())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_modal_nonzero_widget_height_is_measured_independently() {
        let measurement = RowMeasurement::from_widget_heights([0, 25, 25, 34]);

        assert_eq!(measurement.modal(), RowHeight::new(25.0));
        assert!(!measurement.is_uniform());
    }

    #[test]
    fn stale_adjustment_and_recycled_widgets_are_not_settled() {
        let measurement = RowMeasurement::from_widget_heights([0, 25, 25, 34]);

        assert_eq!(settled_row_height(748.0, 2_276, measurement), None);
    }

    #[test]
    fn independent_uniform_sources_can_settle() {
        let measurement = RowMeasurement::from_widget_heights([34, 34, 34]);

        assert_eq!(
            settled_row_height(2_276.0 * 34.0 + 0.25, 2_276, measurement),
            RowHeight::new(34.0)
        );
        assert!(is_settled(2_276.0 * 34.0 + 0.25, 2_276, measurement));
    }

    #[test]
    fn section_content_stays_unknown_until_its_header_is_measured() {
        let row_height = RowHeight::new(34.0).unwrap();

        assert_eq!(
            content_height(100, 0, row_height, None),
            ContentHeight::Known(3_400.0)
        );
        assert_eq!(
            content_height(100, 2, row_height, None),
            ContentHeight::Unknown
        );
        assert_eq!(
            content_height(100, 2, row_height, RowHeight::new(20.0)),
            ContentHeight::Known(3_440.0)
        );
    }
}
