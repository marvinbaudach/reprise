use std::rc::Rc;

use crate::ui::list_geometry::{self, RowHeight};

/// One tolerance owns both observed-height inference and scroll adoption.
/// `row_top` rounds the inferred components separately, unlike the former
/// fused calculation, so the final match tolerance must never drift below the
/// inference floor that absorbs that ULP-scale difference.
pub(in crate::ui) const CONTENT_HEIGHT_EPSILON: f64 = 0.5;

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
    starts: Rc<[u32]>,
}

/// Counts the section headers at or above `position`.
///
/// `starts` must be **strictly ascending**. Duplicates would be counted twice
/// and shift every row top below them by one header height. The invariant holds
/// by construction: `compose_virtual` (`reprise-view/src/queue.rs:284-311`)
/// pushes each section at `items.len()` behind a non-emptiness guard, so every
/// section contributes at least one row before the next start is taken. The one
/// theoretical violation is the `u32::try_from(...).unwrap_or(u32::MAX)`
/// saturation at `:296` and `:305`, which needs more than `u32::MAX` queue rows.
///
/// The counting itself does not depend on ordering -- the assert is deliberately
/// stricter than the arithmetic needs, because ascending order is what the
/// producer actually guarantees and a break in it is a real upstream bug.
pub(in crate::ui) fn headers_above_in(starts: &[u32], position: u32) -> usize {
    debug_assert!(
        starts.windows(2).all(|pair| pair[0] < pair[1]),
        "section starts must be strictly ascending, got {starts:?}"
    );
    starts.iter().filter(|start| **start <= position).count()
}

