//! Geometry guard that lets a provisional row-only scroll target adopt GTK's
//! own settled `value-changed` reading once section headers resolve.

use std::rc::Rc;

use crate::ui::list_geometry_layout::{ListLayout, CONTENT_HEIGHT_EPSILON};

#[derive(Clone)]
pub(super) struct ScrollAdoptionGeometry {
    guard_position: u32,
    row_count: usize,
    layout: Rc<ListLayout>,
    pub(super) before: f64,
}

impl ScrollAdoptionGeometry {
    pub(super) fn new(
        guard_position: u32,
        row_count: usize,
        expected_section_count: usize,
        layout: Rc<ListLayout>,
        before: f64,
    ) -> Option<Self> {
        if row_count == 0
            || expected_section_count == 0
            || layout.section_count() != expected_section_count
            || layout.headers_above(guard_position) > expected_section_count
            || guard_position as usize >= row_count
        {
            return None;
        }
        Some(Self {
            guard_position,
            row_count,
            layout,
            before,
        })
    }

    pub(super) fn matches(&self, candidate: f64, lower: f64, upper: f64, page_size: f64) -> bool {
        if !candidate.is_finite()
            || !self.before.is_finite()
            || !lower.is_finite()
            || !upper.is_finite()
            || !page_size.is_finite()
            || upper < lower
            || page_size < 0.0
        {
            return false;
        }

        let Some(layout) = self
            .layout
            .infer_section_header_from_observed_upper(self.row_count, upper)
        else {
            return false;
        };
        let guard_top = layout.row_top(self.guard_position);
        let requested = guard_top.clamp(lower, (upper - page_size).max(lower));
        let candidate_error = (candidate - requested).abs();
        let before_error = (self.before - requested).abs();
        candidate_error <= CONTENT_HEIGHT_EPSILON && candidate_error < before_error
    }
}
