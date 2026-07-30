//! Pure per-source episode windowing for the grouped list.

const EPISODE_PREVIEW_LIMIT: usize = 10;

pub(super) fn visible_count(total: usize, expanded: bool) -> usize {
    if expanded {
        total
    } else {
        total.min(EPISODE_PREVIEW_LIMIT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapsed_groups_show_ten_episodes_and_expanded_groups_show_everything() {
        assert_eq!(visible_count(0, false), 0);
        assert_eq!(visible_count(9, false), 9);
        assert_eq!(visible_count(10, false), 10);
        assert_eq!(visible_count(15, false), 10);
        assert_eq!(visible_count(15, true), 15);
    }
}
