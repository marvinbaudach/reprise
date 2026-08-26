//! View-neutral row geometry for virtualized GTK list widgets.
//!
//! A model swap can pair the new row count with the old adjustment for one
//! allocation frame. The adjustment quotient is therefore trusted only when
//! it agrees with an independently measured, uniform set of bound row widgets.

use std::cell::Cell;
use std::collections::BTreeMap;

use gtk4::glib::prelude::{Cast, ObjectExt};
use gtk4::prelude::{AdjustmentExt, ScrollableExt, WidgetExt};
use reprise_core::library::settings;

pub(in crate::ui) use crate::ui::list_geometry_content::{
    content_height, rows_content_height, sectioned_content_height, ContentHeight,
};
use crate::ui::list_geometry_layout::ListLayout;

pub(in crate::ui) const ROW_HEIGHT_AGREEMENT_EPSILON: f64 = 0.5;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum RowHeightSource {
    Assumed,
    Measured,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) struct TrustedRowHeight {
    pub(in crate::ui) height: RowHeight,
    pub(in crate::ui) source: RowHeightSource,
}

impl TrustedRowHeight {
    pub(in crate::ui) fn assumed(height: RowHeight) -> Self {
        Self {
            height,
            source: RowHeightSource::Assumed,
        }
    }

    pub(in crate::ui) fn measured(height: RowHeight) -> Self {
        Self {
            height,
            source: RowHeightSource::Measured,
        }
    }

    pub(in crate::ui) fn from_cache(value: f64) -> Option<Self> {
        if value == 0.0 {
            return None;
        }
        let source = if value.is_sign_negative() {
            RowHeightSource::Assumed
        } else {
            RowHeightSource::Measured
        };
        RowHeight::new(value.abs()).map(|height| Self { height, source })
    }

    pub(in crate::ui) fn cache_value(self) -> f64 {
        match self.source {
            RowHeightSource::Assumed => -self.height.pixels(),
            RowHeightSource::Measured => self.height.pixels(),
        }
    }
}

