use std::collections::{HashMap, HashSet};

use reprise_core::connectivity::Connectivity;
use reprise_core::device_sync::podcasts::{
    build_plan as build_podcast_plan, query_candidates_for_device,
    query_selection_candidates_for_device, PodcastDeviceFile, PodcastSyncSource,
};
use reprise_core::device_sync::preparation::MissingFile;
use reprise_core::device_sync::settings::{
    load_device_files, load_device_playlists, resolve_selection_track_ids, save_settings,
};
use reprise_core::device_sync::targets::{load_target, save_target};
use reprise_core::device_sync::{
    load_mirror_playlist_snapshots, load_or_create_targets, plan_preparation, project_storage,
    project_sync_page, resolve_latest_per_channel, select_episodes, DeviceSelection,
    EpisodeSelectionCandidate, EpisodeSelectionResult, EpisodeSelectionRule, PreparationFacts,
    SelectionSource, SyncPageInput, SyncTarget, SyncTargetKind, TransferProfile,
};

use super::*;

impl DeviceSyncRuntime {
    pub fn update_settings(self: &Rc<Self>, settings: DeviceSettings) -> Result<(), String> {
        {
            let devices = self.device_states.borrow();
            let device = devices
                .iter()
                .find(|device| device.descriptor.id == settings.device_serial)
                .ok_or_else(|| "device is not connected".to_string())?;
            if device.is_busy() {
                return Err("device synchronization is active".into());
            }
        }
        save_settings(&self.conn, &settings).map_err(|error| error.to_string())?;
        let device_id = settings.device_serial.clone();
        {
            let mut devices = self.device_states.borrow_mut();
            let Some(device) = devices
                .iter_mut()
                .find(|device| device.descriptor.id == device_id)
            else {
                return Err("device is not connected".into());
            };
            device.settings = settings;
            device.sync_phase = PlannedSyncPhase::ComputingDelta;
            device.sync_error = None;
        }
        self.recompute_delta(&device_id)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_transfer_profile(
        self: &Rc<Self>,
        device_id: &str,
        profile: TransferProfile,
    ) -> Result<(), String> {
        let mut settings = self.settings_for_update(device_id)?;
        settings.profile = profile;
        self.update_settings(settings)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_playlist_selected(
        self: &Rc<Self>,
        device_id: &str,
        source: SelectionSource,
        selected: bool,
    ) -> Result<(), String> {
        let mut settings = self.settings_for_update(device_id)?;
        let mut sources = match settings.selection {
            DeviceSelection::Sources(sources) => sources,
            DeviceSelection::EntireLibrary => Vec::new(),
        };
        sources.retain(|candidate| candidate != &source);
        if selected {
            sources.push(source);
        }
        settings.selection = DeviceSelection::Sources(sources);
        self.update_settings(settings)
    }

    /// "Remove from phone when deleted or unsubscribed here" (design 7a).
    /// Per-device, like every other sync rule since `E-6` moved them onto
    /// the device page — see `db_device_sync::migrate_v44`'s doc comment.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_remove_deleted(
        self: &Rc<Self>,
        device_id: &str,
        remove_deleted: bool,
    ) -> Result<(), String> {
        let mut settings = self.settings_for_update(device_id)?;
        settings.remove_deleted = remove_deleted;
        self.update_settings(settings)
    }

    /// "Sync automatically when this phone connects" (design 7a) — likewise
    /// per-device.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_sync_automatically(
        self: &Rc<Self>,
        device_id: &str,
        sync_automatically: bool,
    ) -> Result<(), String> {
        let mut settings = self.settings_for_update(device_id)?;
        settings.sync_automatically = sync_automatically;
        self.update_settings(settings)
    }

