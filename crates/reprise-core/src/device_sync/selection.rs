//! Content selection per sync category (`MTP-21`) — turn E2.
//!
//! Design 7a/7b name the shape each category's "what will sync" summary
//! must answer: Playlists reads "2 of 4 selected · 278 tracks", YouTube
//! audio reads "2 of 6 channels · latest 5 each", and podcast episodes read
//! "Unplayed downloads only". This module is the pure projection behind
//! those three summaries and, for YouTube/podcasts, the intended episode
//! set itself — the per-channel toggle from design 6b and the per-show
//! selection feed in here, not into a display string built by hand.
//!
//! Playlists already have a complete, tested selection engine
//! (`SelectionSource`, `MirrorPlaylistSnapshot`, `page::project_sync_page`
//! → [`super::page::SyncPlaylistRow`]); [`summarize_playlist_selection`] is a
//! thin read of what that engine already produced, not a second one.
//! YouTube and podcasts have no equivalent yet — this module adds it as one
//! shared, provider-neutral shape ([`EpisodeSelectionCandidate`]) plus one
//! rule per category, because an RSS episode and a YouTube video both
//! reduce to "an episode with a publish time, a played flag, and a local
//! file or not" before selection logic needs to run.
//!
//! ## The ready/waiting split (`MTP-20`)
//!
//! `podcasts::wanted_on_device` established that an item can be *wanted*
//! before it has a file: marking "Sync to phone" on an episode with no
//! local copy starts (or queues) a download instead of being rejected.
//! [`select_episodes`] carries that distinction through selection itself —
//! its result never lets a wanted-but-missing episode masquerade as
//! ready-to-copy. [`EpisodeSelectionResult::waiting`] exists specifically so
//! a caller (and, downstream, `category_diff`'s balance) cannot
//! accidentally count a file that has not been downloaded yet as part of
//! "what this sync will copy".
//!
//! ## What stays inert
//!
//! This module has no database access and persists nothing. The live
//! per-device pipeline (`reprise_gnome::device_sync_compact::
//! recompute_delta_silent`) calls [`select_episodes`] over
//! `podcasts::query_selection_candidates_for_device`'s facts and only feeds
//! `EpisodeSelectionResult::ready` into `podcasts::build_plan` — that is
//! what actually gates a podcast/YouTube episode reaching a device (`MTP-21`).
//! What remains unbuilt is design 6b's per-channel toggle *UI*: the
//! [`YoutubeChannelToggle`]/[`summarize_youtube_selection`] pair stays plain
//! input data with no persisted backing or GTK surface yet, and YouTube's
//! own "latest N per channel" cap has no persisted value either — the live
//! pipeline calls [`select_episodes`] with an unbounded `latest` until that
//! lands (`MTP-36`, `[geplant]`). `podcasts::phone_sync` (`POD-12`) already
//! decides which shows/channels are enabled for a device; that join is this
//! module's source for `EpisodeSelectionRule`'s `enabled_shows`/
//! `enabled_channels`, not a second selection surface layered on top.

use std::collections::{HashMap, HashSet};

use crate::connectivity::LocalAvailability;

use super::page::SyncPlaylistRow;

/// "N of M selected · K tracks" — Playlists' selection summary (design:
/// "2 of 4 selected · 278 tracks"). `available_total`/`selected` only ever
/// count rows the library can currently resolve to a playlist
/// (`SyncPlaylistRow::available`); a selected-but-missing playlist is
/// tracked elsewhere (`MirrorBlocker::MissingPlaylist`), not folded in here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlaylistSelectionSummary {
    pub selected: usize,
    pub available_total: usize,
    pub unique_track_count: usize,
}

#[must_use]
pub fn summarize_playlist_selection(
    rows: &[SyncPlaylistRow],
    unique_track_count: usize,
) -> PlaylistSelectionSummary {
    let available_total = rows.iter().filter(|row| row.available).count();
    let selected = rows
        .iter()
        .filter(|row| row.available && row.selected)
        .count();
    PlaylistSelectionSummary {
        selected,
        available_total,
        unique_track_count,
    }
}

/// One YouTube channel's per-device sync toggle (design 6b). Plain input
/// data — see the module docs on why persisting this choice is out of
/// scope here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct YoutubeChannelToggle {
    pub subscription_id: i64,
    pub enabled: bool,
}

/// "N of M channels · latest K each" (design: "2 of 6 channels · latest 5
/// each").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct YoutubeSelectionSummary {
    pub channels_selected: usize,
    pub channels_total: usize,
    pub latest_per_channel: usize,
}

