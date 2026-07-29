#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PodcastsEmptyState {
    List,
    Empty,
    /// `SRC-10` addendum (Block B2): nothing subscribed yet **and** this
    /// source's own module is switched off (`G1`/`NET-1a`) — the true empty
    /// state's sibling, offering "Enable in Preferences" instead of Add.
    ModuleOff,
    NoEpisodes,
    NoResults,
    /// `SRC-10` addendum (Block B2): the "Downloaded" filter is active and
    /// nothing downloaded matches it — distinct from `NoResults` so the
    /// copy can say why, not just that nothing matched.
    NoDownloads,
}

/// Decides which of the six states a render pass is in. Never renders
/// anything itself — `podcasts_view.rs` reads the copy for each case
/// separately, so this stays testable without a display.
#[allow(clippy::too_many_arguments)]
pub(super) fn podcasts_empty_state_for(
    subscription_count: usize,
    total_episodes: usize,
    visible_episodes: usize,
    has_filter: bool,
    downloaded_filter_active: bool,
    module_enabled: bool,
) -> PodcastsEmptyState {
    if visible_episodes > 0 {
        PodcastsEmptyState::List
    } else if subscription_count == 0 {
        if module_enabled {
            PodcastsEmptyState::Empty
        } else {
            PodcastsEmptyState::ModuleOff
        }
    } else if total_episodes == 0 {
        PodcastsEmptyState::NoEpisodes
    } else if downloaded_filter_active {
        PodcastsEmptyState::NoDownloads
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
            podcasts_empty_state_for(1, 2, 2, false, false, true),
            PodcastsEmptyState::List
        );
        assert_eq!(
            podcasts_empty_state_for(0, 0, 0, false, false, true),
            PodcastsEmptyState::Empty
        );
        assert_eq!(
            podcasts_empty_state_for(1, 0, 0, false, false, true),
            PodcastsEmptyState::NoEpisodes
        );
        assert_eq!(
            podcasts_empty_state_for(1, 3, 0, true, false, true),
            PodcastsEmptyState::NoResults
        );
    }

    /// `SRC-10` addendum: a switched-off module with zero subscriptions must
    /// decide `ModuleOff`, not the ordinary `Empty` — would go red if the
    /// module gate were ignored, since both share `subscription_count == 0`.
    #[test]
    fn src_10_a_switched_off_module_with_nothing_subscribed_decides_module_off_not_empty() {
        assert_eq!(
            podcasts_empty_state_for(0, 0, 0, false, false, false),
            PodcastsEmptyState::ModuleOff
        );
        // Existing subscriptions outrank the module gate here — B2 only
        // replaces the empty case's Add button, it does not lock out an
        // already-populated view.
        assert_eq!(
            podcasts_empty_state_for(1, 2, 2, false, false, false),
            PodcastsEmptyState::List
        );
    }

    /// `SRC-10` addendum: a filter that matches nothing must decide
    /// `NoResults`, never silently fall back to `Empty`/`NoEpisodes` — would
    /// go red if `has_filter` were ignored after subscriptions and episodes
    /// both exist.
    #[test]
    fn src_10_a_filter_matching_nothing_decides_no_results_not_empty_or_no_episodes() {
        assert_eq!(
            podcasts_empty_state_for(2, 10, 0, true, false, true),
            PodcastsEmptyState::NoResults
        );
    }

    /// `SRC-10` addendum: the "Downloaded" filter matching nothing decides
    /// its own `NoDownloads` case rather than the generic `NoResults` — the
    /// two must diverge even though both are "a filter matched zero rows".
    #[test]
    fn src_10_the_downloaded_filter_matching_nothing_decides_no_downloads_not_no_results() {
        assert_eq!(
            podcasts_empty_state_for(2, 10, 0, true, true, true),
            PodcastsEmptyState::NoDownloads
        );
    }
}
