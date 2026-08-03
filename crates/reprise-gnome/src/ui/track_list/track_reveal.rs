//! Deferred, generation-guarded centering for loaded-track changes.

use std::rc::Rc;

use gtk4::prelude::*;

use super::current_track_selection::{CurrentTrackChange, USER_SCROLL_GRACE};
use super::Shared;

pub(super) fn defer(
    shared: &Rc<Shared>,
    track_id: i64,
    position: u32,
    change: CurrentTrackChange,
    generation: u64,
    attempts: u8,
) {
    let shared = Rc::downgrade(shared);
    gtk4::glib::idle_add_local_once(move || {
        if let Some(shared) = shared.upgrade() {
            reveal(&shared, track_id, position, change, generation, attempts);
        }
    });
}

fn reveal(
    shared: &Rc<Shared>,
    track_id: i64,
    position: u32,
    change: CurrentTrackChange,
    generation: u64,
    attempts: u8,
) {
    if shared.track_reveal_generation.get() != generation {
        tracing::debug!(track_id, position, "superseded track reveal skipped");
        return;
    }
    if change == CurrentTrackChange::AutomaticAdvance
        && shared
            .last_scroll_activity
            .get()
            .is_some_and(|last| last.elapsed() < USER_SCROLL_GRACE)
    {
        tracing::debug!(
            position,
            "automatic track centering suppressed by scroll activity"
        );
        return;
    }

    let n_rows = shared
        .column_view
        .model()
        .map_or(0, |model| model.n_items());
    if let Some(adjustment) = shared.column_view.vadjustment() {
        if let Some(value) = centered_reveal_value(
            generation,
            shared.track_reveal_generation.get(),
            position,
            n_rows,
            adjustment.upper(),
            adjustment.page_size(),
        ) {
            adjustment.set_value(value);
            tracing::info!(track_id, position, ?change, "current track centered");
            return;
        }
    }
    if attempts == 0 {
        return;
    }
    let shared = Rc::downgrade(shared);
    gtk4::glib::idle_add_local_once(move || {
        if let Some(shared) = shared.upgrade() {
            reveal(
                &shared,
                track_id,
                position,
                change,
                generation,
                attempts - 1,
            );
        }
    });
}

fn centered_reveal_value(
    request_generation: u64,
    current_generation: u64,
    position: u32,
    n_rows: u32,
    upper: f64,
    page_size: f64,
) -> Option<f64> {
    (request_generation == current_generation)
        .then(|| {
            crate::ui::scroll_center::centered_scroll_value(position, n_rows, upper, page_size)
        })
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nav_10a_superseded_reveal_has_no_center_target() {
        assert_eq!(
            centered_reveal_value(7, 7, 50, 100, 1_000.0, 200.0),
            Some(405.0)
        );
        assert_eq!(centered_reveal_value(7, 8, 50, 100, 1_000.0, 200.0), None);
    }
}
