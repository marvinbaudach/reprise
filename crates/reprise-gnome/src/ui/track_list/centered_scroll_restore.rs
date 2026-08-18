//! Centering the playing track after a model reload.
//!
//! Two things happen on this occasion, and only the first belongs here:
//! resolving *which* row, and then moving the viewport onto it. The move is
//! `track_reveal::reveal_position` — the same function the jump path uses, at
//! the motion this occasion needs.
//!
//! It used to be a second implementation of that move, and the copy differed
//! in exactly one respect: what it did when the rebuilt list had no settled
//! geometry yet. It registered two refinements and then snapped the row to the
//! nearest viewport edge, so a restore that could not centre on its first try
//! moved the list twice — once to the edge, once to the centre. That second
//! move is the hop SEARCH-16 now rules out. The floor the snap provided is
//! kept, behind the attempts instead of in front of them.
//!
//! ## Why this occasion does not claim `track_reveal_pending`
//!
//! The jump path sets that marker before it yields, so a reload landing in the
//! same main-loop turn can see the viewport is already spoken for and anchor
//! on the reveal's destination rather than on the frame it is passing through
//! (`track_list_reload::capture_reload_anchor`). The marker exists for reloads
//! to read. This *is* the reload, so claiming it would only make the next
//! capture anchor on a centering this same reload started — a reload waiting
//! on itself.

use std::rc::Rc;

use super::track_reveal::RevealMotion;
use super::Shared;
use crate::ui::adjustment_hold::AdjustmentHold;

/// Same budget as the jump path (`current_track_selection`): enough idle
/// rounds for a rebuilt list to allocate, and few enough that a list which
/// never settles reaches its visibility floor promptly.
const RESTORE_ATTEMPTS: u8 = 8;

pub(super) fn schedule(
    shared: &Rc<Shared>,
    track_id: Option<i64>,
    current_ids: Vec<i64>,
    hold: Option<AdjustmentHold>,
) {
    let anchor = track_id.map(|track_id| (track_id, 0.0));
    let Some(position) = super::reload_restore::prepaint_position(anchor, &current_ids) else {
        return;
    };
    super::track_reveal::reveal_position(
        shared,
        position,
        RESTORE_ATTEMPTS,
        RevealMotion::Instant,
        hold,
    );
}
