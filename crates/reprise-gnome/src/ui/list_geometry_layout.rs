use crate::ui::list_geometry::{self, RowHeight};

const CONTENT_HEIGHT_EPSILON: f64 = 0.5;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ui) enum LayoutValidation {
    Accepted,
    Rejected,
    NoOpinion,
}

/// The row positions that carry a section header, together with the height
/// every one of those headers has. Non-empty by construction: a layout with
/// no sections holds `None` instead.
///
/// `starts` is strictly ascending -- see `headers_above_in`.
#[derive(Clone, Debug, PartialEq)]
struct SectionBands {
    header_height: RowHeight,
    starts: Vec<u32>,
}

/// Content-space geometry of a list that may carry section headers: the one
/// place that knows a row's top edge is
/// `position * row_height + headers_above(position) * section_header_height`.
///
/// Deliberately GTK-free. `scroll_center::centered_scroll_value_with_height`
/// still models a list as rows only; it centres rather than anchors and is
/// tracked separately -- it is the last remaining copy of this model.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::ui) struct ListLayout {
    row_height: RowHeight,
    sections: Option<SectionBands>,
}

impl ListLayout {
    pub(in crate::ui) fn rows_only(row_height: RowHeight) -> Self {
        Self {
            row_height,
            sections: None,
        }
    }

    pub(in crate::ui) fn sectioned(
        row_height: RowHeight,
        header_height: RowHeight,
        starts: Vec<u32>,
    ) -> Self {
        let sections = (!starts.is_empty()).then_some(SectionBands {
            header_height,
            starts,
        });
        Self {
            row_height,
            sections,
        }
    }

    pub(in crate::ui) fn row_height(&self) -> RowHeight {
        self.row_height
    }

    pub(in crate::ui) fn has_sections(&self) -> bool {
        self.sections.is_some()
    }

    pub(in crate::ui) fn headers_above(&self, position: u32) -> usize {
        self.sections.as_ref().map_or(0, |sections| {
            sections
                .starts
                .iter()
                .filter(|start| **start <= position)
                .count()
        })
    }

    pub(in crate::ui) fn row_top(&self, position: u32) -> f64 {
        let rows = f64::from(position) * self.row_height.pixels();
        let headers = self.sections.as_ref().map_or(0.0, |sections| {
            self.headers_above(position) as f64 * sections.header_height.pixels()
        });
        rows + headers
    }

    /// Returns the largest row position whose top is at or above `content_y`
    /// and the distance from that top to `content_y`.
    ///
    /// The result is intentionally not clamped to a model length. Inside a
    /// header band, the preceding row owns an offset larger than one row; a
    /// value above row zero owns a negative offset.
    pub(in crate::ui) fn row_at(&self, content_y: f64) -> (u32, f64) {
        let first_top = self.row_top(0);
        if content_y < first_top || content_y.is_nan() {
            return (0, content_y - first_top);
        }

        let upper = (content_y / self.row_height.pixels())
            .floor()
            .clamp(0.0, f64::from(u32::MAX)) as u32;
        if self.row_top(upper) <= content_y {
            return (upper, content_y - self.row_top(upper));
        }

        let mut lower = 0_u32;
        let mut upper = upper;
        while lower + 1 < upper {
            let middle = lower + (upper - lower) / 2;
            if self.row_top(middle) <= content_y {
                lower = middle;
            } else {
                upper = middle;
            }
        }
        (lower, content_y - self.row_top(lower))
    }

    /// Returns the last row whose top edge lies strictly above `content_y`.
    pub(in crate::ui) fn last_row_above(&self, content_y: f64) -> Option<u32> {
        let (position, offset) = self.row_at(content_y);
        if offset > 0.0 {
            Some(position)
        } else {
            position.checked_sub(1)
        }
    }

    /// Delegates the complete row-and-header equation to `list_geometry`.
    pub(in crate::ui) fn content_height(&self, n_rows: usize) -> f64 {
        match &self.sections {
            Some(sections) => list_geometry::sectioned_content_height(
                n_rows,
                sections.starts.len(),
                self.row_height,
                sections.header_height,
            ),
            None => list_geometry::rows_content_height(n_rows, self.row_height),
        }
    }

    pub(in crate::ui) fn max_scroll(&self, n_rows: usize, viewport_height: f64) -> f64 {
        (self.content_height(n_rows) - viewport_height).max(0.0)
    }

