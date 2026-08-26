use super::*;

#[test]
fn an_upper_is_gtk_authored_before_the_cache_has_written_anything() {
    let cache = ListGeometryCache::default();

    assert!(gtk_authored(&cache, 60_180.0, 2_006));
}

#[test]
fn only_a_changed_upper_is_gtk_authored_for_the_recorded_row_count() {
    let cache = ListGeometryCache::default();
    record_configured_upper(&cache, 2_006, 60_180.0);

    assert!(!gtk_authored(&cache, 60_180.0, 2_006));
    assert!(gtk_authored(&cache, 90_270.0, 2_006));
}

#[test]
fn a_recorded_upper_for_another_row_count_is_gtk_authored() {
    let cache = ListGeometryCache::default();
    record_configured_upper(&cache, 2_006, 60_180.0);

    assert!(gtk_authored(&cache, 60_180.0, 500));
}

#[test]
fn an_unclaimed_measured_range_is_still_preseeded() {
    let cache = ListGeometryCache::default();
    let wanted = ContentHeight::Known(60_180.0);

    assert_eq!(
        preseed_unclaimed_upper(&cache, 748.0, wanted, RowHeightSource::Measured, 2_006,),
        Some(60_180.0)
    );
}

#[test]
fn a_gtk_authored_range_is_never_preseeded() {
    let cache = ListGeometryCache::default();
    record_configured_upper(&cache, 2_006, 60_180.0);
    let wanted = ContentHeight::Known(60_180.0);

    assert_eq!(
        preseed_unclaimed_upper(&cache, 90_270.0, wanted, RowHeightSource::Measured, 2_006,),
        None
    );
}

#[test]
fn an_old_models_range_can_seed_the_new_row_count() {
    let cache = ListGeometryCache::default();
    record_configured_upper(&cache, 2_006, 60_180.0);

    assert_eq!(
        preseed_unclaimed_upper(
            &cache,
            60_180.0,
            ContentHeight::Known(15_000.0),
            RowHeightSource::Measured,
            500,
        ),
        Some(15_000.0)
    );
}

#[test]
fn layout_uses_the_gtk_quotient_once_the_range_changes() {
    let cache = ListGeometryCache::default();
    record_configured_upper(&cache, 2_006, 60_180.0);
    let remembered = RowHeight::new(30.0).unwrap();

    assert_eq!(
        authoritative_row_height(&cache, 90_270.0, 2_006, 0, remembered, None),
        RowHeight::new(45.0).unwrap()
    );
}

#[test]
fn layout_keeps_the_remembered_height_while_the_range_is_ours() {
    let cache = ListGeometryCache::default();
    record_configured_upper(&cache, 2_006, 60_180.0);
    let remembered = RowHeight::new(30.0).unwrap();

    assert_eq!(
        authoritative_row_height(&cache, 60_180.0, 2_006, 0, remembered, None),
        remembered
    );
}

#[test]
fn a_widget_height_that_disagrees_with_gtk_persists_nothing() {
    let cache = ListGeometryCache::default();
    cache.seed_measured_row_height(30.0);
    record_configured_upper(&cache, 100, 3_000.0);
    let rows = RowMeasurement::from_widget_heights([34, 34, 34]);

    assert_eq!(
        persistable_row_height(&cache, 5_300.0, 100, 0, rows, None),
        None
    );
    assert_eq!(cache.row_height.get(), 30.0);
}

#[test]
fn rows_allocated_below_their_natural_height_are_not_evidence() {
    let measurement = RowMeasurement::from_widget_measurements([(30, 31), (30, 31), (30, 31)]);

    assert_eq!(measurement.modal(), None);
    assert!(!measurement.is_uniform());
}

#[test]
fn rows_that_reached_their_natural_height_supply_the_modal() {
    let measurement = RowMeasurement::from_widget_measurements([(45, 31), (45, 31), (45, 31)]);

    assert_eq!(measurement.modal(), RowHeight::new(45.0));
    assert!(measurement.is_uniform());
}

#[test]
fn mixed_finished_rows_remain_non_uniform() {
    let measurement =
        RowMeasurement::from_widget_measurements([(30, 31), (45, 31), (45, 31), (46, 31)]);

    assert!(!measurement.is_uniform());
}

#[test]
fn three_finished_rows_are_the_minimum_settled_sample() {
    let two = RowMeasurement::from_widget_measurements([(45, 31), (45, 31)]);
    let three = RowMeasurement::from_widget_measurements([(45, 31), (45, 31), (45, 31)]);

    assert_eq!(two.modal(), None);
    assert!(!two.is_uniform());
    assert_eq!(three.modal(), RowHeight::new(45.0));
    assert!(three.is_uniform());
}

#[test]
fn the_minimum_sample_counts_only_finished_rows() {
    let measurements = std::iter::repeat_n((30, 31), 200)
        .chain([(45, 31), (45, 31)])
        .collect::<Vec<_>>();

    let measurement = RowMeasurement::from_widget_measurements(measurements);

    assert_eq!(measurement.modal(), None);
    assert!(!measurement.is_uniform());
}
