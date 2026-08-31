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

use super::centered_scroll_restore::Centering;
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
    // Claimed synchronously, before yielding: a reload in this same main-loop
    // turn has to see that the viewport is already spoken for.
    shared.track_reveal_pending.set(true);
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
        record_reveal(shared, track_id, None, change, "superseded");
        tracing::debug!(track_id, "superseded track reveal skipped");
        return;
    }
    if change == CurrentTrackChange::AutomaticAdvance
        && shared
            .last_scroll_activity
            .get()
            .is_some_and(|last| last.elapsed() < USER_SCROLL_GRACE)
    {
        record_reveal(shared, track_id, None, change, "suppressed-by-scroll");
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
        record_reveal(shared, track_id, None, change, "left-view");
        tracing::debug!(
            track_id,
            "loaded track left the visible query before its reveal ran"
        );
        return;
    };

    if reveal_position(shared, position, attempts, RevealMotion::Glide) {
        record_reveal(shared, track_id, Some(position), change, "centered");
        tracing::info!(track_id, position, ?change, "current track centered");
        return;
    }
    if attempts == 0 {
        record_reveal(shared, track_id, Some(position), change, "no-geometry");
        return;
    }
    record_reveal(shared, track_id, Some(position), change, "retry");
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

/// Every attempt ends here, which makes it the one place that can honestly
/// close the claim `defer` opened: it outlives this attempt only when another
/// one is queued behind it. Past that, a reveal that reached its target is
/// represented by the glide it started (`ScrollGlide::destination`).
fn record_reveal(
    shared: &Shared,
    track_id: i64,
    position: Option<u32>,
    change: CurrentTrackChange,
    outcome: &str,
) {
    shared.track_reveal_pending.set(outcome == "retry");
    shared
        .diagnostic_trail
        .record(super::diagnostic_trail::Event::Reveal {
            track_id,
            position,
            change: format!("{change:?}").to_lowercase(),
            outcome: outcome.to_owned(),
        });
}

/// How the viewport travels to the row.
///
/// The two occasions that centre a row want opposite things, and the
/// difference is not taste. A track change happens under a list that stays
/// put, so movement is what tells the user the table followed the music, and a
/// scroll of the user's own can take the viewport back mid-flight. An occasion
/// that follows a *model swap* happens under a list that was just replaced;
/// there visible travel is not a reveal but the hop SEARCH-16 forbids, so the
/// destination is written in one step — and the geometry behind it is seeded
/// rather than awaited (`centered_scroll_restore::write_centered`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum RevealMotion {
    Glide,
    Instant,
}