#[must_use]
pub fn summarize_youtube_selection(
    channels: &[YoutubeChannelToggle],
    latest_per_channel: usize,
) -> YoutubeSelectionSummary {
    YoutubeSelectionSummary {
        channels_selected: channels.iter().filter(|channel| channel.enabled).count(),
        channels_total: channels.len(),
        latest_per_channel,
    }
}

/// "N of M shows selected" (`MTP-37`) — podcast episodes have no per-show
/// count cap (`MTP-21`'s uncapped "unplayed downloads only" rule), so this
/// carries no `latest`-style field the way [`YoutubeSelectionSummary`]
/// does. Built directly from
/// [`crate::podcasts::phone_sync::selection_summary`]'s live counts, not
/// from a second selection engine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PodcastSelectionSummary {
    pub shows_selected: usize,
    pub shows_total: usize,
}

/// One episode considered for phone-sync selection — provider-neutral: an
/// RSS episode and a YouTube video both reduce to this shape. `group_id` is
/// the owning subscription/channel id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpisodeSelectionCandidate {
    pub episode_id: i64,
    pub group_id: i64,
    pub published_at: i64,
    pub played: bool,
    pub local: LocalAvailability,
}

/// E2's per-category selection rule — what makes an episode "wanted" for a
/// device. The two shapes match the design's own summaries: YouTube caps
/// each enabled channel to its `latest` newest episodes regardless of
/// played state (there is no "played" concept for a YouTube audio track);
/// podcasts want every unplayed, already-downloaded episode from an enabled
/// show, uncapped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EpisodeSelectionRule {
    LatestPerChannel {
        enabled_channels: HashSet<i64>,
        latest: usize,
    },
    UnplayedDownloadsOnly {
        enabled_shows: HashSet<i64>,
    },
}

/// The intended episode set for a category (`MTP-21`): wanted episodes that
/// already have a local file, and wanted episodes still waiting on one
/// (`MTP-20`). `waiting` must never be treated as "to copy" — see the
/// module docs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EpisodeSelectionResult {
    pub ready: Vec<i64>,
    pub waiting: Vec<i64>,
}

impl EpisodeSelectionResult {
    #[must_use]
    pub fn wanted_count(&self) -> usize {
        self.ready.len() + self.waiting.len()
    }
}

/// `MTP-21`: the intended file set for a podcast/YouTube category, given
/// the selection rule and the library state (`candidates`). Wanted-but-
/// missing episodes land in [`EpisodeSelectionResult::waiting`], never in
/// `ready`.
#[must_use]
pub fn select_episodes(
    candidates: &[EpisodeSelectionCandidate],
    rule: &EpisodeSelectionRule,
) -> EpisodeSelectionResult {
    let wanted_ids = match rule {
        EpisodeSelectionRule::LatestPerChannel {
            enabled_channels,
            latest,
        } => latest_per_channel(candidates, enabled_channels, *latest),
        EpisodeSelectionRule::UnplayedDownloadsOnly { enabled_shows } => candidates
            .iter()
            .filter(|candidate| enabled_shows.contains(&candidate.group_id) && !candidate.played)
            .map(|candidate| candidate.episode_id)
            .collect(),
    };

    let by_id = candidates
        .iter()
        .map(|candidate| (candidate.episode_id, candidate))
        .collect::<HashMap<_, _>>();
    let mut result = EpisodeSelectionResult::default();
    for episode_id in wanted_ids {
        match by_id.get(&episode_id).map(|candidate| candidate.local) {
            Some(LocalAvailability::Available) => result.ready.push(episode_id),
            Some(LocalAvailability::Missing) | None => result.waiting.push(episode_id),
        }
    }
    result
}

