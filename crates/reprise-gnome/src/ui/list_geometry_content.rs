use crate::ui::list_geometry::RowHeight;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) enum ContentHeight {
    Known(f64),
    Unknown,
}

pub(in crate::ui) fn content_height(
    n_rows: usize,
    n_sections: usize,
    row_height: RowHeight,
    section_header_height: Option<RowHeight>,
) -> ContentHeight {
    if n_sections == 0 {
        return ContentHeight::Known(rows_content_height(n_rows, row_height));
    }
    section_header_height.map_or(ContentHeight::Unknown, |header| {
        ContentHeight::Known(sectioned_content_height(
            n_rows, n_sections, row_height, header,
        ))
    })
}

pub(in crate::ui) fn rows_content_height(n_rows: usize, row_height: RowHeight) -> f64 {
    n_rows as f64 * row_height.pixels()
}

pub(in crate::ui) fn sectioned_content_height(
    n_rows: usize,
    n_sections: usize,
    row_height: RowHeight,
    header: RowHeight,
) -> f64 {
    (n_sections as f64).mul_add(header.pixels(), rows_content_height(n_rows, row_height))
}
