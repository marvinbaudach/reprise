//! Toolkit-neutral projection for the compact per-device sync surface.

use std::collections::{HashMap, HashSet};

use super::{
    plan_mirror, project_storage, DeviceFileRecord, DevicePlaylistRecord, DeviceStorageAccess,
    DeviceStorageProjection, DeviceStorageSnapshot, ManagedDeviceFile, MirrorBlocker, MirrorInput,
    MirrorPlan, MirrorPlaylistSnapshot, MirrorTrack, MirrorWarning, SelectionSource,
    StorageProjectionState, TransferProfile,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncPlaylistRow {
    pub source: SelectionSource,
    pub name: Option<String>,
    pub smart: bool,
    pub selected: bool,
    pub available: bool,
    pub entry_count: usize,
    pub unique_track_count: usize,
    pub unavailable_count: usize,
    pub target_bytes: u64,
    pub last_synced_at: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncChangeSummary {
    pub additions: usize,
    pub replacements: usize,
    pub removals: usize,
    pub retained_unavailable: usize,
    pub playlist_writes: usize,
    pub playlist_removals: usize,
    pub transfer_bytes: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SyncPageControls {
    pub editable: bool,
    pub can_start: bool,
    pub can_cancel: bool,
    pub can_eject: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncPageWarning {
    UnavailableNotOnDevice { track_id: i64 },
    UnsafeManagedItem,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncPageState {
    pub profile_options: Vec<TransferProfile>,
    pub profile: TransferProfile,
    pub playlists: Vec<SyncPlaylistRow>,
    pub unique_track_count: usize,
    pub target_bytes: u64,
    pub changes: SyncChangeSummary,
    pub storage: DeviceStorageProjection,
    pub blockers: Vec<MirrorBlocker>,
    pub warnings: Vec<SyncPageWarning>,
    pub controls: SyncPageControls,
}

impl SyncPageState {
    pub fn update_controls(&mut self, connected: bool, ready: bool, active: bool) {
        let storage_blocks = matches!(
            self.storage.state,
            StorageProjectionState::Blocked | StorageProjectionState::Insufficient { .. }
        ) || self.storage.access == DeviceStorageAccess::ReadOnly;
        let transfer_blocks = self
            .storage
            .current
            .free_bytes
            .is_some_and(|free| self.changes.transfer_bytes > free);
        self.controls = SyncPageControls {
            editable: connected && !active,
            can_start: connected
                && ready
                && !active
                && self.blockers.is_empty()
                && !storage_blocks
                && !transfer_blocks,
            can_cancel: active,
            can_eject: connected && !active,
        };
    }
}

impl Default for SyncPageState {
    fn default() -> Self {
        project_sync_page(SyncPageInput::default()).page
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncPageInput {
    pub selected: Vec<SelectionSource>,
    pub playlists: Vec<MirrorPlaylistSnapshot>,
    pub profile: TransferProfile,
    pub inventory: Vec<DeviceFileRecord>,
    pub playlist_inventory: Vec<DevicePlaylistRecord>,
    pub managed_files: Vec<ManagedDeviceFile>,
    pub storage: DeviceStorageSnapshot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncPageProjection {
    pub page: SyncPageState,
    pub plan: MirrorPlan,
}

pub fn project_sync_page(input: SyncPageInput) -> SyncPageProjection {
    let selected = input.selected.iter().cloned().collect::<HashSet<_>>();
    let inventory_by_id = input
        .inventory
        .iter()
        .map(|file| (file.track_id, file))
        .collect::<HashMap<_, _>>();
    let playlist_inventory_by_source = input
        .playlist_inventory
        .iter()
        .map(|playlist| (playlist.source.clone(), playlist))
        .collect::<HashMap<_, _>>();
    let mut rows = input
        .playlists
        .iter()
        .map(|playlist| {
            playlist_row(
                playlist,
                selected.contains(&playlist.source),
                input.profile,
                &inventory_by_id,
                playlist_inventory_by_source
                    .get(&playlist.source)
                    .and_then(|record| record.last_synced_at),
            )
        })
        .collect::<Vec<_>>();
    let available_sources = input
        .playlists
        .iter()
        .map(|playlist| playlist.source.clone())
        .collect::<HashSet<_>>();
    let previous_names = input
        .playlist_inventory
        .iter()
        .map(|playlist| (playlist.source.clone(), playlist.source_name.clone()))
        .collect::<HashMap<_, _>>();
    let mut missing_sources = HashSet::new();
    for source in &input.selected {
        if available_sources.contains(source) || !missing_sources.insert(source.clone()) {
            continue;
        }
        rows.push(SyncPlaylistRow {
            source: source.clone(),
            name: previous_names.get(source).cloned(),
            smart: matches!(source, SelectionSource::Smart(_)),
            selected: true,
            available: false,
            entry_count: 0,
            unique_track_count: 0,
            unavailable_count: 0,
            target_bytes: 0,
            last_synced_at: playlist_inventory_by_source
                .get(source)
                .and_then(|record| record.last_synced_at),
        });
    }
    rows.sort_by(|left, right| {
        left.name
            .as_deref()
            .unwrap_or_default()
            .to_lowercase()
            .cmp(&right.name.as_deref().unwrap_or_default().to_lowercase())
            .then_with(|| left.source.cmp(&right.source))
    });

    let unique_track_count = input
        .playlists
        .iter()
        .filter(|playlist| selected.contains(&playlist.source))
        .flat_map(|playlist| playlist.entries.iter().map(mirror_track_id))
        .collect::<HashSet<_>>()
        .len();
    let plan = plan_mirror(MirrorInput {
        selected: input.selected,
        playlists: input.playlists,
        profile: input.profile,
        inventory: input.inventory,
        playlist_inventory: input.playlist_inventory,
        managed_files: input.managed_files,
    });
    let storage = project_storage(&input.storage, &plan);
    let page = SyncPageState {
        profile_options: TransferProfile::ALL.to_vec(),
        profile: input.profile,
        playlists: rows,
        unique_track_count,
        target_bytes: plan.target_bytes,
        changes: SyncChangeSummary {
            additions: plan.copy.len(),
            replacements: plan.replace.len(),
            removals: plan.remove.len(),
            retained_unavailable: plan.retained_unavailable.len(),
            playlist_writes: plan.playlist_writes.len(),
            playlist_removals: plan.playlist_removals.len(),
            transfer_bytes: plan.transfer_bytes,
        },
        storage,
        blockers: plan.blockers.clone(),
        warnings: plan
            .warnings
            .iter()
            .map(|warning| match warning {
                MirrorWarning::UnavailableNotOnDevice { track_id } => {
                    SyncPageWarning::UnavailableNotOnDevice {
                        track_id: *track_id,
                    }
                }
                MirrorWarning::UnsafeManagedPath { .. } => SyncPageWarning::UnsafeManagedItem,
            })
            .collect(),
        controls: SyncPageControls::default(),
    };
    SyncPageProjection { page, plan }
}

fn playlist_row(
    playlist: &MirrorPlaylistSnapshot,
    selected: bool,
    profile: TransferProfile,
    inventory: &HashMap<i64, &DeviceFileRecord>,
    last_synced_at: Option<i64>,
) -> SyncPlaylistRow {
    let mut unique = HashSet::new();
    let mut target_bytes = 0_u64;
    let mut unavailable_count = 0;
    for entry in &playlist.entries {
        let track_id = mirror_track_id(entry);
        if matches!(entry, MirrorTrack::Unavailable(_)) {
            unavailable_count += 1;
        }
        if !unique.insert(track_id) {
            continue;
        }
        target_bytes = target_bytes.saturating_add(match entry {
            MirrorTrack::Available(track) => profile.estimated_target_bytes(track),
            MirrorTrack::Unavailable(_) => {
                inventory.get(&track_id).map_or(0, |file| file.device_size)
            }
        });
    }
    SyncPlaylistRow {
        source: playlist.source.clone(),
        name: Some(playlist.name.clone()),
        smart: matches!(playlist.source, SelectionSource::Smart(_)),
        selected,
        available: true,
        entry_count: playlist.entries.len(),
        unique_track_count: unique.len(),
        unavailable_count,
        target_bytes,
        last_synced_at,
    }
}

fn mirror_track_id(track: &MirrorTrack) -> i64 {
    match track {
        MirrorTrack::Available(track) => track.id,
        MirrorTrack::Unavailable(track) => track.track_id,
    }
}