    /// "Download missing files before syncing" (design 7f, `MTP-43`) —
    /// likewise per-device, beside `remove_deleted`/`sync_automatically`.
    /// Only ever stores what the user chose: offline and metered overrides
    /// are `preparation::plan_preparation`'s job, never a mutation of this
    /// stored value.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_prepare_before_sync(
        self: &Rc<Self>,
        device_id: &str,
        prepare_before_sync: bool,
    ) -> Result<(), String> {
        let mut settings = self.settings_for_update(device_id)?;
        settings.prepare_before_sync = prepare_before_sync;
        self.update_settings(settings)
    }

    /// `MTP-38`'s one per-device toggle exposed by the Content section
    /// (`MTP-37`): whether this device's slot for `kind` is active at all.
    /// `E-6` withdrew the once-planned global "sync this content type" rule
    /// outright, so this switch owns its section without a second,
    /// higher-priority one anywhere — see
    /// `device_view::project_device_category_reading`.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_target_enabled(
        self: &Rc<Self>,
        device_id: &str,
        kind: SyncTargetKind,
        enabled: bool,
    ) -> Result<(), String> {
        {
            let devices = self.device_states.borrow();
            let device = devices
                .iter()
                .find(|device| device.descriptor.id == device_id)
                .ok_or_else(|| "device is not connected".to_string())?;
            if device.is_busy() {
                return Err("device synchronization is active".into());
            }
        }
        let mut target = {
            let conn = &self.conn;
            load_target(conn, device_id, kind)
                .map_err(|error| error.to_string())?
                .unwrap_or_else(|| SyncTarget::default_for(kind))
        };
        target.enabled = enabled;
        save_target(&self.conn, device_id, &target).map_err(|error| error.to_string())?;
        self.recompute_delta(device_id)
    }

    /// `MTP-37` (`E-6`, `E-8`): the Content section's size-cap column
    /// becomes a real per-device control here — before this, `cap_bytes`
    /// was persisted (`MTP-38`) and enforced by `build_plan`'s eviction
    /// pass (`MTP-39`/`MTP-25`) but had no editing surface anywhere, so a
    /// user could never actually change it. `None` clears the cap
    /// (unlimited); `Some` sets it in bytes. Playlists have no cap concept
    /// (`MTP-38`'s `default_cap_bytes`) and no eviction path reads a
    /// playlist cap, so the GTK side never offers this control for that
    /// kind — this method does not special-case it either way, it simply
    /// persists whatever is asked and lets the existing eviction pass
    /// (which already ignores `None`) do the rest.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_target_cap(
        self: &Rc<Self>,
        device_id: &str,
        kind: SyncTargetKind,
        cap_bytes: Option<u64>,
    ) -> Result<(), String> {
        {
            let devices = self.device_states.borrow();
            let device = devices
                .iter()
                .find(|device| device.descriptor.id == device_id)
                .ok_or_else(|| "device is not connected".to_string())?;
            if device.is_busy() {
                return Err("device synchronization is active".into());
            }
        }
        let mut target = {
            let conn = &self.conn;
            load_target(conn, device_id, kind)
                .map_err(|error| error.to_string())?
                .unwrap_or_else(|| SyncTarget::default_for(kind))
        };
        target.cap_bytes = cap_bytes;
        save_target(&self.conn, device_id, &target).map_err(|error| error.to_string())?;
        self.recompute_delta(device_id)
    }

    pub fn selection_options(&self) -> Result<Vec<DeviceSelectionOption>, String> {
        let conn = &self.conn;
        let mut options = reprise_core::library::playlists::list(conn)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|playlist| DeviceSelectionOption {
                source: SelectionSource::Playlist(playlist.id),
                name: playlist.name,
                track_count: usize::try_from(playlist.track_count.max(0)).unwrap_or(usize::MAX),
                smart: false,
            })
            .collect::<Vec<_>>();
        for playlist in
            reprise_core::library::playlists::list_smart(conn).map_err(|error| error.to_string())?
        {
            let source = SelectionSource::Smart(playlist.id);
            let count =
                resolve_selection_track_ids(conn, &DeviceSelection::Sources(vec![source.clone()]))
                    .map_err(|error| error.to_string())?
                    .len();
            options.push(DeviceSelectionOption {
                source,
                name: playlist.name,
                track_count: count,
                smart: true,
            });
        }
        Ok(options)
    }

    /// `MTP-46`/`SET-4`: re-plans every connected device because something
    /// outside the device page changed what may sync — today the two source
    /// module switches on the "Online sources" page. Without this, an open
    /// device page keeps showing a switched-off source's Content row until
    /// something else happens to trigger a recompute, which `SET-4` (settings
    /// take effect immediately) does not allow.
    ///
    /// A device that is busy is skipped rather than interrupted: its own
    /// recompute runs when it finishes, and the alternative is cancelling a
    /// transfer the user started because they flipped an unrelated switch.
    pub fn recompute_all_devices(self: &Rc<Self>) {
        let device_ids = self
            .device_states
            .borrow()
            .iter()
            .filter(|device| !device.is_busy())
            .map(|device| device.descriptor.id.clone())
            .collect::<Vec<_>>();
        for device_id in device_ids {
            if let Err(error) = self.recompute_delta(&device_id) {
                tracing::warn!(device_id, %error, "could not re-plan after a settings change");
            }
        }
    }

    pub fn recompute_delta(self: &Rc<Self>, device_id: &str) -> Result<(), String> {
        let result = self.recompute_delta_silent(device_id);
        if result.is_ok() {
            self.notify();
        }
        result
    }

    pub(super) fn recompute_delta_silent(self: &Rc<Self>, device_id: &str) -> Result<(), String> {
        let (settings, storage, managed_files, podcast_files, youtube_files) = self
            .device_states
            .borrow()
            .iter()
            .find(|device| device.descriptor.id == device_id)
            .map(|device| {
                (
                    device.settings.clone(),
                    device.storage.clone(),
                    device.managed_files.clone(),
                    device.podcast_files.clone(),
                    device.youtube_files.clone(),
                )
            })
            .ok_or_else(|| "device is not connected".to_string())?;
        let selected = match &settings.selection {
            DeviceSelection::Sources(sources) => sources.clone(),
            DeviceSelection::EntireLibrary => Vec::new(),
        };
        let (
            mut projection,
            podcast_plan,
            youtube_plan,
            podcast_waiting,
            youtube_waiting,
            managed_track_count,
            targets,
            youtube_selection_summary,
            podcast_selection_summary,
            preparation_phase,
            preparation_missing,
            enabled_sources,
        ) = {
            let conn = &self.conn;
            let files = load_device_files(conn, device_id).map_err(|error| error.to_string())?;
            let managed_track_count = files.len();
            let playlist_inventory =
                load_device_playlists(conn, device_id).map_err(|error| error.to_string())?;
            let playlists =
                load_mirror_playlist_snapshots(conn).map_err(|error| error.to_string())?;
            let projection = project_sync_page(SyncPageInput {
                selected,
                playlists,
                profile: settings.profile,
                inventory: files,
                playlist_inventory,
                managed_files,
                storage: storage.clone(),
            });
            let targets =
                load_or_create_targets(conn, device_id).map_err(|error| error.to_string())?;
            let podcast_inventory = as_podcast_device_files(&podcast_files);
            let youtube_inventory = as_podcast_device_files(&youtube_files);
            // Both kinds are queried once and each `build_plan` call filters
            // by its own `PodcastSyncSource` — the same candidate set feeds
            // both target plans, mirroring how RSS and YouTube are equally
            // eligible for phone sync (`POD-12`).
            let candidates =
                query_candidates_for_device(conn, device_id).map_err(|error| error.to_string())?;
            // `MTP-45`: `select_episodes` — not the raw downloaded-file
            // query above — decides which episodes are actually wanted
            // (unplayed RSS episodes, latest-per-channel YouTube episodes)
            // and splits wanted-but-missing ones into `waiting` instead of
            // letting them vanish from the balance (`MTP-40`).
            let selection_candidates = query_selection_candidates_for_device(conn, device_id)
                .map_err(|error| error.to_string())?;
            // `MTP-46`: the same switches the query above already honours,
            // read once more here so the Content rows can hide a source the
            // user switched off. Reading them rather than inferring them
            // from an empty candidate list: "no episodes" and "not a feature
            // you use" are different states, and only the second hides a row.
            let enabled_sources = reprise_core::device_sync::podcasts::enabled_sync_sources(conn)
                .map_err(|error| error.to_string())?;
            // `MTP-36`: the global default plus every enabled YouTube
            // channel's persisted override — resolved here (the only place
            // with DB access) and handed to `plan_episode_selection` as
            // plain data, keeping that function pure and unit-testable.
            let default_latest_per_channel = reprise_core::podcasts::config::load(conn)
                .map_err(|error| error.to_string())?
                .latest_per_channel_default;
            let youtube_channel_ids = selection_candidates
                .iter()
                .filter(|(source, _)| *source == PodcastSyncSource::Youtube)
                .map(|(_, candidate)| candidate.group_id)
                .collect::<Vec<_>>();
            let latest_per_channel_overrides =
                reprise_core::podcasts::store::latest_per_channel_overrides(
                    conn,
                    &youtube_channel_ids,
                )
                .map_err(|error| error.to_string())?;
            let (rss_selection, youtube_selection) = plan_episode_selection(
                &selection_candidates,
                default_latest_per_channel,
                &latest_per_channel_overrides,
            );
            let ready_ids = rss_selection
                .ready
                .iter()
                .chain(youtube_selection.ready.iter())
                .copied()
                .collect::<HashSet<_>>();
            let candidates = candidates
                .into_iter()
                .filter(|candidate| ready_ids.contains(&candidate.episode_id))
                .collect::<Vec<_>>();
            let podcast_plan = target_podcast_plan(
                &targets,
                SyncTargetKind::PodcastEpisodes,
                candidates.clone(),
                &podcast_inventory,
                PodcastSyncSource::Rss,
                settings.remove_deleted,
                enabled_sources,
            );
            let youtube_plan = target_podcast_plan(
                &targets,
                SyncTargetKind::YoutubeAudio,
                candidates,
                &youtube_inventory,
                PodcastSyncSource::Youtube,
                settings.remove_deleted,
                enabled_sources,
            );
            // `MTP-37`: the Content section's live "N of M ... selected"
            // read, sourced straight from `POD-12`'s per-device selection —
            // not a second selection surface, just an honest count of the
            // one that already exists.
            let (youtube_selected, youtube_total) =
                reprise_core::podcasts::phone_sync::selection_summary(
                    conn,
                    device_id,
                    reprise_core::podcasts::PodcastKind::Youtube,
                )
                .map_err(|error| error.to_string())?;
            let (podcast_selected, podcast_total) =
                reprise_core::podcasts::phone_sync::selection_summary(
                    conn,
                    device_id,
                    reprise_core::podcasts::PodcastKind::Rss,
                )
                .map_err(|error| error.to_string())?;
            let youtube_selection_summary = reprise_core::device_sync::YoutubeSelectionSummary {
                channels_selected: youtube_selected,
                channels_total: youtube_total,
                // `MTP-36`: the global default — channels may individually
                // override it (`latest_per_channel_overrides` above), but
                // this one-line summary has room for a single number, so it
                // reports the default that applies absent an override, the
                // same simplification `MTP-37`'s cap row already makes for
                // per-target settings.
                latest_per_channel: default_latest_per_channel,
            };
            let podcast_selection_summary = reprise_core::device_sync::PodcastSelectionSummary {
                shows_selected: podcast_selected,
                shows_total: podcast_total,
            };
            // `MTP-42`/`MTP-43`: the same `waiting` set that already feeds
            // `podcast_waiting`/`youtube_waiting` above is the preparation
            // phase's missing-file list — gathered once here (one query per
            // episode for its title) rather than re-deriving `waiting` a
            // second time from scratch.
            let preparation_missing = gather_missing_files(
                self.conn.as_ref(),
                rss_selection
                    .waiting
                    .iter()
                    .chain(youtube_selection.waiting.iter())
                    .copied(),
            );
            let preparation_phase = plan_preparation(&PreparationFacts {
                missing: preparation_missing.clone(),
                connectivity: current_connectivity(),
                metered: gio::NetworkMonitor::default().is_network_metered(),
                online_sources_enabled: reprise_core::online_sources::is_enabled(conn)
                    .unwrap_or(true),
                prepare_switch_on: settings.prepare_before_sync,
            });
            (
                projection,
                podcast_plan,
                youtube_plan,
                rss_selection.waiting.len(),
                youtube_selection.waiting.len(),
                managed_track_count,
                targets,
                youtube_selection_summary,
                podcast_selection_summary,
                preparation_phase,
                preparation_missing,
                enabled_sources,
            )
        };
        projection.plan.transfer_bytes = projection
            .plan
            .transfer_bytes
            .saturating_add(podcast_plan.bytes)
            .saturating_add(youtube_plan.bytes);
        if podcast_plan.selected > 0 || youtube_plan.selected > 0 {
            projection.plan.blockers.retain(|blocker| {
                blocker != &reprise_core::device_sync::MirrorBlocker::NoPlaylistsSelected
            });
        }
        projection.page.changes.transfer_bytes = projection.plan.transfer_bytes;
        projection.page.blockers = projection.plan.blockers.clone();
        projection.page.storage = project_storage(&storage, &projection.plan);
        if let Some(device) = self
            .device_states
            .borrow_mut()
            .iter_mut()
            .find(|device| device.descriptor.id == device_id)
        {
            device.managed_track_count = managed_track_count;
            device.mirror_plan = projection.plan;
            device.podcast_plan = podcast_plan;
            device.youtube_plan = youtube_plan;
            device.podcast_waiting = podcast_waiting;
            device.youtube_waiting = youtube_waiting;
            device.targets = targets;
            device.youtube_selection = youtube_selection_summary;
            device.podcast_selection = podcast_selection_summary;
            device.enabled_sources = enabled_sources;
            device.preparation = preparation_phase;
            device.preparation_missing = preparation_missing;
            device.page = projection.page;
            device.sync_phase = PlannedSyncPhase::Idle;
        }
        Ok(())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn settings_for_update(&self, device_id: &str) -> Result<DeviceSettings, String> {
        self.device_states
            .borrow()
            .iter()
            .find(|device| device.descriptor.id == device_id)
            .map(|device| device.settings.clone())
            .ok_or_else(|| "device is not connected".to_string())
    }
}

