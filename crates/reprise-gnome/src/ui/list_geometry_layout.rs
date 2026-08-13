use crate::ui::list_geometry::{self, ContentHeight, RowHeight};

const CONTENT_HEIGHT_EPSILON: f64 = 0.5;

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
    section_header_height: Option<RowHeight>,
    section_starts: Vec<u32>,
}

impl ListLayout {
    /// Returns `None` when sections exist but no header height is known.
    pub(in crate::ui) fn new(
        row_height: RowHeight,
        section_header_height: Option<RowHeight>,
        section_starts: Vec<u32>,
    ) -> Option<Self> {
        if !section_starts.is_empty() && section_header_height.is_none() {
            return None;
        }
        Some(Self {
            row_height,
            section_header_height,
            section_starts,
        })
    }

    #[cfg(test)]
    pub(in crate::ui) fn rows_only(row_height: RowHeight) -> Self {
        Self {
            row_height,
            section_header_height: None,
            section_starts: Vec::new(),
        }
    }

    pub(in crate::ui) fn row_height(&self) -> RowHeight {
        self.row_height
    }

    pub(in crate::ui) fn headers_above(&self, position: u32) -> usize {
        self.section_starts
            .iter()
            .filter(|start| **start <= position)
            .count()
    }

    pub(in crate::ui) fn row_top(&self, position: u32) -> f64 {
        let rows = f64::from(position) * self.row_height.pixels();
        let headers = self.section_header_height.map_or(0.0, |height| {
            self.headers_above(position) as f64 * height.pixels()
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
    pub(in crate::ui) fn content_height(&self, n_rows: usize) -> Option<f64> {
        match list_geometry::content_height(
            n_rows,
            self.section_starts.len(),
            self.row_height,
            self.section_header_height,
        ) {
            ContentHeight::Known(height) => Some(height),
            ContentHeight::Unknown => None,
        }
    }

    pub(in crate::ui) fn max_scroll(&self, n_rows: usize, viewport_height: f64) -> Option<f64> {
        self.content_height(n_rows)
            .map(|height| (height - viewport_height).max(0.0))
    }

    /// Whether this layout agrees with the live allocation closely enough to
    /// write a scroll value. A wrong assumed header height must fail closed.
    pub(in crate::ui) fn validate(&self, n_rows: usize, upper: f64) -> bool {
        upper.is_finite()
            && self
                .content_height(n_rows)
                .is_some_and(|height| (height - upper).abs() < CONTENT_HEIGHT_EPSILON)
    }
}

#[cfg(test)]
mod tests {
    use crate::ui::list_geometry::RowHeight;

    use super::ListLayout;

    fn height(pixels: f64) -> RowHeight {
        RowHeight::new(pixels).unwrap()
    }

    fn layout(row: f64, header: f64, starts: &[u32]) -> ListLayout {
        ListLayout::new(height(row), Some(height(header)), starts.to_vec()).unwrap()
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

        assert_eq!(layout.content_height(2_276), Some(77_456.0));
        assert_eq!(layout.max_scroll(2_276, 249.0), Some(77_207.0));
    }

    #[test]
    fn validate_accepts_the_queue_allocation_and_rejects_a_wrong_header_guess() {
        let layout = layout(34.0, 36.0, &[0, 1]);

        assert!(layout.validate(2_276, 77_456.0));
        assert!(!layout.validate(2_276, 77_464.0));
    }

    #[test]
    fn rows_only_is_exactly_the_previous_arithmetic() {
        let layout = ListLayout::rows_only(height(20.0));

        assert_eq!(layout.row_top(3), 60.0);
        assert_eq!(layout.row_at(66.0), (3, 6.0));
        assert_eq!(layout.content_height(5), Some(100.0));
    }

    #[test]
    fn sectioned_layout_requires_a_header_height() {
        assert!(ListLayout::new(height(34.0), None, vec![0, 1]).is_none());
    }
}
