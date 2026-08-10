//! View-neutral row geometry for virtualized GTK list widgets.
//!
//! A model swap can pair the new row count with the old adjustment for one
//! allocation frame. The adjustment quotient is therefore trusted only when
//! it agrees with an independently measured, uniform set of bound row widgets.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use gtk4::glib::prelude::{Cast, ObjectExt};
use gtk4::prelude::{AdjustmentExt, ScrollableExt, WidgetExt};
use reprise_core::library::settings::{self, ListDensity};

const ROW_HEIGHT_AGREEMENT_EPSILON: f64 = 0.5;
pub(in crate::ui) const INVALIDATED_ROW_HEIGHT: f64 = -1.0;

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
    let adjustment_height = upper * (n_rows as f64).recip();
    ((adjustment_height - widget_height.pixels()).abs() < ROW_HEIGHT_AGREEMENT_EPSILON)
        .then_some(widget_height)
}

pub(in crate::ui) fn settled_content_row_height(
    upper: f64,
    n_rows: usize,
    n_sections: usize,
    rows: RowMeasurement,
    headers: Option<RowMeasurement>,
) -> Option<RowHeight> {
    if n_sections == 0 {
        return settled_row_height(upper, n_rows, rows);
    }
    let headers = headers?;
    if !headers.is_uniform() {
        return None;
    }
    let header_height = headers.modal()?;
    let row_content_height = upper - n_sections as f64 * header_height.pixels();
    settled_row_height(row_content_height, n_rows, rows)
}

#[cfg(test)]
pub(in crate::ui) fn adjustment_row_height(upper: f64, n_rows: usize) -> Option<RowHeight> {
    if n_rows == 0 {
        return None;
    }
    RowHeight::new(upper * (n_rows as f64).recip())
}

#[allow(dead_code)] // The G4 readiness migration consumes this pure predicate.
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

pub(in crate::ui) fn invalidate_row_height(cache: &Cell<f64>) {
    cache.set(INVALIDATED_ROW_HEIGHT);
}

struct OneShot<F>(RefCell<Option<F>>);

impl<F> OneShot<F> {
    fn new(callback: F) -> Self {
        Self(RefCell::new(Some(callback)))
    }

    fn take(&self) -> Option<F> {
        self.0.borrow_mut().take()
    }
}

pub(in crate::ui) fn on_changed_once(
    adjustment: &gtk4::Adjustment,
    callback: impl FnOnce(&gtk4::Adjustment) + 'static,
) {
    let handler = Rc::new(RefCell::new(None));
    let pending_callback = Rc::new(OneShot::new(callback));
    let callback_handler = handler.clone();
    let callback_slot = pending_callback.clone();
    let id = adjustment.connect_changed(move |changed| {
        let handler = callback_handler.borrow_mut().take();
        if let Some(handler) = handler {
            changed.disconnect(handler);
        }
        let callback = callback_slot.take();
        if let Some(callback) = callback {
            callback(changed);
        }
    });
    handler.borrow_mut().replace(id);
}

fn load_row_height(
    db: &reprise_core::db::Db,
    density: ListDensity,
    cache: &Cell<f64>,
    minimum: RowHeight,
) -> RowHeight {
    if let Some(cached) = RowHeight::new(cache.get()) {
        return cached;
    }
    let invalidated = cache.get() == INVALIDATED_ROW_HEIGHT;
    if invalidated {
        if let Err(error) = settings::set_row_height(db, density, None) {
            tracing::warn!(%error, "could not discard invalidated row height");
        }
    }
    let persisted = if invalidated {
        None
    } else {
        settings::get_row_height(db, density).unwrap_or_else(|error| {
            tracing::warn!(%error, "could not load persisted row height");
            None
        })
    };
    let loaded = persisted.and_then(RowHeight::new).unwrap_or(minimum);
    cache.set(loaded.pixels());
    loaded
}

/// GTK-facing handle for one list view. It owns no track-list state and can be
/// constructed for any `ColumnView`; all acceptance arithmetic above remains
/// GTK-free and directly unit tested.
#[derive(Clone)]
pub(in crate::ui) struct ListGeometry {
    view: gtk4::ColumnView,
}

