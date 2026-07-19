//! QUE-3: maps composite Queue-view row positions (see `queue_sections`)
//! onto section-local operations. The view shows Now Playing at 0, then
//! Play Next, then the snapshot's Up Next tail — but the controller's
//! operations are section-scoped (`up_next` indices vs. snapshot order
//! positions), so every interaction (activate, remove, drag) remaps here
//! first. Pure functions; the wiring lives in `window_action_wiring.rs`.
//!
//! Drag-reorder ([`reorder_op`]) supports a plain reorder within Play Next
//! and promotion of one Up Next row into Play Next. The virtual context is
//! never reordered in place (QUE-8).

use super::queue_sections::{QueueSection, QueueSectionKind};

/// A composite-view row, resolved to its section-local coordinate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QueueRow {
    NowPlaying,
    /// Index into the manual Play Next list.
    PlayNext(usize),
    /// Play-order position in the snapshot AFTER the current track — i.e.
    /// snapshot order position `current + 1 + index`. Stored as the offset
    /// into the Up Next section; the controller adds the playhead base.
    UpNext(usize),
}

/// A drag-reorder over the composite view, resolved to what it means.
// All three variants end in "Next" for a real reason (Play Next / Up Next
// are the section names QUE-3 defines), not an accidental naming pattern —
// clippy's postfix heuristic doesn't know that, so it's silenced here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub(crate) enum QueueReorderOp {
    /// Reorder within the Play Next section.
    WithinPlayNext { from: usize, to: usize },
    /// Drag an Up Next snapshot row into the Play Next section: remove it
    /// from the snapshot, insert it as a manual entry at `insert_at`.
    PromoteUpNext {
        up_next_offset: usize,
        insert_at: usize,
    },
}

/// Resolves a view position to its section-local row. `None` for positions
/// outside every section (defensive — GTK should never hand one over).
pub(crate) fn classify(view_position: u32, sections: &[QueueSection]) -> Option<QueueRow> {
    for section in sections {
        let end = section.start.saturating_add(section.len);
        if view_position < section.start || view_position >= end {
            continue;
        }
        let offset = (view_position - section.start) as usize;
        return Some(match section.kind {
            QueueSectionKind::NowPlaying => QueueRow::NowPlaying,
            QueueSectionKind::PlayNext => QueueRow::PlayNext(offset),
            QueueSectionKind::UpNext { .. } => QueueRow::UpNext(offset),
        });
    }
    None
}

/// Resolves a composite-view drag from `from` to `to` by classifying BOTH
/// positions and matching on the pair. Rules (QUE-3):
/// - Now Playing is never a drag source → `None`.
/// - within Play Next → plain reorder (no-op on the same slot → `None`).
/// - Play Next dropped outside Play Next → rejected.
/// - Up Next dropped onto Play Next → promote out of the snapshot.
/// - within Up Next → rejected; Continuing is not reorderable.
/// - anything that fails to classify on either side → `None`.
pub(crate) fn reorder_op(from: u32, to: u32, sections: &[QueueSection]) -> Option<QueueReorderOp> {
    let from_row = classify(from, sections)?;
    let to_row = classify(to, sections)?;

    reorder_rows(from_row, to_row)
}

