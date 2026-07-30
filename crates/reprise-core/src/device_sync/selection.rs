//! Content selection per sync category (`MTP-45`) — turn E2.
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
//! ## The ready/waiting split (`MTP-40`)
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
//! what actually gates a podcast/YouTube episode reaching a device (`MTP-45`).
//! What remains unbuilt is design 6b's per-channel toggle *UI*: the
//! [`YoutubeChannelToggle`]/[`summarize_youtube_selection`] pair stays plain
//! input data with no persisted backing or GTK surface yet.
//! `podcasts::phone_sync` (`POD-12`) already decides which shows/channels
//! are enabled for a device; that join is this module's source for
//! [`EpisodeSelectionRule::UnplayedDownloadsOnly`]'s `enabled_shows` and
//! [`EpisodeSelectionRule::LatestPerChannel`]'s `channel_latest` keys, not a
//! second selection surface layered on top.
//!
//! ## Per-channel N (`MTP-36`)
//!
//! [`EpisodeSelectionRule::LatestPerChannel`] carries one resolved `latest`
//! value per enabled channel rather than a single value for all of them, so
//! design 6b's future per-channel override and the global default
//! (`podcasts::config::PodcastConfig::latest_per_channel_default`) can
//! coexist: the caller resolves each channel's effective value with
//! [`resolve_latest_per_channel`] before building the rule, this module
//! never reads either setting itself. A resolved value of `0` means
//! unlimited, exactly like the size cap has meant since `MTP-38`
//! (`SyncTarget::cap_bytes` is an `Option`) — never empty.

use std::collections::{HashMap, HashSet};

use crate::connectivity::LocalAvailability;

use super::page::SyncPlaylistRow;
use super::{
    ManagedRemoval, MirrorPlan, MirrorPlaylistSnapshot, MirrorTrack, SelectionSource, SyncTrack,
};

/// Transient source identity for the picker’s “Everything” projection and
/// its published M3U inventory. The durable selection is the existing
/// `DeviceSelection::EntireLibrary`; this value is never encoded as a smart
/// playlist selection.
pub const EVERYTHING_SOURCE: SelectionSource = SelectionSource::Smart(i64::MIN);

#[must_use]
pub fn everything_playlist_snapshot(tracks: Vec<SyncTrack>) -> MirrorPlaylistSnapshot {
    MirrorPlaylistSnapshot {
        source: EVERYTHING_SOURCE,
        name: "Everything".to_string(),
        entries: tracks.into_iter().map(MirrorTrack::Available).collect(),
    }
}

/// Applies the transfer consequences of smart-playlist copies that have
/// already been published and are configured to stay frozen. Their M3U files
/// are not rewritten, and tracks named by their captured membership are not
/// removed. Unrelated authoritative cleanup continues normally.
pub fn apply_frozen_smart_playlist_policy(
    plan: &mut MirrorPlan,
    frozen_sources: &HashSet<SelectionSource>,
    frozen_track_ids: &HashSet<i64>,
) {
    if frozen_sources.is_empty() {
        return;
    }
    plan.playlist_writes
        .retain(|write| !frozen_sources.contains(&write.source));
    plan.remove.retain(|removal| match removal {
        ManagedRemoval::Inventory(file) => !frozen_track_ids.contains(&file.track_id),
        ManagedRemoval::Orphan(_) => true,
    });
    plan.bytes_freed = plan.remove.iter().fold(0_u64, |sum, removal| {
        let bytes = match removal {
            ManagedRemoval::Inventory(file) => file.device_size,
            ManagedRemoval::Orphan(file) => file.size_bytes,
        };
        sum.saturating_add(bytes)
    });
}

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
/// count cap (`MTP-45`'s uncapped "unplayed downloads only" rule), so this
/// carries no `latest`-style field the way [`YoutubeSelectionSummary`]
/// does. Built directly from
/// [`crate::podcasts::phone_sync::selection_summary`]'s live counts, not
/// from a second selection engine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PodcastSelectionSummary {
    pub shows_selected: usize,
    pub shows_total: usize,
}