impl ListGeometry {
    pub(in crate::ui) fn for_view(view: &gtk4::ColumnView) -> Self {
        Self { view: view.clone() }
    }

    pub(in crate::ui) fn measurement(&self) -> RowMeasurement {
        self.widget_measurement("ColumnViewRow")
    }

    fn widget_measurement(&self, type_fragment: &str) -> RowMeasurement {
        fn collect(widget: &gtk4::Widget, type_fragment: &str, heights: &mut Vec<i32>) {
            if widget.type_().name().contains(type_fragment) {
                heights.push(widget.height());
            }
            let mut child = widget.first_child();
            while let Some(current) = child {
                collect(&current, type_fragment, heights);
                child = current.next_sibling();
            }
        }

        let mut heights = Vec::new();
        collect(self.view.upcast_ref(), type_fragment, &mut heights);
        RowMeasurement::from_widget_heights(heights)
    }

    fn section_header_measurement(&self) -> Option<RowMeasurement> {
        let measurement = self.widget_measurement("ListHeader");
        measurement.modal().map(|_| measurement)
    }

    pub(in crate::ui) fn settled_row_height(&self, upper: f64, n_rows: usize) -> Option<RowHeight> {
        settled_row_height(upper, n_rows, self.measurement())
    }

    pub(in crate::ui) fn is_settled(&self, upper: f64, n_rows: usize, n_sections: usize) -> bool {
        settled_content_row_height(
            upper,
            n_rows,
            n_sections,
            self.measurement(),
            self.section_header_measurement(),
        )
        .is_some()
    }

    fn density(&self) -> ListDensity {
        if self.view.has_css_class("reprise-density-comfortable") {
            ListDensity::Comfortable
        } else if self.view.has_css_class("reprise-density-compact") {
            ListDensity::Compact
        } else {
            ListDensity::Standard
        }
    }

    fn minimum_row_height(&self) -> RowHeight {
        use crate::ui::style::tokens::{
            ROW_MIN_HEIGHT_COMFORTABLE, ROW_MIN_HEIGHT_COMPACT, ROW_MIN_HEIGHT_STANDARD,
        };
        let minimum = match self.density() {
            ListDensity::Comfortable => ROW_MIN_HEIGHT_COMFORTABLE,
            ListDensity::Standard => ROW_MIN_HEIGHT_STANDARD,
            ListDensity::Compact => ROW_MIN_HEIGHT_COMPACT,
        };
        RowHeight::new(f64::from(minimum)).expect("density minima are positive")
    }

    pub(in crate::ui) fn row_height(
        &self,
        db: &reprise_core::db::Db,
        cache: &Cell<f64>,
    ) -> RowHeight {
        load_row_height(db, self.density(), cache, self.minimum_row_height())
    }

    pub(in crate::ui) fn remember_if_settled(
        &self,
        db: &reprise_core::db::Db,
        cache: &Cell<f64>,
        upper: f64,
        n_rows: usize,
        n_sections: usize,
    ) -> bool {
        let Some(height) = settled_content_row_height(
            upper,
            n_rows,
            n_sections,
            self.measurement(),
            self.section_header_measurement(),
        ) else {
            return false;
        };
        cache.set(height.pixels());
        if let Err(error) = settings::set_row_height(db, self.density(), Some(height.pixels())) {
            tracing::warn!(%error, "could not persist settled row height");
        }
        true
    }

    pub(in crate::ui) fn observed_row_height(
        &self,
        db: &reprise_core::db::Db,
        cache: &Cell<f64>,
        n_rows: usize,
        n_sections: usize,
    ) -> Option<RowHeight> {
        if n_rows == 0 {
            return None;
        }
        if let Some(adjustment) = self.view.vadjustment() {
            self.remember_if_settled(db, cache, adjustment.upper(), n_rows, n_sections);
        }
        Some(self.row_height(db, cache))
    }

    pub(in crate::ui) fn live_row_height(&self, n_rows: usize) -> Option<RowHeight> {
        let adjustment = self.view.vadjustment()?;
        self.settled_row_height(adjustment.upper(), n_rows)
    }

