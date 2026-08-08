//! Minimal `GListModel::items_changed` ranges for targeted query reloads.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ui) struct ModelChange {
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
    Some(ModelChange {
        position: u32::try_from(prefix).ok()?,
        removed: u32::try_from(before_end - prefix).ok()?,
        added: u32::try_from(after_end - prefix).ok()?,
        before_total: u32::try_from(before.len()).ok()?,
        after_total: u32::try_from(after.len()).ok()?,
        generation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_save_change_range_covers_a_resorted_album_without_the_full_model() {
        let before = [1, 2, 3, 4, 5, 6, 7, 8];
        let after = [1, 2, 6, 7, 8, 3, 4, 5];

        assert_eq!(
            changed_range(&before, &after, &[3, 4, 5], 7),
            Some(ModelChange {
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
