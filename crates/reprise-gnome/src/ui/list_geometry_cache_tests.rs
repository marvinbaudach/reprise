use super::*;

#[test]
fn section_content_uses_the_weaker_of_row_and_header_sources() {
    let measured_row = TrustedRowHeight::measured(RowHeight::new(34.0).unwrap());
    let assumed_row = TrustedRowHeight::assumed(RowHeight::new(28.0).unwrap());
    let measured_header = TrustedRowHeight::measured(RowHeight::new(38.0).unwrap());
    let assumed_header = TrustedRowHeight::assumed(RowHeight::new(36.0).unwrap());

    assert_eq!(
        trusted_content_height(100, 2, measured_row, Some(measured_header)),
        (ContentHeight::Known(3_476.0), RowHeightSource::Measured)
    );
    assert_eq!(
        trusted_content_height(100, 2, measured_row, Some(assumed_header)),
        (ContentHeight::Known(3_472.0), RowHeightSource::Assumed)
    );
    assert_eq!(
        trusted_content_height(100, 2, assumed_row, Some(measured_header)),
        (ContentHeight::Known(2_876.0), RowHeightSource::Assumed)
    );
}

#[test]
fn list_geometry_cache_invalidates_rows_and_section_headers_together() {
    let cache = ListGeometryCache::default();
    cache.row_height.set(34.0);
    cache.section_header_height.set(38.0);

    cache.invalidate();

    assert_eq!(cache.row_height.get(), INVALIDATED_ROW_HEIGHT);
    assert_eq!(cache.section_header_height.get(), INVALIDATED_ROW_HEIGHT);
}
