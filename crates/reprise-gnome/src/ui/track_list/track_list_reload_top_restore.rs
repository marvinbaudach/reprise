//! SEARCH-9's legacy top-of-list restore, extracted so its self-rescheduling
//! idle chain and generation guard stay next to each other instead of buried
//! in `track_list_reload.rs`'s reload orchestration.

use std::rc::Rc;

use gtk4::prelude::*;

use super::super::Shared;

/// How many idle rounds [`schedule_top_scroll_restore`] re-applies its zero.
/// It only has to outlast the one allocation that GTK's own scroll restore
/// rides in on, and every extra round is a round in which the loop cannot
/// tell a re-clamp from the user grabbing the scrollbar — and would snap a
/// deliberate scroll back to the top. Two rounds cover the allocation with
/// one to spare.
pub(super) const TOP_RESTORE_MAX_ATTEMPTS: u8 = 2;

/// SEARCH-9: puts the viewport at the top of a freshly filtered list, and keeps
/// it there.
///
/// A single write does not survive. `restore_reload_anchor` runs right after
/// the model swap, while the rebuilt `ColumnView` still carries the *old*
/// allocation; the allocation pass that follows restores GTK's own scroll
/// position — the pre-filter value, clamped to the new and usually much
/// shorter list. A display test caught exactly that: 486 instead of 0, 486
/// being the clamped remains of where the list stood before the query.
///
/// So the zero is re-applied across idle rounds, like the anchor restore next
/// door. This legacy SEARCH-9 path needs to outlast one allocation, not track a
/// moving target. It stops as soon as a round finds the value still at zero —
/// at that point nothing is writing against us any more.
///
/// A round also stands down the moment `shared.model.generation()` no longer
/// matches the reload that scheduled it: a later reload has already swapped
/// in its own destination (a centred reveal, another `Top`, an anchor
/// restore), and this chain writing zero over it would be a stale write, not
/// a legitimate re-apply. A display test caught exactly this outliving a
/// clear-search reveal under load — see `search_16_clearing_after_a_play_
/// reaches_the_track_in_one_step`.
pub(super) fn schedule_top_scroll_restore(
    shared: Rc<Shared>,
    scheduled_generation: u64,
    attempts: u8,
) {
    if shared.model.generation() != scheduled_generation {
        return;
    }
    let Some(adjustment) = gtk4::prelude::ScrollableExt::vadjustment(&shared.column_view) else {
        return;
    };
    let already_settled = adjustment.value() == 0.0;
    crate::ui::scroll_probe::probe("top_restore", &adjustment, 0.0);
    adjustment.set_value(0.0);
    if already_settled || attempts == 0 {
        return;
    }
    gtk4::glib::idle_add_local_once(move || {
        schedule_top_scroll_restore(shared, scheduled_generation, attempts - 1);
    });
}
