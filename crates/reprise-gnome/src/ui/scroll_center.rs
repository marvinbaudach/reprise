//! Shared vertical-centering wiring for `GtkColumnView`.
//!
//! Both the track table (`track_list::current_track_selection`) and the Artists
//! master list (`library_views::artist_master`) center a selected row by writing
//! the vertical adjustment directly — a plain `scroll_to` only edge-snaps. The
//! geometry belongs to `ListLayout`; this module only resolves the adjustment.

use gtk4::prelude::*;

use crate::ui::list_geometry_layout::ListLayout;

pub(in crate::ui) enum CenteringRequest {
    Layout {
        position: u32,
        layout: ListLayout,
    },
    /// Compatibility for unchanged flat-list display tests outside this
    /// strand's file ownership. The calculation still delegates to
    /// `ListLayout`; there is no second rows-only centering model.
    #[cfg(test)]
    SettledRowsOnly {
        position: u32,
    },
}

impl From<(u32, ListLayout)> for CenteringRequest {
    fn from((position, layout): (u32, ListLayout)) -> Self {
        Self::Layout { position, layout }
    }
}

#[cfg(test)]
impl From<u32> for CenteringRequest {
    fn from(position: u32) -> Self {
        Self::SettledRowsOnly { position }
    }
}

/// Resolves the vertical adjustment and the value that vertically centers row
/// `position` from the caller's complete `ListLayout`. Returns `None` when the
/// list has no usable geometry yet or it fits the viewport entirely.
pub(in crate::ui) fn centered_scroll_target(
    column_view: &gtk4::ColumnView,
    n_rows: u32,
    request: impl Into<CenteringRequest>,
) -> Option<(gtk4::Adjustment, f64)> {
    let adjustment = column_view.vadjustment()?;
    let value = match request.into() {
        CenteringRequest::Layout { position, layout } => {
            layout.centered_value(position, n_rows as usize, adjustment.page_size())?
        }
        #[cfg(test)]
        CenteringRequest::SettledRowsOnly { position } => {
            let row_height = crate::ui::list_geometry::ListGeometry::for_view(column_view)
                .settled_row_height(adjustment.upper(), n_rows as usize)?;
            ListLayout::rows_only(row_height).centered_value(
                position,
                n_rows as usize,
                adjustment.page_size(),
            )?
        }
    };
    Some((adjustment, value))
}

/// Flat-list adapter retained only for pre-existing tests outside this
/// strand's ownership. It delegates to `ListLayout` and contains no centering
/// arithmetic of its own.
#[cfg(test)]
pub(in crate::ui) fn centered_scroll_value(
    position: u32,
    n_rows: u32,
    content_height: f64,
    page_size: f64,
) -> Option<f64> {
    let row_height =
        crate::ui::list_geometry::adjustment_row_height(content_height, n_rows as usize)?;
    ListLayout::rows_only(row_height).centered_value(position, n_rows as usize, page_size)
}