/// Builds one target's podcast/YouTube plan, or an empty one when that
/// target is switched off for this device (`SyncTarget::enabled`) — a
/// disabled target has no active slot for its category regardless of what
/// candidates or inventory exist.
///
/// `remove_deleted` is the per-device "Remove from phone when deleted or
/// unsubscribed here" switch (`DeviceSettings::remove_deleted`, design 7a).
/// Before this fix the switch was rendered and persisted but every planning
/// call hard-coded `true`, so turning it off never actually kept an
/// unsubscribed episode on the phone.
#[allow(clippy::too_many_arguments)]
fn target_podcast_plan(
    targets: &[reprise_core::device_sync::SyncTarget; 3],
    kind: SyncTargetKind,
    candidates: Vec<reprise_core::device_sync::podcasts::PodcastSyncCandidate>,
    inventory: &[PodcastDeviceFile],
    source: PodcastSyncSource,
    remove_deleted: bool,
    enabled: reprise_core::device_sync::podcasts::EnabledSyncSources,
) -> reprise_core::device_sync::podcasts::PodcastSyncPlan {
    let Some(target) = targets.iter().find(|target| target.kind == kind) else {
        return reprise_core::device_sync::podcasts::PodcastSyncPlan::default();
    };
    if !target.enabled {
        return reprise_core::device_sync::podcasts::PodcastSyncPlan::default();
    }
    build_podcast_plan(
        candidates,
        inventory,
        remove_deleted,
        source,
        target.cap_bytes,
        enabled,
    )
}

