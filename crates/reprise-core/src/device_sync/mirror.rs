//! Pure, side-effect-free planning for safe device playlist mirroring.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path};

use super::m3u::{render_named_playlist, DevicePlaylistEntry};
use super::sanitize::sanitize_component;
use super::settings::{DeviceFileRecord, DevicePlaylistRecord, SelectionSource};
use super::transfer::build_transfer_plan_with_inventory;
use super::{SyncTrack, TransferAction, TransferProfile};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnavailableTrack {
    pub track_id: i64,
    pub title: String,
    pub artist: String,
    pub duration_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MirrorTrack {
    Available(SyncTrack),
    Unavailable(UnavailableTrack),
}

impl MirrorTrack {
    fn track_id(&self) -> i64 {
        match self {
            Self::Available(track) => track.id,
            Self::Unavailable(track) => track.track_id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MirrorPlaylistSnapshot {
    pub source: SelectionSource,
    pub name: String,
    pub entries: Vec<MirrorTrack>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedDeviceFile {
    /// Path relative to the platform backend's `Music/Reprise` root.
    pub relative_path: String,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MirrorInput {
    pub selected: Vec<SelectionSource>,
    pub playlists: Vec<MirrorPlaylistSnapshot>,
    pub profile: TransferProfile,
    pub inventory: Vec<DeviceFileRecord>,
    pub playlist_inventory: Vec<DevicePlaylistRecord>,
    pub managed_files: Vec<ManagedDeviceFile>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesiredManagedFile {
    pub track: SyncTrack,
    pub device_path: String,
    pub target_bytes: u64,
    pub profile_fingerprint: String,
    pub action: TransferAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MirrorReplacement {
    pub existing: DeviceFileRecord,
    pub desired: DesiredManagedFile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManagedRemoval {
    Inventory(DeviceFileRecord),
    Orphan(ManagedDeviceFile),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MirrorPlaylistProjection {
    pub source: SelectionSource,
    pub name: String,
    pub entry_count: usize,
    pub unique_track_count: usize,
    pub unavailable_count: usize,
    pub target_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaylistWrite {
    pub source: SelectionSource,
    pub source_name: String,
    pub device_path: String,
    pub entries: Vec<DevicePlaylistEntry>,
    pub contents: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MirrorBlocker {
    NoPlaylistsSelected,
    MissingPlaylist(SelectionSource),
    DuplicatePlaylist(SelectionSource),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MirrorWarning {
    UnavailableNotOnDevice { track_id: i64 },
    UnsafeManagedPath { path: String },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MirrorPlan {
    pub per_playlist: Vec<MirrorPlaylistProjection>,
    pub desired_files: Vec<DesiredManagedFile>,
    pub copy: Vec<DesiredManagedFile>,
    pub replace: Vec<MirrorReplacement>,
    pub remove: Vec<ManagedRemoval>,
    pub retained_unavailable: Vec<DeviceFileRecord>,
    pub playlist_writes: Vec<PlaylistWrite>,
    pub playlist_removals: Vec<DevicePlaylistRecord>,
    pub transfer_bytes: u64,
    pub target_bytes: u64,
    pub blockers: Vec<MirrorBlocker>,
    pub warnings: Vec<MirrorWarning>,
}

pub fn plan_mirror(input: MirrorInput) -> MirrorPlan {
    let selected = deduplicate_sources(&input.selected);
    let mut blockers = Vec::new();
    if selected.is_empty() {
        blockers.push(MirrorBlocker::NoPlaylistsSelected);
    }

    let selected_set = selected.iter().cloned().collect::<HashSet<_>>();
    let mut snapshots = HashMap::new();
    for snapshot in input
        .playlists
        .into_iter()
        .filter(|snapshot| selected_set.contains(&snapshot.source))
    {
        let source = snapshot.source.clone();
        if snapshots.insert(source.clone(), snapshot).is_some() {
            blockers.push(MirrorBlocker::DuplicatePlaylist(source));
        }
    }
    for source in &selected {
        if !snapshots.contains_key(source) {
            blockers.push(MirrorBlocker::MissingPlaylist(source.clone()));
        }
    }
    if !blockers.is_empty() {
        return MirrorPlan {
            blockers,
            ..MirrorPlan::default()
        };
    }

    let ordered_snapshots = selected
        .iter()
        .filter_map(|source| snapshots.remove(source))
        .collect::<Vec<_>>();
    build_plan(
        &ordered_snapshots,
        input.profile,
        input.inventory,
        input.playlist_inventory,
        input.managed_files,
    )
}

fn build_plan(
    playlists: &[MirrorPlaylistSnapshot],
    profile: TransferProfile,
    mut inventory: Vec<DeviceFileRecord>,
    mut playlist_inventory: Vec<DevicePlaylistRecord>,
    mut managed_files: Vec<ManagedDeviceFile>,
) -> MirrorPlan {
    inventory.sort_by(|left, right| {
        left.track_id
            .cmp(&right.track_id)
            .then_with(|| left.device_path.cmp(&right.device_path))
    });
    playlist_inventory.sort_by(|left, right| {
        left.source
            .cmp(&right.source)
            .then_with(|| left.device_path.cmp(&right.device_path))
    });
    managed_files.sort_by(|left, right| {
        left.relative_path
            .cmp(&right.relative_path)
            .then_with(|| left.size_bytes.cmp(&right.size_bytes))
    });

    let mut available = HashMap::<i64, SyncTrack>::new();
    let mut unavailable = HashMap::<i64, UnavailableTrack>::new();
    for entry in playlists.iter().flat_map(|playlist| &playlist.entries) {
        match entry {
            MirrorTrack::Available(track) => {
                available.entry(track.id).or_insert_with(|| track.clone());
            }
            MirrorTrack::Unavailable(track) => {
                unavailable
                    .entry(track.track_id)
                    .or_insert_with(|| track.clone());
            }
        }
    }

    let mut available_tracks = available.values().cloned().collect::<Vec<_>>();
    available_tracks.sort_by_key(|track| track.id);
    let mut desired_files =
        build_transfer_plan_with_inventory(available_tracks, profile, &inventory)
            .into_iter()
            .map(|entry| {
                let action = profile.action_for(&entry.track);
                DesiredManagedFile {
                    track: entry.track,
                    device_path: entry.device_path,
                    target_bytes: entry.expected_bytes,
                    profile_fingerprint: entry.mode.fingerprint(),
                    action,
                }
            })
            .collect::<Vec<_>>();
    desired_files.sort_by_key(|file| file.track.id);
    let desired_by_id = desired_files
        .iter()
        .cloned()
        .map(|file| (file.track.id, file))
        .collect::<HashMap<_, _>>();
    let inventory_by_id = inventory
        .iter()
        .cloned()
        .map(|file| (file.track_id, file))
        .collect::<HashMap<_, _>>();

    let mut plan = MirrorPlan {
        target_bytes: desired_files
            .iter()
            .fold(0_u64, |sum, file| sum.saturating_add(file.target_bytes)),
        desired_files,
        ..MirrorPlan::default()
    };
    plan_file_changes(
        &desired_by_id,
        &inventory,
        &inventory_by_id,
        &unavailable,
        &managed_files,
        &mut plan,
    );
    plan_playlists(
        playlists,
        &desired_by_id,
        &inventory_by_id,
        &playlist_inventory,
        &mut plan,
    );
    plan
}

fn plan_file_changes(
    desired: &HashMap<i64, DesiredManagedFile>,
    inventory: &[DeviceFileRecord],
    inventory_by_id: &HashMap<i64, DeviceFileRecord>,
    unavailable: &HashMap<i64, UnavailableTrack>,
    managed_files: &[ManagedDeviceFile],
    plan: &mut MirrorPlan,
) {
    let mut desired_ids = desired.keys().copied().collect::<Vec<_>>();
    desired_ids.sort_unstable();
    for track_id in desired_ids {
        let file = &desired[&track_id];
        match inventory_by_id.get(&track_id) {
            None => plan.copy.push(file.clone()),
            Some(existing) if inventory_matches(existing, file) => {}
            Some(existing) if safe_managed_path(&existing.device_path) => {
                plan.replace.push(MirrorReplacement {
                    existing: existing.clone(),
                    desired: file.clone(),
                });
            }
            Some(existing) => {
                push_warning(
                    &mut plan.warnings,
                    MirrorWarning::UnsafeManagedPath {
                        path: existing.device_path.clone(),
                    },
                );
                plan.copy.push(file.clone());
            }
        }
    }

    let mut unavailable_ids = unavailable
        .keys()
        .copied()
        .filter(|track_id| !desired.contains_key(track_id))
        .collect::<Vec<_>>();
    unavailable_ids.sort_unstable();
    let mut retained_ids = HashSet::new();
    for track_id in unavailable_ids {
        if let Some(existing) = inventory_by_id.get(&track_id) {
            retained_ids.insert(track_id);
            plan.target_bytes = plan.target_bytes.saturating_add(existing.device_size);
            plan.retained_unavailable.push(existing.clone());
        } else {
            push_warning(
                &mut plan.warnings,
                MirrorWarning::UnavailableNotOnDevice { track_id },
            );
        }
    }

    for existing in inventory {
        if desired.contains_key(&existing.track_id) || retained_ids.contains(&existing.track_id) {
            continue;
        }
        if safe_managed_path(&existing.device_path) {
            plan.remove
                .push(ManagedRemoval::Inventory(existing.clone()));
        } else {
            push_warning(
                &mut plan.warnings,
                MirrorWarning::UnsafeManagedPath {
                    path: existing.device_path.clone(),
                },
            );
        }
    }

    let known_paths = inventory
        .iter()
        .map(|file| file.device_path.as_str())
        .chain(desired.values().map(|file| file.device_path.as_str()))
        .collect::<HashSet<_>>();
    let mut seen_physical = HashSet::new();
    for file in managed_files {
        if !seen_physical.insert(file.relative_path.as_str())
            || known_paths.contains(file.relative_path.as_str())
        {
            continue;
        }
        push_warning(
            &mut plan.warnings,
            MirrorWarning::UnsafeManagedPath {
                path: file.relative_path.clone(),
            },
        );
    }

    plan.transfer_bytes = plan
        .copy
        .iter()
        .map(|file| file.target_bytes)
        .chain(
            plan.replace
                .iter()
                .map(|replacement| replacement.desired.target_bytes),
        )
        .fold(0_u64, u64::saturating_add);
}

fn plan_playlists(
    playlists: &[MirrorPlaylistSnapshot],
    desired: &HashMap<i64, DesiredManagedFile>,
    inventory_by_id: &HashMap<i64, DeviceFileRecord>,
    playlist_inventory: &[DevicePlaylistRecord],
    plan: &mut MirrorPlan,
) {
    let paths = stable_playlist_paths(playlists, playlist_inventory);
    for playlist in playlists {
        let mut entries = Vec::new();
        let mut unique = HashSet::new();
        let mut target_bytes = 0_u64;
        let mut unavailable_count = 0;
        for entry in &playlist.entries {
            let track_id = entry.track_id();
            if unique.insert(track_id) {
                target_bytes = target_bytes.saturating_add(desired.get(&track_id).map_or_else(
                    || {
                        inventory_by_id
                            .get(&track_id)
                            .map_or(0, |file| file.device_size)
                    },
                    |file| file.target_bytes,
                ));
            }
            if matches!(entry, MirrorTrack::Unavailable(_)) {
                unavailable_count += 1;
            }
            if let Some(rendered) = playlist_entry(entry, desired, inventory_by_id) {
                entries.push(rendered);
            }
        }
        let device_path = paths
            .get(&playlist.source)
            .cloned()
            .unwrap_or_else(|| playlist_path(&playlist.name, 1));
        plan.per_playlist.push(MirrorPlaylistProjection {
            source: playlist.source.clone(),
            name: playlist.name.clone(),
            entry_count: playlist.entries.len(),
            unique_track_count: unique.len(),
            unavailable_count,
            target_bytes,
        });
        plan.playlist_writes.push(PlaylistWrite {
            source: playlist.source.clone(),
            source_name: playlist.name.clone(),
            device_path,
            contents: render_named_playlist(&entries),
            entries,
        });
    }

    let desired_paths = plan
        .playlist_writes
        .iter()
        .map(|write| (&write.source, write.device_path.as_str()))
        .collect::<HashMap<_, _>>();
    for existing in playlist_inventory {
        let keep = desired_paths
            .get(&existing.source)
            .is_some_and(|path| *path == existing.device_path);
        if keep {
            continue;
        }
        if safe_managed_path(&existing.device_path) {
            plan.playlist_removals.push(existing.clone());
        } else {
            push_warning(
                &mut plan.warnings,
                MirrorWarning::UnsafeManagedPath {
                    path: existing.device_path.clone(),
                },
            );
        }
    }
}

fn playlist_entry(
    entry: &MirrorTrack,
    desired: &HashMap<i64, DesiredManagedFile>,
    inventory: &HashMap<i64, DeviceFileRecord>,
) -> Option<DevicePlaylistEntry> {
    let (track_id, title, artist, duration_ms) = match entry {
        MirrorTrack::Available(track) => (
            track.id,
            track.title.as_str(),
            track.artist.as_str(),
            track.duration_ms,
        ),
        MirrorTrack::Unavailable(track) => (
            track.track_id,
            track.title.as_str(),
            track.artist.as_str(),
            track.duration_ms,
        ),
    };
    let relative_path = desired
        .get(&track_id)
        .map(|file| file.device_path.clone())
        .or_else(|| {
            inventory
                .get(&track_id)
                .map(|file| file.device_path.clone())
        })?;
    Some(DevicePlaylistEntry {
        relative_path,
        duration_secs: duration_ms.max(0) / 1_000,
        display: if artist.trim().is_empty() {
            title.to_string()
        } else {
            format!("{artist} - {title}")
        },
    })
}

fn inventory_matches(existing: &DeviceFileRecord, desired: &DesiredManagedFile) -> bool {
    existing.source_path == desired.track.source_path.to_string_lossy()
        && existing.source_size == desired.track.size_bytes
        && existing.source_mtime == desired.track.source_mtime
        && existing.device_path == desired.device_path
        && existing.profile_fingerprint == desired.profile_fingerprint
}

fn deduplicate_sources(sources: &[SelectionSource]) -> Vec<SelectionSource> {
    let mut seen = HashSet::new();
    sources
        .iter()
        .filter(|source| seen.insert((*source).clone()))
        .cloned()
        .collect()
}

fn safe_managed_path(path: &str) -> bool {
    !path.is_empty()
        && !path.chars().any(char::is_control)
        && !Path::new(path).is_absolute()
        && Path::new(path)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn push_warning(warnings: &mut Vec<MirrorWarning>, warning: MirrorWarning) {
    if !warnings.contains(&warning) {
        warnings.push(warning);
    }
}

fn stable_playlist_paths(
    playlists: &[MirrorPlaylistSnapshot],
    inventory: &[DevicePlaylistRecord],
) -> HashMap<SelectionSource, String> {
    let mut playlists = playlists.iter().collect::<Vec<_>>();
    playlists.sort_by(|left, right| left.source.cmp(&right.source));
    let mut slots = HashMap::<String, PlaylistCollisionSlots>::new();
    let mut paths = HashMap::new();
    for playlist in playlists {
        let base = sanitize_component(&playlist.name, "Playlist");
        let key = base.to_lowercase();
        let collision = slots
            .entry(key.clone())
            .or_insert_with(|| PlaylistCollisionSlots::from_inventory(&key, inventory));
        let (index, existing_path) = collision.assign(&playlist.source);
        paths.insert(
            playlist.source.clone(),
            existing_path.unwrap_or_else(|| playlist_path(&base, index)),
        );
    }
    paths
}

#[derive(Default)]
struct PlaylistCollisionSlots {
    used: HashSet<usize>,
    owned: HashMap<SelectionSource, (usize, String)>,
}

impl PlaylistCollisionSlots {
    fn from_inventory(base_key: &str, inventory: &[DevicePlaylistRecord]) -> Self {
        let mut slots = Self::default();
        for record in inventory {
            let Some(index) = playlist_collision_index(&record.device_path, base_key) else {
                continue;
            };
            if slots.used.insert(index) {
                slots
                    .owned
                    .insert(record.source.clone(), (index, record.device_path.clone()));
            }
        }
        slots
    }

    fn assign(&mut self, source: &SelectionSource) -> (usize, Option<String>) {
        if let Some((index, path)) = self.owned.get(source) {
            return (*index, Some(path.clone()));
        }
        let mut index = 1;
        while !self.used.insert(index) {
            index = index.saturating_add(1);
        }
        (index, None)
    }
}

fn playlist_collision_index(path: &str, base_key: &str) -> Option<usize> {
    if !safe_managed_path(path) {
        return None;
    }
    let stem = path.strip_suffix(".m3u8")?.to_lowercase();
    if stem.contains('/') {
        return None;
    }
    if stem == base_key {
        return Some(1);
    }
    let suffix = stem.strip_prefix(base_key)?;
    let index = suffix.strip_prefix(" (")?.strip_suffix(')')?.parse().ok()?;
    (index >= 2).then_some(index)
}

fn playlist_path(base: &str, collision_index: usize) -> String {
    let suffix = if collision_index > 1 {
        format!(" ({collision_index})")
    } else {
        String::new()
    };
    format!("{base}{suffix}.m3u8")
}