pub(crate) fn reorder_rows(from_row: QueueRow, to_row: QueueRow) -> Option<QueueReorderOp> {
    match (from_row, to_row) {
        (QueueRow::NowPlaying, _) => None,
        (QueueRow::PlayNext(f), QueueRow::PlayNext(t)) => {
            if f == t {
                None
            } else {
                Some(QueueReorderOp::WithinPlayNext { from: f, to: t })
            }
        }
        (QueueRow::PlayNext(_), QueueRow::NowPlaying) => None,
        (QueueRow::PlayNext(_), QueueRow::UpNext(_)) => None,
        (QueueRow::UpNext(f), QueueRow::PlayNext(t)) => Some(QueueReorderOp::PromoteUpNext {
            up_next_offset: f,
            insert_at: t,
        }),
        (QueueRow::UpNext(_), QueueRow::NowPlaying) => None,
        (QueueRow::UpNext(_), QueueRow::UpNext(_)) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::track_list::queue_sections::compose;

    fn sections() -> Vec<QueueSection> {
        // View: [1] now playing | [2,3] play next | [4,5,6] up next
        compose(Some(1), &[2, 3], &[4, 5, 6], Some("Music")).sections
    }

    #[test]
    fn classify_resolves_each_section_and_rejects_out_of_range() {
        let s = sections();
        assert_eq!(classify(0, &s), Some(QueueRow::NowPlaying));
        assert_eq!(classify(1, &s), Some(QueueRow::PlayNext(0)));
        assert_eq!(classify(2, &s), Some(QueueRow::PlayNext(1)));
        assert_eq!(classify(3, &s), Some(QueueRow::UpNext(0)));
        assert_eq!(classify(5, &s), Some(QueueRow::UpNext(2)));
        assert_eq!(classify(6, &s), None);
    }

    #[test]
    fn reorder_within_play_next_maps_to_local_indices() {
        let s = sections();
        assert_eq!(
            reorder_op(1, 2, &s),
            Some(QueueReorderOp::WithinPlayNext { from: 0, to: 1 })
        );
        // Same slot → no-op.
        assert_eq!(reorder_op(1, 1, &s), None);
    }

    #[test]
    fn que_8_reorder_only_within_the_manual_section() {
        let sections = sections();

        assert_eq!(
            reorder_op(1, 2, &sections),
            Some(QueueReorderOp::WithinPlayNext { from: 0, to: 1 })
        );
        assert_eq!(reorder_op(3, 5, &sections), None);
        assert_eq!(reorder_op(1, 5, &sections), None);
        assert_eq!(reorder_op(3, 0, &sections), None);
        assert_eq!(reorder_op(2, 0, &sections), None);
    }

    #[test]
    fn play_next_cannot_be_dragged_into_the_snapshot() {
        let s = sections();
        assert_eq!(reorder_op(1, 5, &s), None);
    }

    #[test]
    fn up_next_rows_promote_into_play_next() {
        let s = sections();
        // Up Next row 4 (view 3) dropped at the top of Play Next (view 1).
        assert_eq!(
            reorder_op(3, 1, &s),
            Some(QueueReorderOp::PromoteUpNext {
                up_next_offset: 0,
                insert_at: 0
            })
        );
        // Dropped onto the last Play Next row (view 2) still promotes.
        assert_eq!(
            reorder_op(4, 2, &s),
            Some(QueueReorderOp::PromoteUpNext {
                up_next_offset: 1,
                insert_at: 1
            })
        );
    }

    #[test]
    fn up_next_internal_reorder_is_rejected() {
        let s = sections();
        assert_eq!(reorder_op(3, 5, &s), None);
        assert_eq!(reorder_op(5, 3, &s), None);
        assert_eq!(reorder_op(4, 4, &s), None);
    }

    #[test]
    fn up_next_row_dropped_on_now_playing_is_rejected() {
        let s = sections();
        assert_eq!(reorder_op(3, 0, &s), None);
    }

    #[test]
    fn play_next_row_dropped_on_now_playing_is_rejected() {
        let s = sections();
        assert_eq!(reorder_op(2, 0, &s), None);
        assert_eq!(reorder_op(1, 0, &s), None);
    }

    #[test]
    fn without_play_next_has_no_drop_target() {
        // View: [1] now playing | [4,5] up next — no Play Next section.
        let s = compose(Some(1), &[], &[4, 5], Some("Music")).sections;
        assert_eq!(reorder_op(2, 1, &s), None);
        assert_eq!(reorder_op(1, 2, &s), None);
        assert_eq!(reorder_op(2, 0, &s), None);
    }

    #[test]
    fn now_playing_cannot_be_dragged() {
        let s = sections();
        assert_eq!(reorder_op(0, 2, &s), None);
    }
}
