#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PodcastsEmptyState {
    List,
    Empty,
    NoEpisodes,
    NoResults,
}

pub(super) fn podcasts_empty_state_for(
    subscription_count: usize,
    total_episodes: usize,
    visible_episodes: usize,
    has_filter: bool,
) -> PodcastsEmptyState {
    if visible_episodes > 0 {
        PodcastsEmptyState::List
    } else if subscription_count == 0 {
        PodcastsEmptyState::Empty
    } else if total_episodes == 0 {
        PodcastsEmptyState::NoEpisodes
    } else if has_filter {
        PodcastsEmptyState::NoResults
    } else {
        PodcastsEmptyState::NoEpisodes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_state_has_one_deterministic_next_step() {
        assert_eq!(
            podcasts_empty_state_for(1, 2, 2, false),
            PodcastsEmptyState::List
        );
        assert_eq!(
            podcasts_empty_state_for(0, 0, 0, false),
            PodcastsEmptyState::Empty
        );
        assert_eq!(
            podcasts_empty_state_for(1, 0, 0, false),
            PodcastsEmptyState::NoEpisodes
        );
        assert_eq!(
            podcasts_empty_state_for(1, 3, 0, true),
            PodcastsEmptyState::NoResults
        );
    }
}