fn as_podcast_device_files(files: &[ManagedDeviceFile]) -> Vec<PodcastDeviceFile> {
    files
        .iter()
        .map(|file| PodcastDeviceFile {
            device_path: file.relative_path.clone(),
            size_bytes: file.size_bytes,
        })
        .collect()
}

/// `MTP-45`/`MTP-36`: runs each `PodcastSyncSource`'s own selection rule
/// (`UnplayedDownloadsOnly` for RSS, `LatestPerChannel` for YouTube) over
/// `query_selection_candidates_for_device`'s combined candidate list, one
/// rule per source. `enabled_shows`/YouTube's channel set are simply every
/// distinct `group_id` present in that source's candidates — the DB query
/// already scoped candidates to shows/channels selected for this device
/// (`podcast_subscription_devices`, `POD-12`), so nothing here re-derives
/// that scoping.
///
/// Each enabled YouTube channel's cap is resolved from `default_latest`
/// (the global "latest N per channel" setting) and `latest_overrides`
/// (persisted per-channel overrides, keyed by subscription id) via
/// [`resolve_latest_per_channel`] — a missing entry in `latest_overrides`
/// falls back to `default_latest`, and `0` means unlimited. Both inputs are
/// plain data the caller already read from the DB, so this function itself
/// stays pure and DB-free like the rest of `selection`.
fn plan_episode_selection(
    candidates: &[(PodcastSyncSource, EpisodeSelectionCandidate)],
    default_latest: usize,
    latest_overrides: &HashMap<i64, i64>,
) -> (EpisodeSelectionResult, EpisodeSelectionResult) {
    let by_source = |source: PodcastSyncSource| {
        candidates
            .iter()
            .filter(move |(candidate_source, _)| *candidate_source == source)
            .map(|(_, candidate)| candidate.clone())
            .collect::<Vec<_>>()
    };
    let rss = by_source(PodcastSyncSource::Rss);
    let youtube = by_source(PodcastSyncSource::Youtube);
    let rss_shows = rss.iter().map(|candidate| candidate.group_id).collect();
    let youtube_channels = youtube
        .iter()
        .map(|candidate| candidate.group_id)
        .collect::<HashSet<_>>();
    let rss_result = select_episodes(
        &rss,
        &EpisodeSelectionRule::UnplayedDownloadsOnly {
            enabled_shows: rss_shows,
        },
    );
    let channel_latest = youtube_channels
        .into_iter()
        .map(|channel_id| {
            let resolved = resolve_latest_per_channel(
                default_latest,
                latest_overrides.get(&channel_id).copied(),
            );
            (channel_id, resolved)
        })
        .collect();
    let youtube_result = select_episodes(
        &youtube,
        &EpisodeSelectionRule::LatestPerChannel { channel_latest },
    );
    (rss_result, youtube_result)
}