/// One row's contribution to the picker footer. The GTK layer supplies the
/// row facts and chooses the localized noun ("tracks" or "episodes"); the
/// arithmetic and missing-size honesty stay toolkit-neutral here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PickerSelectionItem {
    pub selected: bool,
    pub content_count: usize,
    pub size_bytes: Option<u64>,
    pub needs_download: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PickerSelectionSummary {
    pub selected_items: usize,
    pub content_count: usize,
    pub known_size_bytes: u64,
    pub unknown_size_items: usize,
    pub needs_download: usize,
}

#[must_use]
pub fn summarize_picker_selection(items: &[PickerSelectionItem]) -> PickerSelectionSummary {
    items.iter().filter(|item| item.selected).fold(
        PickerSelectionSummary::default(),
        |mut summary, item| {
            summary.selected_items = summary.selected_items.saturating_add(1);
            summary.content_count = summary.content_count.saturating_add(item.content_count);
            match item.size_bytes {
                Some(bytes) => {
                    summary.known_size_bytes = summary.known_size_bytes.saturating_add(bytes);
                }
                None => {
                    summary.unknown_size_items = summary.unknown_size_items.saturating_add(1);
                }
            }
            if item.needs_download {
                summary.needs_download = summary.needs_download.saturating_add(1);
            }
            summary
        },
    )
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
    /// The same persistent `wanted_on_device` flag operated by an explicit
    /// episode tick. A pin augments the category rule; it never replaces or
    /// mirrors the rule state.
    pub pinned: bool,
}

/// E2's per-category selection rule — what makes an episode "wanted" for a
/// device. The two shapes match the design's own summaries: YouTube caps
/// each enabled channel to its own `latest` newest episodes regardless of
/// played state (there is no "played" concept for a YouTube audio track) —
/// a channel absent from `channel_latest` is not enabled at all; podcasts
/// want every unplayed, already-downloaded episode from an enabled show,
/// uncapped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EpisodeSelectionRule {
    LatestPerChannel {
        /// Every enabled channel's resolved cap (`MTP-36`), keyed by
        /// `group_id`. `0` means unlimited — the caller must already have
        /// resolved a channel's persisted override against the global
        /// default via [`resolve_latest_per_channel`]; this rule does not
        /// distinguish "no override" from "explicitly unlimited" itself.
        channel_latest: HashMap<i64, usize>,
    },
    UnplayedDownloadsOnly {
        enabled_shows: HashSet<i64>,
    },
}

/// `MTP-36`: resolves one channel's effective "latest N" against the global
/// default (`podcasts::config::PodcastConfig::latest_per_channel_default`).
/// `None` (no persisted override for this channel) falls back to the
/// default; an explicit override — including `0` — always wins, because the
/// owner decision of 2026-07-29 makes `0` mean unlimited rather than "unset"
/// for every numeric sync setting (the size cap has modelled it that way
/// since `MTP-38`). Pure and DB-free like the rest of this module: the
/// caller reads the default and the override, this function only decides
/// which one applies.
#[must_use]
pub fn resolve_latest_per_channel(default_latest: usize, channel_override: Option<i64>) -> usize {
    match channel_override {
        Some(value) => usize::try_from(value).unwrap_or(0),
        None => default_latest,
    }
}

/// The intended episode set for a category (`MTP-45`): wanted episodes that
/// already have a local file, and wanted episodes still waiting on one
/// (`MTP-40`). `waiting` must never be treated as "to copy" — see the
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

