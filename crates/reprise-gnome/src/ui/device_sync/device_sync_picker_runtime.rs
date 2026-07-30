//! Database-backed inputs and writes for the three device-content pickers.
//!
//! The picker owns no durable selection state. Every snapshot reads the
//! existing playlist, subscription-device, and `wanted_on_device` flags, and
//! every save applies deltas to those same flags.

use std::collections::{HashMap, HashSet};

use reprise_core::connectivity::LocalAvailability;
use reprise_core::device_sync::{
    resolve_latest_per_channel, select_episodes, DeviceSelection, EpisodeSelectionCandidate,
    EpisodeSelectionRule, MirrorPlaylistSnapshot, MirrorTrack, SelectionSource, SyncTargetKind,
    EVERYTHING_SOURCE,
};
use reprise_core::podcasts::{PodcastKind, SourceGroup};

use super::*;

pub(crate) const KEEP_SMART_UPDATED_KEY: &str = "device_sync.keep_smart_playlists_updated";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PickerSnapshot {
    Playlists {
        rows: Vec<PickerPlaylistRow>,
        keep_smart_updated: bool,
    },
    Episodes {
        kind: SyncTargetKind,
        latest_per_group: usize,
        groups: Vec<PickerEpisodeGroup>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PickerPlaylistRow {
    pub source: SelectionSource,
    pub name: String,
    pub smart: bool,
    pub selected: bool,
    pub track_count: usize,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PickerEpisodeGroup {
    pub id: i64,
    pub name: String,
    pub enabled: bool,
    pub latest_override: Option<usize>,
    pub episodes: Vec<PickerEpisodeRow>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PickerEpisodeRow {
    pub id: i64,
    pub title: String,
    pub published_at: Option<i64>,
    pub duration_secs: Option<i64>,
    pub position_ms: i64,
    pub size_bytes: Option<u64>,
    pub downloaded: bool,
    pub played: bool,
    pub pinned: bool,
    pub selected: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PickerSave {
    pub playlist_changes: Vec<(SelectionSource, bool)>,
    pub group_changes: Vec<(i64, bool)>,
    pub episode_pin_changes: Vec<(i64, bool)>,
    pub latest_per_channel: Option<usize>,
    pub keep_smart_updated: Option<bool>,
}

impl DeviceSyncRuntime {
    pub(crate) fn picker_snapshot(
        &self,
        device_id: &str,
        kind: SyncTargetKind,
    ) -> Result<PickerSnapshot, String> {
        match kind {
            SyncTargetKind::Playlists => self.playlist_picker_snapshot(device_id),
            SyncTargetKind::YoutubeAudio => {
                self.episode_picker_snapshot(device_id, PodcastKind::Youtube)
            }
            SyncTargetKind::PodcastEpisodes => {
                self.episode_picker_snapshot(device_id, PodcastKind::Rss)
            }
        }
    }

    fn playlist_picker_snapshot(&self, device_id: &str) -> Result<PickerSnapshot, String> {
        let device = self
            .device_states
            .borrow()
            .iter()
            .find(|device| device.descriptor.id == device_id)
            .map(DeviceState::view)
            .ok_or_else(|| "device is not connected".to_string())?;
        let mut rows = device
            .page
            .playlists
            .into_iter()
            .filter(|row| row.available)
            .map(picker_playlist_row)
            .collect::<Vec<_>>();
        let everything_selected = device.settings.selection == DeviceSelection::EntireLibrary;
        let everything = reprise_core::device_sync::load_everything_playlist_snapshot(&self.conn)
            .map_err(|error| error.to_string())?;
        let everything_row = reprise_core::device_sync::project_sync_page(
            reprise_core::device_sync::SyncPageInput {
                selected: everything_selected
                    .then_some(EVERYTHING_SOURCE)
                    .into_iter()
                    .collect(),
                playlists: vec![everything],
                profile: device.settings.profile,
                ..Default::default()
            },
        )
        .page
        .playlists
        .into_iter()
        .next()
        .ok_or_else(|| "Everything playlist projection is missing".to_string())?;
        rows.insert(0, picker_playlist_row(everything_row));
        let keep_smart_updated =
            reprise_core::library::settings::get_bool(&self.conn, KEEP_SMART_UPDATED_KEY, true)
                .map_err(|error| error.to_string())?;
        Ok(PickerSnapshot::Playlists {
            rows,
            keep_smart_updated,
        })
    }

    fn episode_picker_snapshot(
        &self,
        device_id: &str,
        podcast_kind: PodcastKind,
    ) -> Result<PickerSnapshot, String> {
        let kind = match podcast_kind {
            PodcastKind::Youtube => SyncTargetKind::YoutubeAudio,
            PodcastKind::Rss => SyncTargetKind::PodcastEpisodes,
        };
        let groups = reprise_core::podcasts::query::list_source_groups(&self.conn, podcast_kind)
            .map_err(|error| error.to_string())?;
        let default_latest = reprise_core::podcasts::config::load(&self.conn)
            .map_err(|error| error.to_string())?
            .latest_per_channel_default;
        let group_ids = groups
            .iter()
            .map(|group| group.subscription_id)
            .collect::<Vec<_>>();
        let latest_overrides =
            reprise_core::podcasts::store::latest_per_channel_overrides(&self.conn, &group_ids)
                .map_err(|error| error.to_string())?;
        let enabled = enabled_group_ids(&self.conn, device_id, &groups)?;
        let pins = episode_pins(&self.conn, &groups)?;
        let candidates = picker_candidates(&groups, &pins);
        let rule = match podcast_kind {
            PodcastKind::Youtube => EpisodeSelectionRule::LatestPerChannel {
                channel_latest: enabled
                    .iter()
                    .map(|group_id| {
                        (
                            *group_id,
                            resolve_latest_per_channel(
                                default_latest,
                                latest_overrides.get(group_id).copied(),
                            ),
                        )
                    })
                    .collect(),
            },
            PodcastKind::Rss => EpisodeSelectionRule::UnplayedDownloadsOnly {
                enabled_shows: enabled.clone(),
            },
        };
        let selection = select_episodes(&candidates, &rule);
        let selected = selection
            .ready
            .into_iter()
            .chain(selection.waiting)
            .collect::<HashSet<_>>();
        let groups = groups
            .into_iter()
            .map(|group| PickerEpisodeGroup {
                id: group.subscription_id,
                name: group.title,
                enabled: enabled.contains(&group.subscription_id),
                latest_override: latest_overrides
                    .get(&group.subscription_id)
                    .and_then(|value| usize::try_from(*value).ok()),
                episodes: group
                    .episodes
                    .into_iter()
                    .map(|episode| PickerEpisodeRow {
                        id: episode.id,
                        title: episode.title,
                        published_at: episode.published_at,
                        duration_secs: episode.duration_secs,
                        position_ms: episode.position_ms,
                        size_bytes: episode
                            .downloaded_bytes
                            .and_then(|bytes| u64::try_from(bytes).ok()),
                        downloaded: episode.downloaded_path.is_some(),
                        played: episode.played_at.is_some(),
                        pinned: pins.get(&episode.id).copied().unwrap_or(false),
                        selected: selected.contains(&episode.id),
                    })
                    .collect(),
            })
            .collect();
        Ok(PickerSnapshot::Episodes {
            kind,
            latest_per_group: default_latest,
            groups,
        })
    }

    pub(crate) fn save_picker(
        self: &Rc<Self>,
        device_id: &str,
        changes: PickerSave,
    ) -> Result<(), String> {
        let keep_smart_updated_now =
            reprise_core::library::settings::get_bool(&self.conn, KEEP_SMART_UPDATED_KEY, true)
                .map_err(|error| error.to_string())?;
        let newly_selected_smart = changes
            .playlist_changes
            .iter()
            .filter_map(|(source, selected)| {
                (*selected
                    && matches!(source, SelectionSource::Smart(_))
                    && source != &EVERYTHING_SOURCE)
                    .then_some(source.clone())
            })
            .collect::<Vec<_>>();
        if !keep_smart_updated_now && !newly_selected_smart.is_empty() {
            capture_frozen_smart_snapshots(&self.conn, device_id, &newly_selected_smart)?;
        }
        if changes.keep_smart_updated == Some(false) {
            let settings = self.settings_for_update(device_id)?;
            let selected = match settings.selection {
                DeviceSelection::Sources(sources) => sources,
                DeviceSelection::EntireLibrary => Vec::new(),
            };
            capture_frozen_smart_snapshots(&self.conn, device_id, &selected)?;
        }
        if !changes.playlist_changes.is_empty() {
            let mut settings = self.settings_for_update(device_id)?;
            let mut sources = match settings.selection {
                DeviceSelection::Sources(sources) => sources,
                DeviceSelection::EntireLibrary => Vec::new(),
            };
            let select_everything = changes
                .playlist_changes
                .iter()
                .any(|(source, selected)| source == &EVERYTHING_SOURCE && *selected);
            for (source, selected) in changes.playlist_changes {
                if source == EVERYTHING_SOURCE {
                    continue;
                }
                sources.retain(|candidate| candidate != &source);
                if selected {
                    sources.push(source);
                }
            }
            settings.selection = if select_everything {
                DeviceSelection::EntireLibrary
            } else {
                DeviceSelection::Sources(sources)
            };
            self.update_settings(settings)?;
        }
        for (group_id, enabled) in changes.group_changes {
            reprise_core::podcasts::phone_sync::set_device_enabled(
                &self.conn, group_id, device_id, enabled,
            )
            .map_err(|error| error.to_string())?;
        }
        for (episode_id, pinned) in changes.episode_pin_changes {
            reprise_core::podcasts::wanted_on_device::set_wanted_on_device(
                &self.conn, episode_id, pinned,
            )
            .map_err(|error| error.to_string())?;
        }
        if let Some(latest) = changes.latest_per_channel {
            reprise_core::library::settings::set_setting(
                &self.conn,
                reprise_core::podcasts::config::LATEST_PER_CHANNEL_DEFAULT_KEY,
                &latest.min(100).to_string(),
            )
            .map_err(|error| error.to_string())?;
        }
        if let Some(keep_updated) = changes.keep_smart_updated {
            reprise_core::library::settings::set_bool(
                &self.conn,
                KEEP_SMART_UPDATED_KEY,
                keep_updated,
            )
            .map_err(|error| error.to_string())?;
        }
        self.recompute_delta(device_id)
    }
}

fn picker_playlist_row(row: reprise_core::device_sync::SyncPlaylistRow) -> PickerPlaylistRow {
    PickerPlaylistRow {
        smart: matches!(row.source, SelectionSource::Smart(_)) && row.source != EVERYTHING_SOURCE,
        source: row.source,
        name: row.name.unwrap_or_else(|| {
            super::device_sync_strings::text(super::device_sync_strings::UNAVAILABLE_PLAYLIST)
        }),
        selected: row.selected,
        track_count: row.unique_track_count,
        size_bytes: row.target_bytes,
    }
}

fn enabled_group_ids(
    db: &Db,
    device_id: &str,
    groups: &[SourceGroup],
) -> Result<HashSet<i64>, String> {
    groups
        .iter()
        .filter_map(|group| {
            let result =
                reprise_core::podcasts::phone_sync::selected_device_ids(db, group.subscription_id);
            match result {
                Ok(ids) => ids
                    .iter()
                    .any(|candidate| candidate == device_id)
                    .then_some(Ok(group.subscription_id)),
                Err(error) => Some(Err(error.to_string())),
            }
        })
        .collect()
}

fn episode_pins(db: &Db, groups: &[SourceGroup]) -> Result<HashMap<i64, bool>, String> {
    groups
        .iter()
        .flat_map(|group| group.episodes.iter())
        .map(|episode| {
            reprise_core::podcasts::wanted_on_device::wanted_on_device(db, episode.id)
                .map(|wanted| (episode.id, wanted.unwrap_or(false)))
                .map_err(|error| error.to_string())
        })
        .collect()
}

fn picker_candidates(
    groups: &[SourceGroup],
    pins: &HashMap<i64, bool>,
) -> Vec<EpisodeSelectionCandidate> {
    groups
        .iter()
        .flat_map(|group| group.episodes.iter())
        .filter(|episode| {
            episode.downloaded_path.is_some() || pins.get(&episode.id).copied().unwrap_or(false)
        })
        .map(|episode| EpisodeSelectionCandidate {
            episode_id: episode.id,
            group_id: episode.subscription_id,
            published_at: episode.published_at.unwrap_or(episode.first_seen_at),
            played: episode.played_at.is_some(),
            local: if episode.downloaded_path.is_some() {
                LocalAvailability::Available
            } else {
                LocalAvailability::Missing
            },
            pinned: pins.get(&episode.id).copied().unwrap_or(false),
        })
        .collect()
}

pub(super) fn apply_frozen_smart_snapshots(
    db: &Db,
    device_id: &str,
    selected: &[SelectionSource],
    snapshots: &mut [MirrorPlaylistSnapshot],
) -> Result<(HashSet<SelectionSource>, HashSet<i64>), String> {
    if reprise_core::library::settings::get_bool(db, KEEP_SMART_UPDATED_KEY, true)
        .map_err(|error| error.to_string())?
    {
        return Ok((HashSet::new(), HashSet::new()));
    }

    let mut frozen = HashSet::new();
    let mut frozen_track_ids = HashSet::new();
    for source in selected {
        let SelectionSource::Smart(id) = source else {
            continue;
        };
        if source == &EVERYTHING_SOURCE {
            continue;
        }
        let key = frozen_smart_key(device_id, *id);
        let live = snapshots
            .iter()
            .find(|snapshot| &snapshot.source == source)
            .map(|snapshot| snapshot.entries.as_slice())
            .unwrap_or_default();
        let ids = match reprise_core::library::settings::get_setting(db, &key)
            .map_err(|error| error.to_string())?
        {
            Some(json) => serde_json::from_str::<Vec<i64>>(&json)
                .map_err(|error| format!("invalid frozen smart-playlist snapshot: {error}"))?,
            None => {
                let ids = live
                    .iter()
                    .map(|track| match track {
                        MirrorTrack::Available(track) => track.id,
                        MirrorTrack::Unavailable(track) => track.track_id,
                    })
                    .collect::<Vec<_>>();
                let json = serde_json::to_string(&ids).map_err(|error| error.to_string())?;
                reprise_core::library::settings::set_setting(db, &key, &json)
                    .map_err(|error| error.to_string())?;
                ids
            }
        };
        let tracks = reprise_core::queries::query_sync_tracks(db, &ids)
            .map_err(|error| error.to_string())?;
        if let Some(snapshot) = snapshots
            .iter_mut()
            .find(|snapshot| &snapshot.source == source)
        {
            snapshot.entries = tracks.into_iter().map(MirrorTrack::Available).collect();
        }
        frozen_track_ids.extend(ids);
        frozen.insert(source.clone());
    }
    Ok((frozen, frozen_track_ids))
}

fn frozen_smart_key(device_id: &str, playlist_id: i64) -> String {
    let device = device_id
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("device_sync.frozen_smart.{device}.{playlist_id}")
}

fn capture_frozen_smart_snapshots(
    db: &Db,
    device_id: &str,
    sources: &[SelectionSource],
) -> Result<(), String> {
    let snapshots = reprise_core::device_sync::load_mirror_playlist_snapshots(db)
        .map_err(|error| error.to_string())?;
    for source in sources {
        let SelectionSource::Smart(id) = source else {
            continue;
        };
        if source == &EVERYTHING_SOURCE {
            continue;
        }
        let ids = snapshots
            .iter()
            .find(|snapshot| &snapshot.source == source)
            .map(|snapshot| {
                snapshot
                    .entries
                    .iter()
                    .map(|track| match track {
                        MirrorTrack::Available(track) => track.id,
                        MirrorTrack::Unavailable(track) => track.track_id,
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let json = serde_json::to_string(&ids).map_err(|error| error.to_string())?;
        reprise_core::library::settings::set_setting(db, &frozen_smart_key(device_id, *id), &json)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}