    /// Whether a live allocation can judge this layout's assumed header height.
    ///
    /// A non-positive or non-finite `upper` is not evidence. Neither is an
    /// allocation shorter than the known row bodies, which cannot describe
    /// this model, nor one below the predicted content height, because GTK's
    /// range can still grow while section headers settle. Once `upper` reaches
    /// or exceeds the prediction, an excess beyond the sub-pixel tolerance is
    /// proof that the assumed header height disagrees with the allocation.
    pub(in crate::ui) fn validate(&self, n_rows: usize, upper: f64) -> LayoutValidation {
        if !upper.is_finite() || upper <= 0.0 {
            return LayoutValidation::NoOpinion;
        }
        let content_height = self.content_height(n_rows);
        let rows_height = n_rows as f64 * self.row_height.pixels();
        if upper + CONTENT_HEIGHT_EPSILON < rows_height
            || upper + CONTENT_HEIGHT_EPSILON < content_height
        {
            return LayoutValidation::NoOpinion;
        }
        if (content_height - upper).abs() < CONTENT_HEIGHT_EPSILON {
            LayoutValidation::Accepted
        } else {
            LayoutValidation::Rejected
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ui::list_geometry::RowHeight;

    use super::{LayoutValidation, ListLayout};

    fn height(pixels: f64) -> RowHeight {
        RowHeight::new(pixels).unwrap()
    }

    fn layout(row: f64, header: f64, starts: &[u32]) -> ListLayout {
        ListLayout::sectioned(height(row), height(header), starts.to_vec())
    }

    #[test]
    fn headers_above_counts_each_start_across_distinct_layouts() {
        let rows_only = ListLayout::rows_only(height(34.0));
        assert_eq!(rows_only.headers_above(0), 0);
        assert_eq!(rows_only.headers_above(40), 0);

        let one = layout(34.0, 36.0, &[0]);
        assert_eq!(one.headers_above(0), 1);
        assert_eq!(one.headers_above(40), 1);

        let queue = layout(34.0, 36.0, &[0, 1]);
        assert_eq!(queue.headers_above(0), 1);
        assert_eq!(queue.headers_above(1), 2);
        assert_eq!(queue.headers_above(40), 2);

        let three = layout(34.0, 36.0, &[0, 12, 40]);
        assert_eq!(three.headers_above(0), 1);
        assert_eq!(three.headers_above(11), 1);
        assert_eq!(three.headers_above(12), 2);
        assert_eq!(three.headers_above(39), 2);
        assert_eq!(three.headers_above(40), 3);
        assert_eq!(three.headers_above(99), 3);
    }

    #[test]
    fn row_top_uses_the_configured_header_height() {
        let tall = layout(34.0, 36.0, &[0, 12]);
        let short = layout(34.0, 20.0, &[0, 12]);

        assert_eq!(tall.row_top(12), 480.0);
        assert_eq!(short.row_top(12), 448.0);
        assert_eq!(tall.row_top(12) - short.row_top(12), 32.0);
    }

    #[test]
    fn row_zero_begins_after_its_section_header() {
        assert_eq!(ListLayout::rows_only(height(34.0)).row_top(0), 0.0);
        assert_eq!(layout(34.0, 36.0, &[0]).row_top(0), 36.0);
    }

    #[test]
    fn row_at_round_trips_every_row_top_in_small_models() {
        let layouts = [
            ListLayout::rows_only(height(34.0)),
            layout(34.0, 36.0, &[0]),
            layout(34.0, 36.0, &[0, 1]),
            layout(34.0, 20.0, &[0, 3, 7]),
        ];

        for layout in layouts {
            for position in 0..10 {
                assert_eq!(
                    layout.row_at(layout.row_top(position)),
                    (position, 0.0),
                    "position {position} did not round trip"
                );
            }
        }
    }

    #[test]
    fn row_at_reconstructs_content_positions_inside_headers_and_rows() {
        let layout = layout(34.0, 36.0, &[0, 3]);
        let samples = [
            -5.0, 0.0, 35.0, 36.0, 69.0, 70.0, 137.0, 138.0, 173.0, 174.0,
        ];

        for content_y in samples {
            let (position, offset) = layout.row_at(content_y);
            assert_eq!(
                layout.row_top(position) + offset,
                content_y,
                "content y {content_y} did not reconstruct"
            );
        }

        assert_eq!(layout.row_at(0.0), (0, -36.0));
        assert_eq!(layout.row_at(150.0), (2, 46.0));
    }

    #[test]
    fn last_row_above_preserves_exact_row_boundaries() {
        let layout = ListLayout::rows_only(height(34.0));

        assert_eq!(layout.last_row_above(0.0), None);
        assert_eq!(layout.last_row_above(34.0), Some(0));
        assert_eq!(layout.last_row_above(34.5), Some(1));
        assert_eq!(layout.last_row_above(68.0), Some(1));
        assert_eq!(layout.last_row_above(68.5), Some(2));
    }

    #[test]
    fn content_height_delegates_the_complete_queue_geometry() {
        let layout = layout(34.0, 36.0, &[0, 1]);

        assert_eq!(layout.content_height(2_276), 77_456.0);
        assert_eq!(layout.max_scroll(2_276, 249.0), 77_207.0);
    }

    #[test]
    fn validate_accepts_the_queue_allocation_and_rejects_a_wrong_header_guess() {
        let layout = layout(34.0, 36.0, &[0, 1]);

        assert_eq!(layout.validate(2_276, 77_456.0), LayoutValidation::Accepted);
        assert_eq!(layout.validate(2_276, 77_464.0), LayoutValidation::Rejected);
    }

    #[test]
    fn validate_has_no_opinion_about_unsettled_or_foreign_allocations() {
        let layout = layout(34.0, 36.0, &[0, 1]);

        assert_eq!(
            layout.validate(2_276, 77_438.0),
            LayoutValidation::NoOpinion
        );
        assert_eq!(layout.validate(2_276, 748.0), LayoutValidation::NoOpinion);
        assert_eq!(
            layout.validate(2_276, f64::NAN),
            LayoutValidation::NoOpinion
        );
        assert_eq!(layout.validate(2_276, 0.0), LayoutValidation::NoOpinion);
    }

    #[test]
    fn rows_only_is_exactly_the_previous_arithmetic() {
        let layout = ListLayout::rows_only(height(20.0));

        assert_eq!(layout.row_top(3), 60.0);
        assert_eq!(layout.row_at(66.0), (3, 6.0));
        assert_eq!(layout.content_height(5), 100.0);
    }

    #[test]
    fn sectioned_with_no_starts_is_identical_to_rows_only() {
        let sectioned = ListLayout::sectioned(height(34.0), height(36.0), vec![]);
        let rows_only = ListLayout::rows_only(height(34.0));

        assert_eq!(sectioned.row_top(12), rows_only.row_top(12));
        assert_eq!(sectioned.content_height(100), rows_only.content_height(100));
        assert_eq!(sectioned.has_sections(), rows_only.has_sections());
    }
}