/// Brings row `position` to the vertical centre. Retries on its own while the
/// list has no usable geometry yet, and falls back to plain visibility once
/// the attempts are spent without a single write.
///
/// Split out from [`reveal`] because the two answer different questions:
/// *which* row (resolved fresh against the live view, see the module comment)
/// and *how the viewport gets there*. Returns whether the viewport is settled
/// — centred, or already showing everything there is to show; `false` means a
/// retry was queued or the attempts ran out.
///
/// Both centring occasions end here, and that is the point. They used to be
/// two implementations of the same arithmetic that differed only in what they
/// did when the geometry was not ready: this one retried, the reload path
/// snapped the row to the viewport edge *first* and refined afterwards. That
/// snap is why clearing a search moved the list twice. The floor it provided
/// survives, behind the attempts instead of in front of them, and the jump
/// path — which had no floor at all and simply gave up — now shares it.
pub(super) fn reveal_position(
    shared: &Rc<Shared>,
    position: u32,
    attempts: u8,
    motion: RevealMotion,
) -> bool {
    let n_rows = shared
        .column_view
        .model()
        .map_or(0, |model| model.n_items());
    // Whether the row is on screen even though this attempt did not finish —
    // which decides whether the floor below is a rescue or a step backwards.
    let placed_provisionally = match motion {
        RevealMotion::Glide => {
            let layout = super::track_list_geometry::layout(shared, None, n_rows as usize);
            if let Some((adjustment, value)) = layout.and_then(|layout| {
                crate::ui::scroll_center::centered_scroll_target(
                    &shared.column_view,
                    n_rows,
                    (position, layout),
                )
            }) {
                shared.scroll_glide.glide_to(&adjustment, value);
                return true;
            }
            false
        }
        RevealMotion::Instant => {
            match super::centered_scroll_restore::write_centered(shared, position, n_rows) {
                Centering::NothingToCenter | Centering::Settled => return true,
                // Written, but against a row height the allocation has not
                // confirmed. Re-running once it has costs nothing when the
                // prediction held — the same value is written again and the
                // viewport does not move.
                Centering::Predicted => true,
                Centering::Unavailable => false,
            }
        }
    };
    if attempts == 0 {
        // Only when nothing was ever placed. A predicted centring that ran out
        // of rounds is off by the error in a remembered row height; snapping it
        // to the viewport edge would trade that for a whole screen.
        if !placed_provisionally {
            ensure_visible(shared, position);
        }
        return false;
    }
    let generation = shared.model.generation();
    let view = shared.column_view.clone();
    let shared = Rc::downgrade(shared);
    view.add_tick_callback(move |_, _| {
        let Some(shared) = shared.upgrade() else {
            return gtk4::glib::ControlFlow::Break;
        };
        // A model swap invalidates the row index this attempt carries, and the
        // centering arithmetic cannot tell a stale index from a live one — it
        // clamps into range and answers plausibly. The jump path re-resolves
        // per round in [`reveal`]; here the honest answer is to stop, because
        // whoever replaced the model owns the viewport now.
        if shared.model.generation() != generation {
            return gtk4::glib::ControlFlow::Break;
        }
        reveal_position(&shared, position, attempts - 1, motion);
        gtk4::glib::ControlFlow::Break
    });
    false
}

/// Last resort: the geometry never settled, so centring is impossible — but
/// the promise that the row ends up on screen still stands.
///
/// This is the same `scroll_to` the reload path used to fire *before* it tried
/// to centre, which is what made the viewport move twice. Behind the attempts
/// it can only run when centring genuinely failed, so it is a floor rather
/// than a first move.
fn ensure_visible(shared: &Shared, position: u32) {
    scroll_into_view(shared, position, "centered.reveal.scroll_to");
}

/// Puts GTK's own list anchor on `position` without moving a viewport that
/// already shows the row where it wants it.
///
/// The same call as [`ensure_visible`], asked for a different reason and at a
/// different moment — see the ordering note in
/// `centered_scroll_restore::write_centered`, which is the only caller.
pub(super) fn anchor_view_on(shared: &Shared, position: u32) {
    scroll_into_view(shared, position, "centered.reveal.anchor");
}

fn scroll_into_view(shared: &Shared, position: u32, writer: &str) {
    if let Some(adjustment) = shared.column_view.vadjustment() {
        crate::ui::scroll_probe::probe_scroll_to(writer, &adjustment, position);
    }
    let scroll = gtk4::ScrollInfo::new();
    scroll.set_enable_vertical(true);
    shared
        .column_view
        .scroll_to(position, None, gtk4::ListScrollFlags::NONE, Some(scroll));
}

#[cfg(test)]
mod tests {
    use super::*;

    // NAV-10b: a pending reveal must follow the track, not the row number it
    // happened to sit on. The generation token cannot see a filter change, so
    // this is what keeps the deferred scroll honest.
    #[test]
    fn nav_10b_reveal_follows_the_track_when_the_view_changes_underneath() {
        let track = 4242;
        let before: Vec<i64> = (0..100).map(|i| if i == 42 { track } else { i }).collect();
        let after: Vec<i64> = (0..30)
            .map(|i| if i == 5 { track } else { 900 + i })
            .collect();

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
        // 30-row list; `ListLayout`'s bound check is the second
        // line of defence and refuses it outright.
        let layout = crate::ui::list_geometry_layout::ListLayout::rows_only(
            crate::ui::list_geometry::RowHeight::new(10.0).unwrap(),
        );
        assert_eq!(
            layout.centered_value(42, 30, 200.0),
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