/// `MTP-45`: the intended file set for a podcast/YouTube category, given
/// the selection rule and the library state (`candidates`). Wanted-but-
/// missing episodes land in [`EpisodeSelectionResult::waiting`], never in
/// `ready`.
#[must_use]
pub fn select_episodes(
    candidates: &[EpisodeSelectionCandidate],
    rule: &EpisodeSelectionRule,
) -> EpisodeSelectionResult {
    let wanted_ids = match rule {
        EpisodeSelectionRule::LatestPerChannel { channel_latest } => {
            latest_per_channel(candidates, channel_latest)
        }
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
    channel_latest: &HashMap<i64, usize>,
) -> Vec<i64> {
    let mut by_channel: HashMap<i64, Vec<&EpisodeSelectionCandidate>> = HashMap::new();
    for candidate in candidates
        .iter()
        .filter(|candidate| channel_latest.contains_key(&candidate.group_id))
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
        // `MTP-36`: 0 means unlimited (like `SyncTarget::cap_bytes` since
        // `MTP-38`), never empty — `take(0)` would silently drop the whole
        // channel, which is exactly the "0 stops syncing" bug this rule
        // must not have.
        let latest = channel_latest.get(&channel_id).copied().unwrap_or(0);
        let automatic = if latest == 0 {
            episodes.len()
        } else {
            latest.min(episodes.len())
        };
        let mut selected = episodes.iter().take(automatic).copied().collect::<Vec<_>>();
        selected.extend(
            episodes
                .iter()
                .skip(automatic)
                .filter(|candidate| candidate.pinned)
                .copied(),
        );
        selected.sort_by(|left, right| {
            right
                .published_at
                .cmp(&left.published_at)
                .then_with(|| right.episode_id.cmp(&left.episode_id))
        });
        wanted.extend(selected.into_iter().map(|candidate| candidate.episode_id));
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
            pinned: false,
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
    fn mtp_45_playlist_selection_summary_counts_selected_available_and_total() {
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
    fn mtp_45_youtube_selection_summary_counts_enabled_channels_and_names_the_rule() {
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
    fn mtp_45_youtube_selection_caps_each_enabled_channel_to_its_latest_n() {
        let candidates = vec![
            candidate(1, 10, 100, false, LocalAvailability::Available),
            candidate(2, 10, 200, false, LocalAvailability::Available),
            candidate(3, 10, 300, false, LocalAvailability::Available),
            // Disabled channel — excluded even though it is newer.
            candidate(4, 20, 400, false, LocalAvailability::Available),
        ];
        let rule = EpisodeSelectionRule::LatestPerChannel {
            channel_latest: HashMap::from([(10, 2)]),
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
    fn mtp_45_podcast_selection_wants_every_unplayed_download_from_enabled_shows_uncapped() {
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
    fn mtp_45_a_wanted_episode_without_a_local_file_waits_instead_of_being_ready_to_copy() {
        let candidates = vec![
            candidate(1, 10, 100, false, LocalAvailability::Available),
            candidate(2, 10, 200, false, LocalAvailability::Missing),
        ];
        let rule = EpisodeSelectionRule::LatestPerChannel {
            channel_latest: HashMap::from([(10, 5)]),
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

    // `MTP-36`: the global default (5) overridable per channel, and 0
    // meaning unlimited — three settings have shipped on this branch that
    // rendered and persisted but were never read by any code path, so each
    // of these asserts the actual `select_episodes` outcome, never a
    // database round-trip.

    #[test]
    fn mtp_36_the_resolved_latest_value_bounds_a_channels_selection() {
        // 8 episodes on one channel — the design's own example.
        let candidates = (1..=8)
            .map(|n| candidate(n, 10, n * 100, false, LocalAvailability::Available))
            .collect::<Vec<_>>();

        let global_default_rule = EpisodeSelectionRule::LatestPerChannel {
            channel_latest: HashMap::from([(10, 5)]),
        };
        let result = select_episodes(&candidates, &global_default_rule);
        assert_eq!(
            result.ready.len(),
            5,
            "the global default of 5 must actually bound the channel's selection"
        );

        let overridden_rule = EpisodeSelectionRule::LatestPerChannel {
            channel_latest: HashMap::from([(10, 2)]),
        };
        let result = select_episodes(&candidates, &overridden_rule);
        assert_eq!(
            result.ready.len(),
            2,
            "a channel override of 2 must change the selection, not just round-trip through storage"
        );
    }

    #[test]
    fn mtp_36_a_channel_latest_of_zero_means_unlimited_not_empty() {
        let candidates = (1..=8)
            .map(|n| candidate(n, 10, n * 100, false, LocalAvailability::Available))
            .collect::<Vec<_>>();
        let rule = EpisodeSelectionRule::LatestPerChannel {
            channel_latest: HashMap::from([(10, 0)]),
        };

        let result = select_episodes(&candidates, &rule);

        assert_eq!(
            result.ready.len(),
            8,
            "0 must mean unlimited, exactly like the size cap since MTP-38 — \
             getting this wrong would silently stop syncing the channel"
        );
    }

    #[test]
    fn mtp_36_resolve_latest_per_channel_prefers_the_channel_override_over_the_global_default() {
        assert_eq!(
            resolve_latest_per_channel(5, None),
            5,
            "no persisted override falls back to the global default"
        );
        assert_eq!(
            resolve_latest_per_channel(5, Some(2)),
            2,
            "an explicit channel override beats the global default"
        );
        assert_eq!(
            resolve_latest_per_channel(5, Some(0)),
            0,
            "an explicit override of 0 is unlimited, not the default and not empty"
        );
    }

    #[test]
    fn mtp_50_explicit_episode_pin_survives_rule_changes_refreshes_and_ageing_out() {
        let initially_pinned = EpisodeSelectionCandidate {
            episode_id: 1,
            group_id: 10,
            published_at: 100,
            played: false,
            local: LocalAvailability::Available,
            pinned: true,
        };
        let first_refresh = vec![
            initially_pinned.clone(),
            candidate(2, 10, 200, false, LocalAvailability::Available),
            candidate(3, 10, 300, false, LocalAvailability::Available),
        ];
        let latest_one = EpisodeSelectionRule::LatestPerChannel {
            channel_latest: HashMap::from([(10, 1)]),
        };

        assert_eq!(
            select_episodes(&first_refresh, &latest_one).ready,
            [3, 1],
            "the explicit pin stays selected outside the automatic latest-one window"
        );

        let refreshed = vec![
            initially_pinned,
            candidate(2, 10, 200, false, LocalAvailability::Available),
            candidate(3, 10, 300, false, LocalAvailability::Available),
            candidate(4, 10, 400, false, LocalAvailability::Available),
            candidate(5, 10, 500, false, LocalAvailability::Available),
        ];
        let latest_two = EpisodeSelectionRule::LatestPerChannel {
            channel_latest: HashMap::from([(10, 2)]),
        };

        assert_eq!(
            select_episodes(&refreshed, &latest_two).ready,
            [5, 4, 1],
            "the same flag survives a changed rule, a refresh, and further ageing out"
        );
    }

    #[test]
    fn mtp_50_podcast_pin_does_not_override_the_unplayed_standing_rule() {
        let played_and_pinned = EpisodeSelectionCandidate {
            episode_id: 1,
            group_id: 10,
            published_at: 100,
            played: true,
            local: LocalAvailability::Available,
            pinned: true,
        };
        let rule = EpisodeSelectionRule::UnplayedDownloadsOnly {
            enabled_shows: HashSet::from([10]),
        };

        assert!(
            select_episodes(&[played_and_pinned], &rule)
                .ready
                .is_empty(),
            "a podcast episode leaves the phone after it is played even if its explicit flag remains"
        );
    }

    #[test]
    fn picker_footer_sums_only_selected_items_and_keeps_missing_sizes_honest() {
        let items = [
            PickerSelectionItem {
                selected: true,
                content_count: 278,
                size_bytes: Some(2_000),
                needs_download: false,
            },
            PickerSelectionItem {
                selected: false,
                content_count: 99,
                size_bytes: Some(50_000),
                needs_download: true,
            },
            PickerSelectionItem {
                selected: true,
                content_count: 134,
                size_bytes: None,
                needs_download: true,
            },
        ];

        assert_eq!(
            summarize_picker_selection(&items),
            PickerSelectionSummary {
                selected_items: 2,
                content_count: 412,
                known_size_bytes: 2_000,
                unknown_size_items: 1,
                needs_download: 1,
            }
        );
    }

    #[test]
    fn everything_is_a_real_playlist_selection_over_the_whole_library() {
        let tracks = [1_i64, 2, 3]
            .into_iter()
            .map(|id| crate::device_sync::SyncTrack {
                id,
                source_path: format!("/{id}.flac").into(),
                original_name: format!("{id}.flac"),
                title: format!("Track {id}"),
                artist: "Artist".into(),
                album: "Album".into(),
                album_artist: "Artist".into(),
                track_number: None,
                duration_ms: 180_000,
                bitrate_kbps: None,
                size_bytes: 1_000,
                source_mtime: 1,
            })
            .collect::<Vec<_>>();

        let snapshot = everything_playlist_snapshot(tracks);

        assert_eq!(snapshot.source, EVERYTHING_SOURCE);
        assert_eq!(snapshot.name, "Everything");
        assert_eq!(snapshot.entries.len(), 3);
    }

    #[test]
    fn frozen_smart_playlist_keeps_its_published_copy_until_refresh_is_enabled() {
        let frozen = SelectionSource::Smart(7);
        let manual = SelectionSource::Playlist(8);
        let write = |source: SelectionSource| crate::device_sync::PlaylistWrite {
            source,
            source_name: "List".into(),
            device_path: "List.m3u".into(),
            entries: Vec::new(),
            contents: String::new(),
        };
        let device_file = |track_id: i64, path: &str| crate::device_sync::DeviceFileRecord {
            device_serial: "phone".into(),
            track_id,
            source_path: format!("/{path}"),
            source_size: 1_024,
            source_mtime: 1,
            device_path: path.into(),
            device_size: 1_024,
            profile_fingerprint: "original".into(),
            pinned: false,
        };
        let mut plan = crate::device_sync::MirrorPlan {
            playlist_writes: vec![write(frozen.clone()), write(manual.clone())],
            remove: vec![
                crate::device_sync::ManagedRemoval::Inventory(device_file(1, "frozen.flac")),
                crate::device_sync::ManagedRemoval::Inventory(device_file(2, "unrelated.flac")),
                crate::device_sync::ManagedRemoval::Orphan(crate::device_sync::ManagedDeviceFile {
                    relative_path: "old.flac".into(),
                    size_bytes: 1_024,
                }),
            ],
            bytes_freed: 3_072,
            ..Default::default()
        };

        apply_frozen_smart_playlist_policy(
            &mut plan,
            &HashSet::from([frozen]),
            &HashSet::from([1]),
        );

        assert_eq!(
            plan.playlist_writes
                .iter()
                .map(|write| write.source.clone())
                .collect::<Vec<_>>(),
            [manual],
            "manual playlists still publish while the frozen smart copy stays untouched"
        );
        assert_eq!(
            plan.remove,
            [
                crate::device_sync::ManagedRemoval::Inventory(device_file(2, "unrelated.flac")),
                crate::device_sync::ManagedRemoval::Orphan(crate::device_sync::ManagedDeviceFile {
                    relative_path: "old.flac".into(),
                    size_bytes: 1_024,
                }),
            ],
            "only tracks named by the frozen snapshot are retained; authoritative cleanup continues"
        );
        assert_eq!(plan.bytes_freed, 2_048);
    }
}
