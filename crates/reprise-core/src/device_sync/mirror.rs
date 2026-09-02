//! Pure, side-effect-free planning for safe device playlist mirroring.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path};

use super::m3u::{render_named_playlist, DevicePlaylistEntry};
use super::sanitize::sanitize_component;
use super::settings::{DeviceFileRecord, DevicePlaylistRecord, SelectionSource};
use super::transfer::build_transfer_plan_with_inventory;
use super::{SyncTrack, TransferAction, TransferProfile};

#[path = "mirror_file_changes.rs"]
mod file_changes;
#[path = "mirror_lyrics.rs"]
mod lyrics;

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
    /// Ranked smart-playlist members just below its addition cap.
    pub stability_margin_track_ids: Vec<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedDeviceFile {
    /// Path relative to the platform backend's `Music/Reprise` root.
    pub relative_path: String,
    pub size_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DesktopAnalysis {
    pub track_id: i64,
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
    pub partial_paths: Vec<String>,
    pub lyrics_files: Vec<ManagedDeviceFile>,
    pub managed_files_scanned: bool,
    pub desktop_analyses: Vec<DesktopAnalysis>,
}

struct ManagedTreeInventory {
    managed_files: Vec<ManagedDeviceFile>,
    partial_paths: Vec<String>,
    lyrics_files: Vec<ManagedDeviceFile>,
    scanned: bool,
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
pub struct AnalysisSidecarWrite {
    pub track_id: i64,
    pub device_path: String,
    pub size_bytes: u64,
    pub existing_size_bytes: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LyricsSidecarWrite {
    pub track_id: i64,
    pub source_path: std::path::PathBuf,
    pub device_path: String,
    pub size_bytes: u64,
    pub existing_size_bytes: Option<u64>,
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
    pub analysis_writes: Vec<AnalysisSidecarWrite>,
    pub lyrics_writes: Vec<LyricsSidecarWrite>,
    pub partial_paths: Vec<String>,
    pub remove: Vec<ManagedRemoval>,
    pub retained_unavailable: Vec<DeviceFileRecord>,
    pub retained_stable: Vec<DeviceFileRecord>,
    pub playlist_writes: Vec<PlaylistWrite>,
    pub playlist_removals: Vec<DevicePlaylistRecord>,
    pub transfer_bytes: u64,
    pub target_bytes: u64,
    /// Total size of everything in [`Self::remove`] — kept as its own sum
    /// rather than derived at read time, and deliberately never merged
    /// into [`Self::transfer_bytes`] (which only ever counts bytes moving
    /// *onto* the device). A deletions-only plan has `transfer_bytes == 0`
    /// and `bytes_freed > 0`; keeping the two separate is what lets a
    /// truthful "0 B to copy · frees 148 MiB" reading exist instead of a
    /// single blended "bytes" figure that reads as "nothing to do" (`MTP-22`,
    /// design 7c).
    pub bytes_freed: u64,
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
        ManagedTreeInventory {
            managed_files: input.managed_files,
            partial_paths: input.partial_paths,
            lyrics_files: input.lyrics_files,
            scanned: input.managed_files_scanned,
        },
        &input.desktop_analyses,
    )
}

fn build_plan(
    playlists: &[MirrorPlaylistSnapshot],
    profile: TransferProfile,
    mut inventory: Vec<DeviceFileRecord>,
    mut playlist_inventory: Vec<DevicePlaylistRecord>,
    managed_tree: ManagedTreeInventory,
    desktop_analyses: &[DesktopAnalysis],
) -> MirrorPlan {
    let ManagedTreeInventory {
        mut managed_files,
        mut partial_paths,
        mut lyrics_files,
        scanned: managed_files_scanned,
    } = managed_tree;
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
    partial_paths.sort();
    lyrics_files.sort_by(|left, right| {
        left.relative_path
            .cmp(&right.relative_path)
            .then_with(|| left.size_bytes.cmp(&right.size_bytes))
    });

    let mut available = HashMap::<i64, SyncTrack>::new();
    let mut unavailable = HashMap::<i64, UnavailableTrack>::new();
    let stability_margin_ids = playlists
        .iter()
        .flat_map(|playlist| playlist.stability_margin_track_ids.iter().copied())
        .collect::<HashSet<_>>();
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
    let inventory_by_id = inventory
        .iter()
        .cloned()
        .map(|file| (file.track_id, file))
        .collect::<HashMap<_, _>>();
    let mut desired_by_id = desired_files
        .drain(..)
        .map(|file| (file.track.id, file))
        .collect::<HashMap<_, _>>();
    let unplanned_resident_paths = super::device_case::rewrite_desired_paths(
        &mut desired_by_id,
        &inventory,
        &inventory_by_id,
        &managed_files,
    );
    desired_files = desired_by_id.values().cloned().collect();
    desired_files.sort_by_key(|file| file.track.id);
    let managed_paths = managed_files
        .iter()
        .map(|file| file.relative_path.to_lowercase())
        .collect::<HashSet<_>>();

    let mut plan = MirrorPlan {
        target_bytes: desired_files
            .iter()
            .fold(0_u64, |sum, file| sum.saturating_add(file.target_bytes)),
        desired_files,
        partial_paths,
        ..MirrorPlan::default()
    };
    file_changes::plan_file_changes(
        file_changes::FileChangeInput {
            desired: &desired_by_id,
            inventory: &inventory,
            inventory_by_id: &inventory_by_id,
            unavailable: &unavailable,
            stability_margin_ids: &stability_margin_ids,
            managed_files_scanned,
            managed_paths: &managed_paths,
        },
        &mut plan,
    );
    plan_analysis_sidecars(desktop_analyses, &managed_files, &mut plan);
    lyrics::plan_lyrics_sidecars(&lyrics_files, &managed_files, &mut plan);
    plan_playlists(
        playlists,
        &desired_by_id,
        &inventory_by_id,
        &playlist_inventory,
        &mut plan,
    );
    let owned_analysis_sidecars = owned_analysis_sidecar_paths(&plan, &managed_files);
    let known_paths = inventory
        .iter()
        .map(|file| file.device_path.clone())
        .chain(
            playlist_inventory
                .iter()
                .map(|playlist| playlist.device_path.clone()),
        )
        .chain(
            plan.desired_files
                .iter()
                .map(|file| file.device_path.clone()),
        )
        .chain(
            plan.playlist_writes
                .iter()
                .map(|playlist| playlist.device_path.clone()),
        )
        .chain(unplanned_resident_paths.iter().cloned())
        .chain(
            unplanned_resident_paths
                .iter()
                .filter_map(|path| super::analysis_sidecar::device_path_for_track(path)),
        )
        .chain(owned_analysis_sidecars)
        .collect::<HashSet<_>>();
    plan_orphan_removals(&known_paths, &managed_files, &mut plan);
    plan
}

/// The device paths whose audio this run puts on the device.
///
/// Two decisions turn on it — which sidecars this plan owns, and which
/// sidecars it writes — and computing it twice is how those two drift apart.
/// It takes the two fields rather than the whole plan so a caller holding
/// `&mut MirrorPlan` can still write to the other fields.
fn arriving_audio_paths<'a>(
    copy: &'a [DesiredManagedFile],
    replace: &'a [MirrorReplacement],
) -> HashSet<&'a str> {
    copy.iter()
        .map(|file| file.device_path.as_str())
        .chain(
            replace
                .iter()
                .map(|replacement| replacement.desired.device_path.as_str()),
        )
        .collect()
}

