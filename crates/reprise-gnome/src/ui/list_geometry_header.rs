//! Section-header measurement and persisted geometry, independent of GTK.

use std::cell::Cell;

use reprise_core::library::settings;

use crate::ui::list_geometry::{
    load_trusted_height, remember_preferred_height, RowHeight, RowMeasurement, TrustedRowHeight,
};

fn minimum_height() -> RowHeight {
    RowHeight::new(f64::from(
        crate::ui::style::tokens::SECTION_HEADER_MIN_HEIGHT,
    ))
    .expect("the section-header minimum is positive")
}

pub(in crate::ui) fn measurement_from_widget_heights(
    heights: impl IntoIterator<Item = i32>,
) -> RowMeasurement {
    RowMeasurement::from_widget_heights_at_least(heights, minimum_height())
}

pub(in crate::ui) fn load_height(db: &reprise_core::db::Db, cache: &Cell<f64>) -> TrustedRowHeight {
    load_trusted_height(cache, minimum_height(), || {
        settings::get_section_header_height(db).unwrap_or_else(|error| {
            tracing::warn!(%error, "could not load persisted section-header height");
            None
        })
    })
}

pub(in crate::ui) fn measured_height(measurement: RowMeasurement) -> Option<RowHeight> {
    if !measurement.is_uniform() {
        return None;
    }
    measurement.modal()
}

pub(in crate::ui) fn remember_settled_heights(
    db: &reprise_core::db::Db,
    row_cache: &Cell<f64>,
    header_cache: &Cell<f64>,
    row_height: RowHeight,
    header_height: RowHeight,
) {
    if let Err(error) = settings::set_row_and_section_header_heights(
        db,
        row_height.pixels(),
        header_height.pixels(),
    ) {
        tracing::warn!(%error, "could not persist settled list geometry");
    }
    remember_preferred_height(row_cache, TrustedRowHeight::measured(row_height));
    remember_preferred_height(header_cache, TrustedRowHeight::measured(header_height));
}

#[cfg(test)]
mod tests {
    use crate::ui::list_geometry::RowHeightSource;

    use super::*;

    #[test]
    fn the_authored_floor_filters_unallocated_and_partial_headers() {
        let measurement = measurement_from_widget_heights([0, 20, 36, 36]);

        assert!(measurement.is_uniform());
        assert_eq!(measurement.modal(), RowHeight::new(36.0));
    }

    #[test]
    fn a_non_uniform_measurement_preserves_the_last_good_height() {
        let db = reprise_core::db::Db::open_in_memory().unwrap();
        let cache = Cell::new(0.0);
        settings::set_section_header_height(&db, Some(36.0)).unwrap();
        let trusted = load_height(&db, &cache);
        assert_eq!(trusted.source, RowHeightSource::Measured);

        assert_eq!(
            measured_height(measurement_from_widget_heights([36, 40])),
            None
        );
        assert_eq!(load_height(&db, &cache), trusted);
        assert_eq!(
            settings::get_section_header_height(&db).unwrap(),
            Some(36.0)
        );
    }

    #[test]
    fn cold_cache_uses_an_assumed_floor_until_a_measurement_is_remembered() {
        let db = reprise_core::db::Db::open_in_memory().unwrap();
        let cache = Cell::new(0.0);

        let cold = load_height(&db, &cache);
        assert_eq!(cold.source, RowHeightSource::Assumed);
        assert_eq!(cold.height, RowHeight::new(36.0).unwrap());

        assert_eq!(
            measured_height(measurement_from_widget_heights([38, 38])),
            RowHeight::new(38.0)
        );
        let row_cache = Cell::new(0.0);
        remember_settled_heights(
            &db,
            &row_cache,
            &cache,
            RowHeight::new(34.0).unwrap(),
            RowHeight::new(38.0).unwrap(),
        );
        let measured = load_height(&db, &cache);
        assert_eq!(measured.source, RowHeightSource::Measured);
        assert_eq!(measured.height, RowHeight::new(38.0).unwrap());
    }
}
