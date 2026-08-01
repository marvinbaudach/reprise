//! The episode order a range selection is defined over.
//!
//! `podcasts_groups::render` decides what is on screen; this reads the same
//! inputs, so the two cannot disagree about it. Rows the user cannot see — a
//! collapsed group, everything past a group's ten-episode preview window — are
//! not part of the order, so a Shift-click never selects invisible episodes.
//!
//! Deliberately derived on each use rather than recorded while rendering: a
//! group's expander writes `expanded_sources` straight from its `notify`
//! handler without a re-render, so a cached order would be stale the moment a
//! user opened or closed a group.

use std::collections::BTreeSet;

use reprise_core::podcasts::SourceGroup;

pub(super) fn rendered_episode_ids(
    groups: &[SourceGroup],
    expanded_sources: &BTreeSet<i64>,
    expanded_episode_sources: &BTreeSet<i64>,
) -> Vec<i64> {
    groups
        .iter()
        .filter(|group| expanded_sources.contains(&group.subscription_id))
        .flat_map(|group| {
            let visible = super::podcasts_episode_window::visible_count(
                group.episodes.len(),
                expanded_episode_sources.contains(&group.subscription_id),
            );
            group
                .episodes
                .iter()
                .take(visible)
                .map(|episode| episode.id)
        })
        .collect()
}

#[cfg(test)]
#[path = "podcasts_rendered_order_tests.rs"]
mod tests;