/// `MTP-43`'s preparation overview needs episode titles, which
/// `EpisodeSelectionResult::waiting` (bare `i64` ids) does not carry — this
/// is the one extra read per missing episode that supplies them. A row that
/// no longer exists (deleted in the moment between the selection query above
/// and this lookup) is skipped rather than failing the whole recompute.
///
/// `size_bytes` is always `0`: no feed or provider in this codebase persists
/// an expected byte size for an episode before it is downloaded (RSS
/// enclosure `length` attributes and YouTube's size are both parsed
/// nowhere), so the combined size the overview reports only ever reflects
/// episodes whose size happens to be already known — which today is none.
/// Wiring that up is future work, not a decision this projection can make up
/// on its own.
fn gather_missing_files(db: &Db, episode_ids: impl IntoIterator<Item = i64>) -> Vec<MissingFile> {
    let conn = &db;
    episode_ids
        .into_iter()
        .filter_map(
            |episode_id| match reprise_core::podcasts::store::episode(conn, episode_id) {
                Ok(Some(episode)) => Some(MissingFile {
                    episode_id,
                    title: episode.title,
                    size_bytes: 0,
                }),
                Ok(None) => None,
                Err(error) => {
                    tracing::warn!(
                        episode_id,
                        %error,
                        "could not read a wanted episode for the preparation overview"
                    );
                    None
                }
            },
        )
        .collect()
}

/// `NET-3a`: the app's current connectivity belief, read from the one real
/// OS signal `podcast_refresh_scheduler.rs` already uses for "metered"
/// (`gio::NetworkMonitor`) — not a guess after a failed request, which
/// `reprise_core::connectivity`'s module docs explicitly rule out.
fn current_connectivity() -> Connectivity {
    if gio::NetworkMonitor::default().is_network_available() {
        Connectivity::Online
    } else {
        Connectivity::Offline
    }
}