    pub(in crate::ui) fn content_height(
        &self,
        db: &reprise_core::db::Db,
        cache: &Cell<f64>,
        n_rows: usize,
        n_sections: usize,
    ) -> ContentHeight {
        let row_height = self.row_height(db, cache);
        let header_height = self
            .section_header_measurement()
            .filter(|measurement| measurement.is_uniform())
            .and_then(RowMeasurement::modal);
        content_height(n_rows, n_sections, row_height, header_height)
    }

    pub(in crate::ui) fn configure(
        &self,
        adjustment: &gtk4::Adjustment,
        target: f64,
        db: &reprise_core::db::Db,
        cache: &Cell<f64>,
        n_rows: usize,
        n_sections: usize,
    ) -> bool {
        let ContentHeight::Known(upper) = self.content_height(db, cache, n_rows, n_sections) else {
            return false;
        };
        crate::ui::scroll_probe::probe_upper("anchor.configure", adjustment, upper);
        adjustment.configure(
            target,
            adjustment.lower(),
            upper,
            adjustment.step_increment(),
            adjustment.page_increment(),
            adjustment.page_size(),
        );
        true
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

    #[test]
    fn sectioned_geometry_needs_independent_row_and_header_measurements() {
        let rows = RowMeasurement::from_widget_heights([34, 34, 34]);
        let headers = RowMeasurement::from_widget_heights([20, 20]);

        assert_eq!(
            settled_content_row_height(3_440.0, 100, 2, rows, Some(headers)),
            RowHeight::new(34.0)
        );
        assert_eq!(
            settled_content_row_height(3_440.0, 100, 2, rows, None),
            None
        );
        assert_eq!(
            settled_content_row_height(
                3_440.0,
                100,
                2,
                rows,
                Some(RowMeasurement::from_widget_heights([19, 20])),
            ),
            None
        );
    }

    #[test]
    fn persisted_row_heights_are_independent_per_density_and_discardable() {
        use reprise_core::library::settings::{self, ListDensity};

        let db = reprise_core::db::Db::open_in_memory().unwrap();
        assert_eq!(
            settings::get_row_height(&db, ListDensity::Standard).unwrap(),
            None
        );

        settings::set_row_height(&db, ListDensity::Standard, Some(34.0)).unwrap();
        settings::set_row_height(&db, ListDensity::Compact, Some(26.0)).unwrap();
        assert_eq!(
            settings::get_row_height(&db, ListDensity::Standard).unwrap(),
            RowHeight::new(34.0).map(RowHeight::pixels)
        );
        assert_eq!(
            settings::get_row_height(&db, ListDensity::Compact).unwrap(),
            Some(26.0)
        );

        settings::set_row_height(&db, ListDensity::Standard, None).unwrap();
        assert_eq!(
            settings::get_row_height(&db, ListDensity::Standard).unwrap(),
            None
        );
        assert_eq!(
            settings::get_row_height(&db, ListDensity::Compact).unwrap(),
            Some(26.0)
        );
    }

    #[test]
    fn density_change_discards_that_density_before_using_its_token_floor() {
        use std::cell::Cell;

        use reprise_core::library::settings::{self, ListDensity};

        let db = reprise_core::db::Db::open_in_memory().unwrap();
        settings::set_row_height(&db, ListDensity::Standard, Some(34.0)).unwrap();
        let cache = Cell::new(INVALIDATED_ROW_HEIGHT);

        let loaded = load_row_height(
            &db,
            ListDensity::Standard,
            &cache,
            RowHeight::new(28.0).unwrap(),
        );

        assert_eq!(loaded, RowHeight::new(28.0).unwrap());
        assert_eq!(cache.get(), 28.0);
        assert_eq!(
            settings::get_row_height(&db, ListDensity::Standard).unwrap(),
            None
        );
    }

    #[test]
    fn changed_subscription_callback_can_only_be_taken_once() {
        let callback = OneShot::new(|| 42);

        assert_eq!(callback.take().map(|callback| callback()), Some(42));
        assert!(callback.take().is_none());
    }
}
