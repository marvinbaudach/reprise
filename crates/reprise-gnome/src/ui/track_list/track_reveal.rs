//! Deferred, generation-guarded centering for loaded-track changes.
//!
//! The centering scroll is the expensive half of a track change: a distant
//! `GtkAdjustment` write makes GtkColumnView rebind a screenful of rows before
//! it returns. Running it in the same main-loop turn as the player-bar update
//! kept the whole window from painting for the duration, so it is deferred to a
//! later turn — GTK's redraw sits at a higher priority than
//! `idle_add_local_once`, so the new track reaches the screen first.
//!
//! Deferring means the request outlives the state it was born from, and there
//! are two independent ways it can go stale:
//!
//! - a **newer track change** supersedes it — covered by
//!   `Shared::track_reveal_generation`;
//! - the **view itself** changes (a filter edit, a re-sort, a source switch, a
//!   reload after a scan or a tag save) while the request waits or retries.
//!   The generation token says nothing about that, so the row index is
//!   re-resolved from the track id on every attempt rather than carried across
//!   the yield. A carried index cannot be validated after the fact: the
//!   centering arithmetic clamps into range, so a stale row reads as a
//!   perfectly ordinary answer and silently scrolls somewhere else.

use std::rc::Rc;

use gtk4::prelude::*;

use super::current_track_selection::{
    visible_position_for_track_in_source, CurrentTrackChange, USER_SCROLL_GRACE,
};
use super::Shared;
use reprise_core::view_source::ViewSource;

pub(super) fn defer(
    shared: &Rc<Shared>,
    track_id: i64,
    queue_position: Option<usize>,
    change: CurrentTrackChange,
    generation: u64,
    attempts: u8,
) {
    let shared = Rc::downgrade(shared);
    gtk4::glib::idle_add_local_once(move || {
        if let Some(shared) = shared.upgrade() {
            reveal(
                &shared,
                track_id,
                queue_position,
                change,
                generation,
                attempts,
            );
        }
    });
}

fn reveal(
    shared: &Rc<Shared>,
    track_id: i64,
    queue_position: Option<usize>,
    change: CurrentTrackChange,
    generation: u64,
    attempts: u8,
) {
    if shared.track_reveal_generation.get() != generation {
        tracing::debug!(track_id, "superseded track reveal skipped");
        return;
    }
    if change == CurrentTrackChange::AutomaticAdvance
        && shared
            .last_scroll_activity
            .get()
            .is_some_and(|last| last.elapsed() < USER_SCROLL_GRACE)
    {
        tracing::debug!(
            track_id,
            "automatic track centering suppressed by scroll activity"
        );
        return;
    }

    // Resolve the row against the view as it stands *now*, not as it stood when
    // this reveal was scheduled — see the module comment.
    let ids = shared.current_view_ids();
    let is_queue = matches!(*shared.source.borrow(), ViewSource::Queue);
    let Some(position) =
        visible_position_for_track_in_source(&ids, track_id, queue_position, is_queue)
    else {
        tracing::debug!(
            track_id,
            "loaded track left the visible query before its reveal ran"
        );
        return;
    };

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
                queue_position,
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

    // NAV-10a: a pending reveal must follow the track, not the row number it
    // happened to sit on. The generation token cannot see a filter change, so
    // this is what keeps the deferred scroll honest.
    #[test]
    fn nav_10a_reveal_follows_the_track_when_the_view_changes_underneath() {
        let track = 4242;
        let before: Vec<i64> = (0..100).map(|i| if i == 42 { track } else { i }).collect();
        let after: Vec<i64> = (0..30).map(|i| if i == 5 { track } else { 900 + i }).collect();

        assert_eq!(
            visible_position_for_track_in_source(&before, track, None, false),
            Some(42)
        );
        // Re-resolving against the shortened view finds the real row.
        assert_eq!(
            visible_position_for_track_in_source(&after, track, None, false),
            Some(5)
        );
        // Carrying the old index instead would have centered row 42 of a
        // 30-row list; the bound check in `centered_scroll_value` is the second
        // line of defence and refuses it outright.
        assert_eq!(
            centered_reveal_value(1, 1, 42, 30, 300.0, 200.0),
            None,
            "a row the list no longer has must not produce a scroll target"
        );
    }

    // The track can also leave the view entirely (filtered out). Then there is
    // nothing to center and the reveal must simply drop, leaving whatever
    // scroll restoration the filter change itself performed (FIL-9) intact.
    #[test]
    fn fil_9_reveal_drops_when_the_track_left_the_view() {
        let after: Vec<i64> = (0..30).collect();
        assert_eq!(
            visible_position_for_track_in_source(&after, 4242, None, false),
            None
        );
    }
}