fn owned_analysis_sidecar_paths(
    plan: &MirrorPlan,
    managed_files: &[ManagedDeviceFile],
) -> HashSet<String> {
    let resident = managed_files
        .iter()
        .map(|file| file.relative_path.as_str())
        .collect::<HashSet<_>>();
    let arriving = arriving_audio_paths(&plan.copy, &plan.replace);
    plan.desired_files
        .iter()
        .map(|file| file.device_path.as_str())
        .filter(|path| resident.contains(path) || arriving.contains(path))
        .chain(
            plan.retained_unavailable
                .iter()
                .map(|file| file.device_path.as_str())
                .filter(|path| resident.contains(path)),
        )
        .chain(
            plan.retained_stable
                .iter()
                .map(|file| file.device_path.as_str())
                .filter(|path| resident.contains(path)),
        )
        .filter_map(super::analysis_sidecar::device_path_for_track)
        .collect()
}

fn plan_analysis_sidecars(
    desktop_analyses: &[DesktopAnalysis],
    managed_files: &[ManagedDeviceFile],
    plan: &mut MirrorPlan,
) {
    let analyses = desktop_analyses
        .iter()
        .map(|analysis| (analysis.track_id, analysis.size_bytes))
        .collect::<HashMap<_, _>>();
    let resident = managed_files
        .iter()
        .map(|file| (file.relative_path.as_str(), file.size_bytes))
        .collect::<HashMap<_, _>>();
    let arriving_audio = arriving_audio_paths(&plan.copy, &plan.replace);
    let mut analysis_target_bytes = 0_u64;
    for desired in &plan.desired_files {
        if !resident.contains_key(desired.device_path.as_str())
            && !arriving_audio.contains(desired.device_path.as_str())
        {
            continue;
        }
        let Some(size_bytes) = analyses.get(&desired.track.id).copied() else {
            continue;
        };
        analysis_target_bytes = analysis_target_bytes.saturating_add(size_bytes);
        let Some(device_path) =
            super::analysis_sidecar::device_path_for_track(&desired.device_path)
        else {
            continue;
        };
        let existing_size_bytes = resident.get(device_path.as_str()).copied();
        // Size is deliberately a coarse change signal. It catches a sidecar
        // whose encoded shape changed without reading every file back over
        // MTP, but cannot distinguish recomputed analyses of the same length.
        if existing_size_bytes == Some(size_bytes) {
            continue;
        }
        plan.analysis_writes.push(AnalysisSidecarWrite {
            track_id: desired.track.id,
            device_path,
            size_bytes,
            existing_size_bytes,
        });
    }
    let analysis_bytes = plan
        .analysis_writes
        .iter()
        .map(|sidecar| sidecar.size_bytes)
        .fold(0_u64, u64::saturating_add);
    plan.transfer_bytes = plan.transfer_bytes.saturating_add(analysis_bytes);
    plan.target_bytes = plan.target_bytes.saturating_add(analysis_target_bytes);
}

