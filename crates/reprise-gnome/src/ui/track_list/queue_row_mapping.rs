//! QUE-3: maps composite Queue-view row positions (see `queue_sections`)
//! onto section-local operations. The view shows Now Playing at 0, then
//! Play Next, then the snapshot's Up Next tail — but the controller's
//! operations are section-scoped (`up_next` indices vs. snapshot order
//! positions), so every interaction (activate, remove, drag) remaps here
//! first. Pure functions; the wiring lives in `window_action_wiring.rs`.

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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QueueReorderOp {
    /// Reorder within the Play Next section (QUE-3's only free reorder).
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

/// Resolves a composite-view drag from `from` to `to`. Rules (QUE-3):
/// - within Play Next → plain reorder;
/// - Up Next row dropped into (or at the boundary of) Play Next → promote;
/// - everything else (reordering the snapshot, dragging onto Now Playing,
///   dragging Play Next into the snapshot) → `None` (rejected).
pub(crate) fn reorder_op(from: u32, to: u32, sections: &[QueueSection]) -> Option<QueueReorderOp> {
    let from_row = classify(from, sections)?;
    // A drop at the very end of the Play Next section arrives as the first
    // Up Next position (or one past the model for a trailing drop) — treat
    // "to" leniently: resolve against Play Next bounds below instead of
    // requiring it to classify.
    let play_next = sections
        .iter()
        .find(|section| section.kind == QueueSectionKind::PlayNext);

    match from_row {
        QueueRow::PlayNext(from_offset) => {
            let section = play_next?;
            let end = section.start.saturating_add(section.len);
            if to < section.start || to > end {
                return None;
            }
            let to_offset = ((to - section.start) as usize).min(section.len as usize - 1);
            if to_offset == from_offset {
                return None;
            }
            Some(QueueReorderOp::WithinPlayNext {
                from: from_offset,
                to: to_offset,
            })
        }
        QueueRow::UpNext(up_next_offset) => {
            // Promotion target: anywhere in Play Next, or the slot directly
            // after Now Playing when no Play Next section exists yet.
            let (start, end) = match play_next {
                Some(section) => (section.start, section.start.saturating_add(section.len)),
                None => {
                    let after_now = sections
                        .iter()
                        .find(|section| section.kind == QueueSectionKind::NowPlaying)
                        .map_or(0, |section| section.start.saturating_add(section.len));
                    (after_now, after_now)
                }
            };
            if to < start || to > end {
                return None;
            }
            Some(QueueReorderOp::PromoteUpNext {
                up_next_offset,
                insert_at: (to - start) as usize,
            })
        }
        QueueRow::NowPlaying => None,
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
        // Dropped at the Play Next end boundary (view 3 = first up-next
        // slot doubles as "after the last play-next entry").
        assert_eq!(
            reorder_op(4, 3, &s),
            Some(QueueReorderOp::PromoteUpNext {
                up_next_offset: 1,
                insert_at: 2
            })
        );
    }

    #[test]
    fn up_next_promotes_after_now_playing_when_no_play_next_exists() {
        // View: [1] now playing | [4,5] up next — no Play Next section.
        let s = compose(Some(1), &[], &[4, 5], Some("Music")).sections;
        assert_eq!(
            reorder_op(2, 1, &s),
            Some(QueueReorderOp::PromoteUpNext {
                up_next_offset: 1,
                insert_at: 0
            })
        );
        // Dropping it back into the snapshot region is rejected.
        assert_eq!(reorder_op(1, 2, &s), None);
    }

    #[test]
    fn snapshot_internal_reorder_and_now_playing_drags_are_rejected() {
        let s = sections();
        assert_eq!(reorder_op(3, 5, &s), None);
        assert_eq!(reorder_op(0, 2, &s), None);
    }
}