/// Content-space geometry of a list that may carry section headers: the one
/// place that knows a row's top edge is
/// `position * row_height + headers_above(position) * section_header_height`.
///
/// Deliberately GTK-free. Both centering and anchor restoration use this
/// complete row-and-header model; GTK-facing code only supplies measurements
/// and writes the resulting adjustment value.
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
            starts: starts.into(),
        });
        Self {
            row_height,
            sections,
        }
    }

    /// Infers uniform section-header height from a live content `upper`.
    ///
    /// The scroll-adoption path reaches this only while no settled layout is
    /// available, so it knows the row height and section starts but not a
    /// trustworthy header height. This is the inverse of [`Self::sectioned`]:
    /// it assigns the observed height left after all row bodies evenly across
    /// the sections. A sub-epsilon shortfall retains the old zero-header
    /// interpretation; inputs that cannot describe that layout are rejected.
    pub(in crate::ui) fn sectioned_inferred_from_observed_upper(
        row_height: RowHeight,
        starts: Rc<[u32]>,
        row_count: usize,
        observed_upper: f64,
    ) -> Option<Self> {
        let section_count = starts.len();
        if row_count == 0 || section_count == 0 || !observed_upper.is_finite() {
            return None;
        }

        let row_content_height = row_count as f64 * row_height.pixels();
        let section_content_height = observed_upper - row_content_height;
        if section_content_height < -CONTENT_HEIGHT_EPSILON {
            return None;
        }
        let header_height = section_content_height.max(0.0) / section_count as f64;
        let Some(header_height) = RowHeight::new(header_height) else {
            return Some(Self::rows_only(row_height));
        };
        Some(Self {
            row_height,
            sections: Some(SectionBands {
                header_height,
                starts,
            }),
        })
    }

    pub(in crate::ui) fn infer_section_header_from_observed_upper(
        &self,
        row_count: usize,
        observed_upper: f64,
    ) -> Option<Self> {
        let starts = Rc::clone(&self.sections.as_ref()?.starts);
        Self::sectioned_inferred_from_observed_upper(
            self.row_height,
            starts,
            row_count,
            observed_upper,
        )
    }

    pub(in crate::ui) fn row_height(&self) -> RowHeight {
        self.row_height
    }

    pub(in crate::ui) fn has_sections(&self) -> bool {
        self.sections.is_some()
    }

    pub(in crate::ui) fn section_count(&self) -> usize {
        self.sections
            .as_ref()
            .map_or(0, |sections| sections.starts.len())
    }

    pub(in crate::ui) fn headers_above(&self, position: u32) -> usize {
        self.sections
            .as_ref()
            .map_or(0, |sections| headers_above_in(&sections.starts, position))
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

    /// Returns the adjustment value that puts the exact middle of `position`
    /// at the viewport middle, including every section header above the row.
    ///
    /// The stale-position rejection is load-bearing: clamping an index from a
    /// replaced model into the new scroll range would otherwise return a
    /// plausible target for an unrelated row.
    pub(in crate::ui) fn centered_value(
        &self,
        position: u32,
        n_rows: usize,
        page_size: f64,
    ) -> Option<f64> {
        let content_height = self.content_height(n_rows);
        if n_rows == 0 || page_size <= 0.0 || content_height <= page_size {
            return None;
        }
        if usize::try_from(position).ok()? >= n_rows {
            return None;
        }
        let target = self.row_top(position) + self.row_height.pixels() / 2.0 - page_size / 2.0;
        Some(target.clamp(0.0, self.max_scroll(n_rows, page_size)))
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

    use super::{headers_above_in, LayoutValidation, ListLayout};

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
    fn layout_and_bare_section_starts_count_the_same_headers() {
        let starts = vec![0, 12, 40];
        let position = 39;
        let layout = ListLayout::sectioned(height(34.0), height(36.0), starts.clone());

        assert_eq!(layout.headers_above(position), 2);
        assert_eq!(
            layout.headers_above(position),
            headers_above_in(&starts, position)
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "strictly ascending")]
    fn duplicated_section_start_trips_the_invariant() {
        headers_above_in(&[0, 12, 12], 40);
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

    #[test]
    fn observed_upper_infers_the_header_height_inverse_of_sectioned() {
        let layout = ListLayout::sectioned_inferred_from_observed_upper(
            height(34.5),
            vec![0, 1].into(),
            2_276,
            78_594.0,
        )
        .unwrap();

        assert_eq!(layout.row_top(1_101), 38_056.5);
        assert_eq!(layout.content_height(2_276), 78_594.0);
    }

    #[test]
    fn observed_upper_rejects_degenerate_section_layouts() {
        assert!(ListLayout::sectioned_inferred_from_observed_upper(
            height(34.0),
            vec![0].into(),
            0,
            36.0,
        )
        .is_none());
        assert!(ListLayout::sectioned_inferred_from_observed_upper(
            height(34.0),
            Vec::<u32>::new().into(),
            1,
            34.0,
        )
        .is_none());
        assert!(ListLayout::sectioned_inferred_from_observed_upper(
            height(34.0),
            vec![0].into(),
            1,
            f64::NAN,
        )
        .is_none());
        assert!(ListLayout::sectioned_inferred_from_observed_upper(
            height(10.0),
            vec![0].into(),
            10,
            99.49,
        )
        .is_none());

        let within_epsilon = ListLayout::sectioned_inferred_from_observed_upper(
            height(10.0),
            vec![0].into(),
            10,
            99.75,
        )
        .unwrap();
        assert_eq!(within_epsilon.row_top(5), 50.0);
    }

    #[test]
    fn centered_value_preserves_the_flat_list_contract() {
        let layout = ListLayout::rows_only(height(10.0));

        assert_eq!(layout.centered_value(50, 100, 200.0), Some(405.0));
        assert_eq!(layout.centered_value(0, 100, 200.0), Some(0.0));
        assert_eq!(layout.centered_value(99, 100, 200.0), Some(800.0));
        assert_eq!(layout.centered_value(5, 100, 0.0), None);
        assert_eq!(layout.centered_value(5, 10, 200.0), None);
        assert_eq!(layout.centered_value(0, 0, 200.0), None);
        assert_eq!(layout.centered_value(42, 30, 200.0), None);
        assert_eq!(layout.centered_value(29, 30, 200.0), Some(100.0));
    }

    #[test]
    fn centered_value_counts_every_header_above_the_row() {
        let rows_only = ListLayout::rows_only(height(10.0));
        let sectioned = layout(10.0, 36.0, &[0, 20, 40]);
        let flat = rows_only.centered_value(50, 100, 200.0).unwrap();
        let with_headers = sectioned.centered_value(50, 100, 200.0).unwrap();

        assert_eq!(with_headers, 513.0);
        assert_eq!(with_headers - flat, 3.0 * 36.0);
    }
}