fn plan_orphan_removals(
    known_paths: &HashSet<String>,
    managed_files: &[ManagedDeviceFile],
    plan: &mut MirrorPlan,
) {
    let mut seen_physical = HashSet::new();
    let known_folded_paths = known_paths
        .iter()
        .map(|path| super::device_case::fold_path(path))
        .collect::<HashSet<_>>();
    for file in managed_files {
        // A folded match may retain a genuine case-variant duplicate, but an
        // MTP phantom delete can abort every later sync. Prefer leaked space
        // over risking that destructive failure.
        if !seen_physical.insert(file.relative_path.as_str())
            || known_paths.contains(&file.relative_path)
            || known_folded_paths.contains(&super::device_case::fold_path(&file.relative_path))
        {
            continue;
        }
        if safe_managed_path(&file.relative_path) {
            if !is_removable_managed_path(&file.relative_path) {
                continue;
            }
            plan.bytes_freed = plan.bytes_freed.saturating_add(file.size_bytes);
            plan.remove.push(ManagedRemoval::Orphan(file.clone()));
        } else {
            push_warning(
                &mut plan.warnings,
                MirrorWarning::UnsafeManagedPath {
                    path: file.relative_path.clone(),
                },
            );
        }
    }
}

fn is_removable_managed_path(path: &str) -> bool {
    let path = Path::new(path);
    // The report happens to be imported before removals today, but ordering is
    // not a safety property: both halves of the return channel are managed
    // files the desktop depends on and therefore are not litter.
    path != Path::new(super::listen_report::REPORT_FILE_NAME)
        && path != Path::new(super::listen_report::ACKNOWLEDGEMENT_FILE_NAME)
        && !super::track_metadata_list::is_list_path(path)
        && !super::lyrics_sidecar::is_sidecar_path(path)
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
