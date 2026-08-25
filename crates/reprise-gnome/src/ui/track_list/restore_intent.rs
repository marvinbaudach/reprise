//! Arbitration between a deliberate reveal and later restore writers.

use gtk4::prelude::AdjustmentExt;

use super::{diagnostic_trail, Shared};
use crate::ui::list_geometry::RowHeight;

const DESTINATION_EPSILON: f64 = 0.5;

pub(super) fn deliberate_destination_outranks(
    shared: &Shared,
    adjustment: &gtk4::Adjustment,
    row_height: RowHeight,
    writer: &str,
    rejected: f64,
) -> bool {
    let Some(destination) = shared.scroll_glide.deliberate_destination() else {
        return false;
    };
    let half_row = row_height.pixels() / 2.0;
    if (adjustment.value() - destination).abs() > half_row
        || (rejected - destination).abs() <= DESTINATION_EPSILON
    {
        return false;
    }
    shared
        .diagnostic_trail
        .record(diagnostic_trail::Event::ScrollRestoreStandDown {
            writer: writer.to_owned(),
            destination,
            rejected,
        });
    true
}
