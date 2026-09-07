//! Minimal `GListModel::items_changed` ranges for targeted query reloads.

/// A query replacement either invalidates one covering span or moves one
/// contiguous, order-preserving block with a remove/insert signal pair.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ui) enum ModelChangeKind {
    Span,
    BlockMove { from: u32, to: u32, len: u32 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ui) struct ModelChange {
    pub(in crate::ui) kind: ModelChangeKind,
    pub(in crate::ui) position: u32,
    pub(in crate::ui) removed: u32,
    pub(in crate::ui) added: u32,
    pub(in crate::ui) before_total: u32,
    pub(in crate::ui) after_total: u32,
    /// The model generation the range was computed against — see
    /// `imp::TrackListModel::generation`.
    pub(in crate::ui) generation: u64,
}

pub(in crate::ui) fn changed_range(
    before: &[i64],
    after: &[i64],
    changed_ids: &[i64],
    generation: u64,
) -> Option<ModelChange> {
    // A set, not the caller's slice: both trims below test membership once per
    // untouched row, so a linear scan makes this O(rows × changed_ids). Tag
    // saves edit a handful of tracks and never noticed, but the deletion path
    // feeds in whatever the user multi-selected — and this runs synchronously
    // on the UI thread, in the very code meant to keep deletion responsive.
    let changed: std::collections::HashSet<i64> = changed_ids.iter().copied().collect();

    let mut prefix = 0;
    while before.get(prefix) == after.get(prefix)
        && before.get(prefix).is_some_and(|id| !changed.contains(id))
    {
        prefix += 1;
    }

    let mut before_end = before.len();
    let mut after_end = after.len();
    while before_end > prefix
        && after_end > prefix
        && before[before_end - 1] == after[after_end - 1]
        && !changed.contains(&before[before_end - 1])
    {
        before_end -= 1;
        after_end -= 1;
    }
    if prefix == before_end && prefix == after_end {
        return None;
    }
    let kind = block_move_kind(
        &before[prefix..before_end],
        &after[prefix..after_end],
        &changed,
        prefix,
    )
    .unwrap_or(ModelChangeKind::Span);
    Some(ModelChange {
        kind,
        position: u32::try_from(prefix).ok()?,
        removed: u32::try_from(before_end - prefix).ok()?,
        added: u32::try_from(after_end - prefix).ok()?,
        before_total: u32::try_from(before.len()).ok()?,
        after_total: u32::try_from(after.len()).ok()?,
        generation,
    })
}

fn block_move_kind(
    before: &[i64],
    after: &[i64],
    changed: &std::collections::HashSet<i64>,
    prefix: usize,
) -> Option<ModelChangeKind> {
    if before.len() != after.len() || changed.is_empty() {
        return None;
    }
    let (before_start, len) = changed_run(before, changed)?;
    let (after_start, after_len) = changed_run(after, changed)?;
    if len != after_len
        || before_start == after_start
        || before[before_start..before_start + len] != after[after_start..after_start + len]
    {
        return None;
    }
    let before_without = before[..before_start]
        .iter()
        .chain(&before[before_start + len..]);
    let after_without = after[..after_start]
        .iter()
        .chain(&after[after_start + len..]);
    if !before_without.eq(after_without) {
        return None;
    }
    Some(ModelChangeKind::BlockMove {
        from: u32::try_from(prefix + before_start).ok()?,
        to: u32::try_from(prefix + after_start).ok()?,
        len: u32::try_from(len).ok()?,
    })
}

fn changed_run(ids: &[i64], changed: &std::collections::HashSet<i64>) -> Option<(usize, usize)> {
    let start = ids.iter().position(|id| changed.contains(id))?;
    let end = ids.iter().rposition(|id| changed.contains(id))? + 1;
    (ids[start..end].iter().all(|id| changed.contains(id)) && end - start == changed.len())
        .then_some((start, end - start))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(range: std::ops::Range<i64>) -> Vec<i64> {
        range.collect()
    }

    fn assert_block_move(
        before: &[i64],
        after: &[i64],
        changed: &[i64],
        expected: ModelChangeKind,
    ) {
        let change = changed_range(before, after, changed, 7).expect("change expected");
        assert_eq!(change.kind, expected);
    }

    #[test]
    fn contiguous_eight_row_block_moving_up_is_a_block_move() {
        let before = ids(0..20);
        let after = [ids(0..2), ids(10..18), ids(2..10), ids(18..20)].concat();

        assert_block_move(
            &before,
            &after,
            &ids(10..18),
            ModelChangeKind::BlockMove {
                from: 10,
                to: 2,
                len: 8,
            },
        );
    }

    #[test]
    fn contiguous_eight_row_block_moving_down_is_a_block_move() {
        let before = ids(0..20);
        let after = [ids(0..2), ids(10..18), ids(2..10), ids(18..20)].concat();

        assert_block_move(
            &before,
            &after,
            &ids(2..10),
            ModelChangeKind::BlockMove {
                from: 2,
                to: 10,
                len: 8,
            },
        );
    }

    #[test]
    fn contiguous_block_moving_to_the_top_is_a_block_move() {
        let before = ids(0..20);
        let after = [ids(8..16), ids(0..8), ids(16..20)].concat();

        assert_block_move(
            &before,
            &after,
            &ids(8..16),
            ModelChangeKind::BlockMove {
                from: 8,
                to: 0,
                len: 8,
            },
        );
    }

    #[test]
    fn non_contiguous_reorder_stays_a_span() {
        assert_block_move(
            &[1, 2, 3, 4, 5, 6],
            &[1, 3, 2, 5, 4, 6],
            &[2, 4],
            ModelChangeKind::Span,
        );
    }

    #[test]
    fn unchanged_rows_without_edits_have_no_change() {
        let ids = ids(0..20);
        assert_eq!(changed_range(&ids, &ids, &[], 7), None);
    }

    #[test]
    fn moved_run_with_changed_internal_order_stays_a_span() {
        assert_block_move(
            &[1, 2, 3, 4, 5, 6, 7],
            &[1, 5, 4, 3, 2, 6, 7],
            &[2, 3, 4, 5],
            ModelChangeKind::Span,
        );
    }

    #[test]
    fn tag_save_change_range_covers_a_resorted_album_without_the_full_model() {
        let before = [1, 2, 3, 4, 5, 6, 7, 8];
        let after = [1, 2, 6, 7, 8, 3, 4, 5];

        assert_eq!(
            changed_range(&before, &after, &[3, 4, 5], 7),
            Some(ModelChange {
                kind: ModelChangeKind::BlockMove {
                    from: 2,
                    to: 5,
                    len: 3,
                },
                position: 2,
                removed: 6,
                added: 6,
                before_total: 8,
                after_total: 8,
                generation: 7,
            })
        );
    }

    #[test]
    fn tag_save_change_range_invalidates_edited_rows_that_do_not_move() {
        let ids = [1, 2, 3, 4, 5, 6];

        assert_eq!(
            changed_range(&ids, &ids, &[3, 4], 7),
            Some(ModelChange {
                kind: ModelChangeKind::Span,
                position: 2,
                removed: 2,
                added: 2,
                before_total: 6,
                after_total: 6,
                generation: 7,
            })
        );
    }
}
