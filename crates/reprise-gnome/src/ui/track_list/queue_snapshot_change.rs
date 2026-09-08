//! Pure decision functions for a queue snapshot swap, kept separate so they can be unit-tested and evolved without the GObject model.

/// Returns the one contiguous `items_changed` span between two queue
/// snapshots. Preserving the common prefix and suffix lets GTK keep their
/// existing row widgets; the frequent automatic-advance shape
/// `[current-next, ...] -> [...]` becomes one leading removal.
pub(super) fn queue_snapshot_change(
    old: &super::queue_sections::QueueViewModel,
    new: &super::queue_sections::QueueViewModel,
) -> (u32, u32, u32) {
    new.change_from(old).unwrap_or((
        0,
        u32::try_from(old.total_len()).unwrap_or(u32::MAX),
        u32::try_from(new.total_len()).unwrap_or(u32::MAX),
    ))
}

/// An `items-changed` span `(position, removed, added)` paired with a
/// `sections-changed` range `(position, n_items)`; either is `None` when
/// that signal must not be emitted.
pub(super) type QueueSnapshotSignals = (Option<(u32, u32, u32)>, Option<(u32, u32)>);

/// The signals a queue snapshot swap has to emit.
///
/// GTK's contract (`gtk_section_model_sections_changed`): `items-changed`
/// implies re-sectioning ONLY for the items it covers. The O(1) advance
/// shape `items_changed(0, 1, 0)` covers no surviving row, so without an
/// explicit `sections-changed` GTK keeps its cached header tiles and merely
/// shifts their bounds by the delta — the Play Next header is dropped and
/// its rows end up titled "Now Playing" (reproduced live by
/// `examples/queue_section_shift_repro.rs`). A full-range `items-changed`
/// already re-matches every header, so it needs no second signal. A narrow
/// items change still needs the full sections range: this emitter cannot see
/// where the section containing `position` starts, and a hinted change can
/// begin after that section's header row. Narrowing safely requires the call
/// site to supply that section start.
pub(super) fn queue_snapshot_emissions(
    change: (u32, u32, u32),
    sections_changed: bool,
    new_total: u32,
) -> QueueSnapshotSignals {
    let (position, removed, added) = change;
    let items = (removed != 0 || added != 0).then_some(change);
    let covers_every_row = items.is_some() && position == 0 && added >= new_total;
    let sections =
        (sections_changed && !covers_every_row && new_total > 0).then_some((0, new_total));
    (items, sections)
}

#[cfg(test)]
#[path = "queue_snapshot_change_tests.rs"]
mod tests;