pub(in crate::ui) fn remember_preferred_height(cache: &Cell<f64>, candidate: TrustedRowHeight) {
    let existing = TrustedRowHeight::from_cache(cache.get());
    if existing.is_some_and(|height| {
        height.source == RowHeightSource::Measured && candidate.source == RowHeightSource::Assumed
    }) {
        return;
    }
    cache.set(candidate.cache_value());
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(in crate::ui) struct RowMeasurement {
    modal: Option<RowHeight>,
    uniform: bool,
}

const MIN_SETTLED_ROW_SAMPLE: usize = 3;

impl RowMeasurement {
    /// Measures the unique most frequent non-zero allocated widget height.
    /// Zero-height widgets are unrealized and do not describe a bound row.
    pub(in crate::ui) fn from_widget_heights(heights: impl IntoIterator<Item = i32>) -> Self {
        Self::from_height_samples(heights.into_iter().filter(|height| *height > 0), 1)
    }

    /// Measures rows only after their allocation has reached the height the
    /// widget naturally requests. A settled virtualized list exposes dozens;
    /// fewer than three survivors are too little evidence for the whole list.
    pub(in crate::ui) fn from_widget_measurements(
        measurements: impl IntoIterator<Item = (i32, i32)>,
    ) -> Self {
        Self::from_height_samples(
            measurements.into_iter().filter_map(|(allocated, natural)| {
                (allocated > 0 && allocated >= natural).then_some(allocated)
            }),
            MIN_SETTLED_ROW_SAMPLE,
        )
    }

    fn from_height_samples(heights: impl IntoIterator<Item = i32>, minimum_sample: usize) -> Self {
        let mut counts = BTreeMap::<i32, usize>::new();
        let mut sample_size = 0;
        for height in heights {
            *counts.entry(height).or_default() += 1;
            sample_size += 1;
        }
        let max_count = counts.values().copied().max();
        let modes = counts
            .iter()
            .filter(|(_, count)| Some(**count) == max_count)
            .map(|(height, _)| *height)
            .collect::<Vec<_>>();
        Self {
            modal: (sample_size >= minimum_sample && modes.len() == 1)
                .then(|| RowHeight::new(f64::from(modes[0])))
                .flatten(),
            uniform: sample_size >= minimum_sample && counts.len() == 1,
        }
    }

    pub(in crate::ui) fn from_widget_heights_at_least(
        heights: impl IntoIterator<Item = i32>,
        minimum: RowHeight,
    ) -> Self {
        Self::from_widget_heights(
            heights
                .into_iter()
                .filter(|height| f64::from(*height) >= minimum.pixels()),
        )
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

fn persistable_row_height(
    cache: &ListGeometryCache,
    upper: f64,
    n_rows: usize,
    n_sections: usize,
    rows: RowMeasurement,
    headers: Option<RowMeasurement>,
) -> Option<RowHeight> {
    gtk_authored(cache, upper, n_rows)
        .then(|| settled_content_row_height(upper, n_rows, n_sections, rows, headers))
        .flatten()
}

#[cfg(test)]
pub(in crate::ui) fn adjustment_row_height(upper: f64, n_rows: usize) -> Option<RowHeight> {
    if n_rows == 0 {
        return None;
    }
    RowHeight::new(upper * (n_rows as f64).recip())
}

#[cfg(test)]
pub(in crate::ui) fn is_settled(upper: f64, n_rows: usize, measurement: RowMeasurement) -> bool {
    settled_row_height(upper, n_rows, measurement).is_some()
}

fn trusted_content_height(
    n_rows: usize,
    n_sections: usize,
    row_height: TrustedRowHeight,
    section_header_height: Option<TrustedRowHeight>,
) -> (ContentHeight, RowHeightSource) {
    let header_height = section_header_height.map(|header| header.height);
    let content = content_height(n_rows, n_sections, row_height.height, header_height);
    let source = if row_height.source == RowHeightSource::Assumed
        || section_header_height.is_some_and(|header| header.source == RowHeightSource::Assumed)
    {
        RowHeightSource::Assumed
    } else {
        RowHeightSource::Measured
    };
    (content, source)
}

fn preseed_upper(
    current_upper: f64,
    content: ContentHeight,
    source: RowHeightSource,
) -> Option<f64> {
    let ContentHeight::Known(wanted_upper) = content else {
        return None;
    };
    let current_describes_geometry = match source {
        // A CSS token is only a lower bound on a complete row. A larger live
        // range may already include GTK's row chrome and must never be shrunk
        // back to the token-derived estimate.
        RowHeightSource::Assumed => {
            current_upper.is_finite()
                && current_upper + ROW_HEIGHT_AGREEMENT_EPSILON >= wanted_upper
        }
        RowHeightSource::Measured => {
            current_upper.is_finite()
                && (current_upper - wanted_upper).abs() < ROW_HEIGHT_AGREEMENT_EPSILON
        }
    };
    (!current_describes_geometry).then_some(wanted_upper)
}

fn preseed_unclaimed_upper(
    cache: &ListGeometryCache,
    current_upper: f64,
    content: ContentHeight,
    source: RowHeightSource,
    n_rows: usize,
) -> Option<f64> {
    let wanted_upper = preseed_upper(current_upper, content, source)?;
    match cache.configured_upper.get() {
        // Before GTK has described a complete range, a cold-start seed may
        // grow its default range. It may never shrink an externally authored
        // range, which is the poisoned-cache loop this guard exists to stop.
        None => (!current_upper.is_finite()
            || wanted_upper > current_upper + ROW_HEIGHT_AGREEMENT_EPSILON)
            .then_some(wanted_upper),
        // The live range still describes the old model. Seeding the new model
        // is the legitimate model-swap case the cache's row count preserves.
        Some((written_rows, _)) if written_rows != n_rows => Some(wanted_upper),
        Some(_) => (!gtk_authored(cache, current_upper, n_rows)).then_some(wanted_upper),
    }
}

#[derive(Default)]
pub(in crate::ui) struct ListGeometryCache {
    row_height: Cell<f64>,
    section_header_height: Cell<f64>,
    configured_upper: Cell<Option<(usize, f64)>>,
}

pub(in crate::ui) fn record_configured_upper(cache: &ListGeometryCache, n_rows: usize, upper: f64) {
    cache.configured_upper.set(Some((n_rows, upper)));
}

/// Reports whether the live range came from outside this geometry service for
/// the current model. A write for another row count cannot describe this one.
pub(in crate::ui) fn gtk_authored(cache: &ListGeometryCache, upper: f64, n_rows: usize) -> bool {
    cache
        .configured_upper
        .get()
        .is_none_or(|(written_rows, written_upper)| {
            written_rows != n_rows || (upper - written_upper).abs() > ROW_HEIGHT_AGREEMENT_EPSILON
        })
}

fn authoritative_row_height(
    cache: &ListGeometryCache,
    upper: f64,
    n_rows: usize,
    n_sections: usize,
    remembered: RowHeight,
    section_header_height: Option<RowHeight>,
) -> RowHeight {
    if !gtk_authored(cache, upper, n_rows) || n_rows == 0 {
        return remembered;
    }
    let header_band =
        section_header_height.map_or(0.0, |height| n_sections as f64 * height.pixels());
    RowHeight::new((upper - header_band) / n_rows as f64)
        // A quotient below the authored row minimum can only describe stale
        // transitional range geometry, not the current model's row pitch.
        .filter(|height| height.pixels() >= f64::from(crate::ui::style::tokens::ROW_MIN_HEIGHT))
        .unwrap_or(remembered)
}

#[cfg(test)]
impl ListGeometryCache {
    pub(in crate::ui) fn seed_measured_row_height(&self, height: f64) {
        self.row_height.set(height);
    }
}

/// The one cache-then-persistence load, shared by row and section-header
/// geometry: check the cache, fall back to the persisted measurement, and
/// finally to `minimum` as an *assumed* height, remembering the result.
/// `load` reads the persisted value — the only thing that differs between the
/// two kinds of height. Keeping this sequence in one place is deliberate: the
/// same decision living in two functions is how this codebase has produced
/// drift before.
pub(in crate::ui) fn load_trusted_height(
    cache: &Cell<f64>,
    minimum: RowHeight,
    load: impl FnOnce() -> Option<f64>,
) -> TrustedRowHeight {
    if let Some(cached) = TrustedRowHeight::from_cache(cache.get()) {
        return cached;
    }
    let loaded = load().and_then(RowHeight::new).map_or_else(
        || TrustedRowHeight::assumed(minimum),
        TrustedRowHeight::measured,
    );
    remember_preferred_height(cache, loaded);
    TrustedRowHeight::from_cache(cache.get()).unwrap_or(loaded)
}

fn load_row_height(db: &reprise_core::db::Db, cache: &Cell<f64>, minimum: RowHeight) -> RowHeight {
    load_trusted_height(cache, minimum, || {
        settings::get_row_height(db).unwrap_or_else(|error| {
            tracing::warn!(%error, "could not load persisted row height");
            None
        })
    })
    .height
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
        RowMeasurement::from_widget_measurements(self.widget_measurements("ColumnViewRow"))
    }

    fn widget_measurements(&self, type_fragment: &str) -> Vec<(i32, i32)> {
        fn collect(widget: &gtk4::Widget, type_fragment: &str, heights: &mut Vec<(i32, i32)>) {
            let is_data_row = type_fragment != "ColumnViewRow" || widget.css_name() == "row";
            if is_data_row && widget.type_().name().contains(type_fragment) {
                let (minimum, natural, _, _) = widget.measure(gtk4::Orientation::Vertical, -1);
                heights.push((
                    widget.height(),
                    if natural == 0 { minimum } else { natural },
                ));
            }
            let mut child = widget.first_child();
            while let Some(current) = child {
                collect(&current, type_fragment, heights);
                child = current.next_sibling();
            }
        }

        let mut heights = Vec::new();
        collect(self.view.upcast_ref(), type_fragment, &mut heights);
        heights
    }

    fn section_header_measurement(&self) -> Option<RowMeasurement> {
        let measurement = crate::ui::list_geometry_header::measurement_from_widget_heights(
            self.widget_measurements("ListHeader")
                .into_iter()
                .map(|(allocated, _)| allocated),
        );
        measurement.modal().map(|_| measurement)
    }

    pub(in crate::ui) fn settled_row_height(&self, upper: f64, n_rows: usize) -> Option<RowHeight> {
        settled_row_height(upper, n_rows, self.measurement())
    }

    /// Real widget realization is the only signal that distinguishes a pre-seeded `upper` from settled geometry.
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

    fn minimum_row_height(&self) -> RowHeight {
        RowHeight::new(f64::from(crate::ui::style::tokens::ROW_MIN_HEIGHT))
            .expect("the authored row minimum is positive")
    }

    pub(in crate::ui) fn row_height(
        &self,
        db: &reprise_core::db::Db,
        cache: &ListGeometryCache,
    ) -> RowHeight {
        load_row_height(db, &cache.row_height, self.minimum_row_height())
    }

    pub(in crate::ui) fn section_header_height(
        &self,
        db: &reprise_core::db::Db,
        cache: &ListGeometryCache,
    ) -> RowHeight {
        crate::ui::list_geometry_header::load_height(db, &cache.section_header_height).height
    }

    pub(in crate::ui) fn layout(
        &self,
        db: &reprise_core::db::Db,
        cache: &ListGeometryCache,
        remembered_row_height: RowHeight,
        section_starts: Vec<u32>,
        upper: f64,
        n_rows: usize,
    ) -> ListLayout {
        let section_header_height =
            (!section_starts.is_empty()).then(|| self.section_header_height(db, cache));
        let row_height = authoritative_row_height(
            cache,
            upper,
            n_rows,
            section_starts.len(),
            remembered_row_height,
            section_header_height,
        );
        if section_starts.is_empty() {
            ListLayout::rows_only(row_height)
        } else {
            ListLayout::sectioned(
                row_height,
                section_header_height.expect("sectioned layout has a header height"),
                section_starts,
            )
        }
    }

    fn trusted_row_height(
        &self,
        db: &reprise_core::db::Db,
        cache: &ListGeometryCache,
    ) -> TrustedRowHeight {
        let height = self.row_height(db, cache);
        TrustedRowHeight::from_cache(cache.row_height.get())
            .unwrap_or_else(|| TrustedRowHeight::assumed(height))
    }

    pub(in crate::ui) fn remember_if_settled(
        &self,
        db: &reprise_core::db::Db,
        cache: &ListGeometryCache,
        upper: f64,
        n_rows: usize,
        n_sections: usize,
    ) -> bool {
        let row_measurement = self.measurement();
        let header_measurement = self.section_header_measurement();
        let height = persistable_row_height(
            cache,
            upper,
            n_rows,
            n_sections,
            row_measurement,
            header_measurement,
        );
        let Some(height) = height else {
            return false;
        };
        if n_sections == 0 {
            remember_preferred_height(&cache.row_height, TrustedRowHeight::measured(height));
            if let Err(error) = settings::set_row_height(db, Some(height.pixels())) {
                tracing::warn!(%error, "could not persist settled row height");
            }
            return true;
        }
        let Some(header_height) =
            header_measurement.and_then(crate::ui::list_geometry_header::measured_height)
        else {
            return false;
        };
        crate::ui::list_geometry_header::remember_settled_heights(
            db,
            &cache.row_height,
            &cache.section_header_height,
            height,
            header_height,
        );
        true
    }

    pub(in crate::ui) fn observed_row_height(
        &self,
        db: &reprise_core::db::Db,
        cache: &ListGeometryCache,
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

    pub(in crate::ui) fn content_height(
        &self,
        db: &reprise_core::db::Db,
        cache: &ListGeometryCache,
        n_rows: usize,
        n_sections: usize,
    ) -> (ContentHeight, RowHeightSource, Option<RowHeightSource>) {
        let row_height = self.trusted_row_height(db, cache);
        let header_height = (n_sections > 0).then(|| {
            crate::ui::list_geometry_header::load_height(db, &cache.section_header_height)
        });
        let header_source = header_height.map(|header| header.source);
        let (content, source) =
            trusted_content_height(n_rows, n_sections, row_height, header_height);
        (content, source, header_source)
    }

    pub(in crate::ui) fn configure(
        &self,
        adjustment: &gtk4::Adjustment,
        target: f64,
        db: &reprise_core::db::Db,
        cache: &ListGeometryCache,
        n_rows: usize,
        n_sections: usize,
    ) -> bool {
        self.remember_if_settled(db, cache, adjustment.upper(), n_rows, n_sections);
        let (content, source, header_source) = self.content_height(db, cache, n_rows, n_sections);
        if n_sections > 0 {
            crate::ui::scroll_probe::probe_preseed_source(&format!("{header_source:?}"));
        }
        let ContentHeight::Known(wanted_upper) = content else {
            return false;
        };
        let Some(upper) =
            preseed_unclaimed_upper(cache, adjustment.upper(), content, source, n_rows)
        else {
            // An exact no-op is the range this configure request accepted.
            // Keeping its row count lets the next model swap distinguish that
            // old-model range without changing the live adjustment here.
            if (adjustment.upper() - wanted_upper).abs() < ROW_HEIGHT_AGREEMENT_EPSILON {
                record_configured_upper(cache, n_rows, wanted_upper);
            }
            return true;
        };
        // `adjustment.configure` re-enters GTK's layout when it runs inside an
        // allocation-time `changed` emission — see [`in_changed_emission`].
        // Callers reached from a `changed` handler must defer instead.
        debug_assert!(
            !crate::ui::list_geometry_changed::in_changed_emission(),
            "list geometry configured the adjustment from inside a changed emission"
        );
        crate::ui::scroll_probe::probe_upper("anchor.configure", adjustment, upper);
        record_configured_upper(cache, n_rows, upper);
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
#[path = "list_geometry_cache_tests.rs"]
mod cache_tests;

#[cfg(test)]
#[path = "list_geometry_acceptance_tests.rs"]
mod acceptance_tests;

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
    fn token_floor_filters_recycled_rows_without_becoming_the_measurement() {
        let minimum = RowHeight::new(28.0).unwrap();
        let measurement = RowMeasurement::from_widget_heights_at_least([0, 25, 25, 34], minimum);

        assert_eq!(measurement.modal(), RowHeight::new(34.0));
        assert!(measurement.is_uniform());
        assert_eq!(
            settled_row_height(2_276.0 * 34.0, 2_276, measurement),
            RowHeight::new(34.0)
        );
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
    fn a_persisted_row_height_round_trips_and_can_be_cleared() {
        let db = reprise_core::db::Db::open_in_memory().unwrap();
        assert_eq!(settings::get_row_height(&db).unwrap(), None);

        settings::set_row_height(&db, Some(34.0)).unwrap();
        assert_eq!(
            settings::get_row_height(&db).unwrap(),
            RowHeight::new(34.0).map(RowHeight::pixels)
        );

        settings::set_row_height(&db, None).unwrap();
        assert_eq!(settings::get_row_height(&db).unwrap(), None);
    }

    #[test]
    fn persisted_measurement_outranks_the_token_floor_on_cold_start() {
        let db = reprise_core::db::Db::open_in_memory().unwrap();
        settings::set_row_height(&db, Some(34.0)).unwrap();
        let cache = Cell::new(0.0);

        let loaded = load_row_height(&db, &cache, RowHeight::new(28.0).unwrap());

        assert_eq!(loaded, RowHeight::new(34.0).unwrap());
        assert_eq!(cache.get(), 34.0);
    }

    #[test]
    fn assumed_height_cannot_displace_a_measured_height() {
        let cache = Cell::new(34.0);

        remember_preferred_height(
            &cache,
            TrustedRowHeight::assumed(RowHeight::new(28.0).unwrap()),
        );

        assert_eq!(cache.get(), 34.0);
    }

    #[test]
    fn assumed_preseed_only_grows_a_range_below_its_lower_bound() {
        let assumed = TrustedRowHeight::assumed(RowHeight::new(28.0).unwrap());
        let wanted = ContentHeight::Known(63_728.0);

        assert_eq!(preseed_upper(748.0, wanted, assumed.source), Some(63_728.0));
        assert_eq!(preseed_upper(77_384.0, wanted, assumed.source), None);
    }

    #[test]
    fn measured_preseed_only_skips_the_range_it_measured() {
        let measured = TrustedRowHeight::measured(RowHeight::new(34.0).unwrap());
        let wanted = ContentHeight::Known(77_384.0);

        assert_eq!(
            preseed_upper(748.0, wanted, measured.source),
            Some(77_384.0)
        );
        assert_eq!(preseed_upper(77_384.0, wanted, measured.source), None);
    }
}
