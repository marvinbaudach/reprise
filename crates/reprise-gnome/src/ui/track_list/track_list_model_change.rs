//! Minimal `GListModel::items_changed` ranges for tag-save query reloads.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ModelChange {
    pub(super) position: u32,
    pub(super) removed: u32,
    pub(super) added: u32,
    pub(super) before_total: u32,
    pub(super) after_total: u32,
    /// The model generation the range was computed against — see
    /// `imp::TrackListModel::generation`.
    pub(super) generation: u64,
}

pub(super) fn changed_range(
    before: &[i64],
    after: &[i64],
    changed_ids: &[i64],
    generation: u64,
) -> Option<ModelChange> {
    let mut prefix = 0;
    while before.get(prefix) == after.get(prefix)
        && before
            .get(prefix)
            .is_some_and(|id| !changed_ids.contains(id))
    {
        prefix += 1;
    }

    let mut before_end = before.len();
    let mut after_end = after.len();
    while before_end > prefix
        && after_end > prefix
        && before[before_end - 1] == after[after_end - 1]
        && !changed_ids.contains(&before[before_end - 1])
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
