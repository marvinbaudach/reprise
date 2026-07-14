//! Keeps the unified filter bar's compact result count aligned with the
//! exact `TrackListModel` query without expanding the already large track
//! list composition module.

use std::cell::RefCell;
use std::rc::Rc;

use reprise_core::queries::{self, BrowseFilter};
use reprise_core::view_source::ViewSource;
use rusqlite::Connection;

use super::browse_bar::BrowseBar;

pub(super) fn update(
    bar: &Rc<BrowseBar>,
    conn: &Rc<RefCell<Connection>>,
    source: &ViewSource,
    count: usize,
    has_filter: bool,
) {
    if !matches!(source, ViewSource::Library) {
        bar.hide_result_count();
        return;
    }
    let total = if has_filter {
        let conn = conn.borrow();
        queries::query_track_count_browsed(
            &conn,
            &ViewSource::Library,
            "",
            &BrowseFilter::default(),
            &[],
        )
        .and_then(|value| {
            usize::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(0, value))
        })
    } else {
        Ok(count)
    };
    match total {
        Ok(total) => bar.set_result_count(count, total),
        Err(error) => {
            tracing::warn!(%error, "could not load total count for filter bar");
            bar.hide_result_count();
        }
    }
}
