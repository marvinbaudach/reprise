//! Pure per-target cap enforcement (`MTP-19`).
//!
//! The design gives YouTube audio and podcast episode targets a byte cap
//! with "oldest files leave first" eviction. This module is that decision
//! and nothing else: given sized, aged items and a cap, which ids would
//! have to leave to bring the total back under it. It knows nothing about
//! `PodcastSyncCandidate`, MTP, or any transport concern — the caller
//! supplies sizes and ages and decides how to act on the result (E4, not
//! built here).

/// One item under cap enforcement: an id, its size, and an age where a
/// *smaller* value means *older* (e.g. a source mtime or `published_at`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapItem<Id> {
    pub id: Id,
    pub size_bytes: u64,
    pub age: i64,
}

/// `MTP-19`: which items must leave to bring `items`' total size back to
/// at most `cap_bytes`, oldest (smallest `age`) first. Ties break on `id`
/// so the result is deterministic for the same input. Returns ids in the
/// order they should be removed; stops as soon as the running total is at
/// or under the cap, so it never evicts more than necessary.
#[must_use]
pub fn items_to_evict<Id: Copy + Ord>(items: &[CapItem<Id>], cap_bytes: u64) -> Vec<Id> {
    let total = items
        .iter()
        .map(|item| item.size_bytes)
        .fold(0_u64, u64::saturating_add);
    if total <= cap_bytes {
        return Vec::new();
    }

    let mut ordered = items.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|item| (item.age, item.id));

    let mut remaining = total;
    let mut evicted = Vec::new();
    for item in ordered {
        if remaining <= cap_bytes {
            break;
        }
        remaining = remaining.saturating_sub(item.size_bytes);
        evicted.push(item.id);
    }
    evicted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: i64, size_bytes: u64, age: i64) -> CapItem<i64> {
        CapItem {
            id,
            size_bytes,
            age,
        }
    }

    #[test]
    fn mtp_19_nothing_leaves_when_empty_or_already_under_cap() {
        assert_eq!(items_to_evict::<i64>(&[], 0), Vec::<i64>::new());

        let items = [item(1, 10, 100), item(2, 10, 200)];
        assert_eq!(items_to_evict(&items, 100), Vec::<i64>::new());
    }

    #[test]
    fn mtp_19_nothing_leaves_when_total_exactly_equals_the_cap() {
        let items = [item(1, 40, 1), item(2, 60, 2)];
        assert_eq!(items_to_evict(&items, 100), Vec::<i64>::new());
    }

    #[test]
    fn mtp_19_oldest_items_leave_first_and_eviction_stops_as_soon_as_it_fits() {
        // Ages: 1 is oldest, 3 is newest. Total is 90, cap is 50, so only
        // enough of the oldest items should leave to reach <= 50.
        let items = [item(1, 30, 1), item(2, 30, 2), item(3, 30, 3)];

        let evicted = items_to_evict(&items, 50);

        assert_eq!(
            evicted,
            vec![1, 2],
            "removing the two oldest reaches 30 <= 50"
        );
    }

    #[test]
    fn mtp_19_a_single_oversized_item_is_evicted_even_alone() {
        let items = [item(1, 500, 1)];

        assert_eq!(items_to_evict(&items, 100), vec![1]);
    }

    #[test]
    fn mtp_19_equal_ages_break_ties_by_id_for_determinism() {
        let items = [item(3, 10, 1), item(1, 10, 1), item(2, 10, 1)];

        let evicted = items_to_evict(&items, 0);

        assert_eq!(evicted, vec![1, 2, 3]);
    }

    #[test]
    fn mtp_19_zero_cap_evicts_everything_oldest_first() {
        let items = [item(1, 5, 2), item(2, 5, 1)];

        assert_eq!(items_to_evict(&items, 0), vec![2, 1]);
    }
}