fn latest_per_channel(
    candidates: &[EpisodeSelectionCandidate],
    enabled_channels: &HashSet<i64>,
    latest: usize,
) -> Vec<i64> {
    let mut by_channel: HashMap<i64, Vec<&EpisodeSelectionCandidate>> = HashMap::new();
    for candidate in candidates
        .iter()
        .filter(|candidate| enabled_channels.contains(&candidate.group_id))
    {
        by_channel
            .entry(candidate.group_id)
            .or_default()
            .push(candidate);
    }

    let mut channel_ids = by_channel.keys().copied().collect::<Vec<_>>();
    channel_ids.sort_unstable();
    let mut wanted = Vec::new();
    for channel_id in channel_ids {
        let mut episodes = by_channel.remove(&channel_id).unwrap_or_default();
        episodes.sort_by(|left, right| {
            right
                .published_at
                .cmp(&left.published_at)
                .then_with(|| right.episode_id.cmp(&left.episode_id))
        });
        wanted.extend(episodes.into_iter().take(latest).map(|c| c.episode_id));
    }
    wanted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(
        episode_id: i64,
        group_id: i64,
        published_at: i64,
        played: bool,
        local: LocalAvailability,
    ) -> EpisodeSelectionCandidate {
        EpisodeSelectionCandidate {
            episode_id,
            group_id,
            published_at,
            played,
            local,
        }
    }

    fn row(source: i64, selected: bool, available: bool) -> SyncPlaylistRow {
        SyncPlaylistRow {
            source: crate::device_sync::SelectionSource::Playlist(source),
            name: Some(format!("Playlist {source}")),
            smart: false,
            selected,
            available,
            entry_count: 0,
            unique_track_count: 0,
            unavailable_count: 0,
            target_bytes: 0,
            last_synced_at: None,
        }
    }

    #[test]
    fn mtp_21_playlist_selection_summary_counts_selected_available_and_total() {
        let rows = vec![
            row(1, true, true),
            row(2, false, true),
            row(3, true, true),
            row(4, false, true),
            // A previously selected playlist that has since disappeared —
            // still `selected`, but no longer `available`, and must not
            // count toward either total.
            row(5, true, false),
        ];

        let summary = summarize_playlist_selection(&rows, 278);

        assert_eq!(
            summary,
            PlaylistSelectionSummary {
                selected: 2,
                available_total: 4,
                unique_track_count: 278,
            },
            "matches the design's own '2 of 4 selected \u{b7} 278 tracks'"
        );
    }

    #[test]
    fn mtp_21_youtube_selection_summary_counts_enabled_channels_and_names_the_rule() {
        let channels = vec![
            YoutubeChannelToggle {
                subscription_id: 1,
                enabled: true,
            },
            YoutubeChannelToggle {
                subscription_id: 2,
                enabled: false,
            },
            YoutubeChannelToggle {
                subscription_id: 3,
                enabled: true,
            },
        ];

        let summary = summarize_youtube_selection(&channels, 5);

        assert_eq!(
            summary,
            YoutubeSelectionSummary {
                channels_selected: 2,
                channels_total: 3,
                latest_per_channel: 5,
            },
            "matches the design's own '2 of 6 channels \u{b7} latest 5 each'"
        );
    }

    #[test]
    fn mtp_21_youtube_selection_caps_each_enabled_channel_to_its_latest_n() {
        let candidates = vec![
            candidate(1, 10, 100, false, LocalAvailability::Available),
            candidate(2, 10, 200, false, LocalAvailability::Available),
            candidate(3, 10, 300, false, LocalAvailability::Available),
            // Disabled channel — excluded even though it is newer.
            candidate(4, 20, 400, false, LocalAvailability::Available),
        ];
        let rule = EpisodeSelectionRule::LatestPerChannel {
            enabled_channels: HashSet::from([10]),
            latest: 2,
        };

        let result = select_episodes(&candidates, &rule);

        assert_eq!(
            result.ready,
            [3, 2],
            "newest two of the enabled channel, newest first"
        );
        assert!(result.waiting.is_empty());
    }

    #[test]
    fn mtp_21_podcast_selection_wants_every_unplayed_download_from_enabled_shows_uncapped() {
        let candidates = vec![
            candidate(1, 1, 100, false, LocalAvailability::Available),
            candidate(2, 1, 200, true, LocalAvailability::Available), // played
            candidate(3, 1, 300, false, LocalAvailability::Available),
            candidate(4, 2, 400, false, LocalAvailability::Available), // disabled show
        ];
        let rule = EpisodeSelectionRule::UnplayedDownloadsOnly {
            enabled_shows: HashSet::from([1]),
        };

        let result = select_episodes(&candidates, &rule);

        let mut ready = result.ready.clone();
        ready.sort_unstable();
        assert_eq!(
            ready,
            [1, 3],
            "unplayed episodes from the enabled show only, uncapped"
        );
    }

    #[test]
    fn mtp_21_a_wanted_episode_without_a_local_file_waits_instead_of_being_ready_to_copy() {
        let candidates = vec![
            candidate(1, 10, 100, false, LocalAvailability::Available),
            candidate(2, 10, 200, false, LocalAvailability::Missing),
        ];
        let rule = EpisodeSelectionRule::LatestPerChannel {
            enabled_channels: HashSet::from([10]),
            latest: 5,
        };

        let result = select_episodes(&candidates, &rule);

        assert_eq!(result.ready, [1]);
        assert_eq!(
            result.waiting,
            [2],
            "missing a local file keeps it out of ready, not out of the result"
        );
        assert_eq!(result.wanted_count(), 2);
    }
}
