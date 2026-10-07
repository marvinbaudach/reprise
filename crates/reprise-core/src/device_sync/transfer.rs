//! Transfer decisions and device destinations independent of platform I/O.

use super::cue_files::CueSyncFile;
use super::sanitize::{
    device_file_path, device_track_path, sanitize_component, DevicePathMetadata,
};
use super::{Mp3Quality, SyncTrack, TransferAction, TransferProfile};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferMode {
    Copy,
    TranscodeOpus160,
    TranscodeMp3 { quality: Mp3Quality },
}

impl TransferMode {
    pub fn fingerprint(self) -> String {
        match self {
            Self::Copy => "copy-original-v1".into(),
            Self::TranscodeOpus160 => TransferProfile::Opus160.fingerprint().into(),
            Self::TranscodeMp3 { quality } => TransferProfile::Mp3(quality).fingerprint().into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferPlanEntry {
    pub track: SyncTrack,
    pub device_path: String,
    pub expected_bytes: u64,
    pub mode: TransferMode,
}

pub fn build_transfer_plan(
    tracks: Vec<SyncTrack>,
    profile: TransferProfile,
) -> Vec<TransferPlanEntry> {
    build_transfer_plan_with_inventory(tracks, profile, &[])
}

pub fn build_transfer_plan_with_inventory(
    tracks: Vec<SyncTrack>,
    profile: TransferProfile,
    inventory: &[super::settings::DeviceFileRecord],
) -> Vec<TransferPlanEntry> {
    build_transfer_plan_with_files(tracks, profile, inventory, &HashMap::new())
}

/// Plans every track's device destination. A track of a CUE file in
/// `cue_files` (by track id) shares one destination with every other track of
/// that file: named after the source file, with one collision slot, and the
/// file's bytes estimated once, on the first of its tracks (CUE-15).
pub(super) fn build_transfer_plan_with_files(
    tracks: Vec<SyncTrack>,
    profile: TransferProfile,
    inventory: &[super::settings::DeviceFileRecord],
    cue_files: &HashMap<i64, &CueSyncFile>,
) -> Vec<TransferPlanEntry> {
    let mut collisions = HashMap::<String, CollisionSlots>::new();
    let mut estimated_files = HashSet::<std::path::PathBuf>::new();
    let mut indexed = tracks.into_iter().enumerate().collect::<Vec<_>>();
    indexed.sort_by_key(|(_, track)| track.id);
    let mut plan = indexed
        .into_iter()
        .map(|(index, track)| {
            let mode = match profile.action_for(&track) {
                TransferAction::CopyOriginal => TransferMode::Copy,
                TransferAction::TranscodeOpus160 => TransferMode::TranscodeOpus160,
                TransferAction::TranscodeMp3(quality) => TransferMode::TranscodeMp3 { quality },
            };
            let cue_file = cue_files.get(&track.id).copied();
            let metadata = match cue_file {
                Some(file) => file_path_metadata(file),
                None => DevicePathMetadata {
                    album_artist: track.album_artist.clone(),
                    artist: track.artist.clone(),
                    album: track.album.clone(),
                    track_number: track.track_number,
                    title: track.title.clone(),
                    source_path: track.source_path.clone(),
                },
            };
            let collision_key = match cue_file {
                Some(_) => file_stem_key(&metadata),
                None => path_stem_key(&metadata),
            };
            let slots = collisions
                .entry(collision_key)
                .or_insert_with_key(|key| CollisionSlots::from_inventory(key, inventory));
            let collision_index = match cue_file {
                Some(file) => slots.assign_group(&file.track_ids().collect::<Vec<_>>()),
                None => slots.assign(track.id),
            };
            let forced_extension = match mode {
                TransferMode::Copy => None,
                TransferMode::TranscodeOpus160 => Some("opus"),
                TransferMode::TranscodeMp3 { .. } => Some("mp3"),
            };
            let (device_path, expected_bytes) = match cue_file {
                Some(file) => (
                    device_file_path(&metadata, forced_extension, collision_index),
                    if estimated_files.insert(file.source_path.clone()) {
                        profile.estimated_target_bytes(&SyncTrack {
                            duration_ms: file.duration_ms,
                            ..track.clone()
                        })
                    } else {
                        0
                    },
                ),
                None => (
                    device_track_path(&metadata, forced_extension, collision_index),
                    profile.estimated_target_bytes(&track),
                ),
            };
            (
                index,
                TransferPlanEntry {
                    track,
                    device_path,
                    expected_bytes,
                    mode,
                },
            )
        })
        .collect::<Vec<_>>();
    plan.sort_by_key(|(index, _)| *index);
    plan.into_iter().map(|(_, entry)| entry).collect()
}

/// The path metadata of a CUE file, the same for every track of it whichever
/// are selected: the file's album and album artist, or, with no album artist,
/// the performer of its first track.
fn file_path_metadata(file: &CueSyncFile) -> DevicePathMetadata {
    DevicePathMetadata {
        album_artist: file.album_artist.clone(),
        artist: file
            .tracks
            .first()
            .map(|first| first.performer.clone())
            .unwrap_or_default(),
        album: file.album.clone(),
        track_number: None,
        title: String::new(),
        source_path: file.source_path.clone(),
    }
}

#[derive(Default)]
struct CollisionSlots {
    used: HashSet<usize>,
    owned: HashMap<i64, usize>,
}

impl CollisionSlots {
    fn from_inventory(
        collision_key: &str,
        inventory: &[super::settings::DeviceFileRecord],
    ) -> Self {
        let mut records = inventory.iter().collect::<Vec<_>>();
        records.sort_by_key(|record| record.track_id);
        let mut slots = Self::default();
        for record in records {
            let Some(index) = inventory_collision_index(&record.device_path, collision_key) else {
                continue;
            };
            if slots.used.insert(index) {
                slots.owned.insert(record.track_id, index);
            }
        }
        slots
    }

    /// One slot for every track of one file: the slot any of them already
    /// owns, or a new one they all share.
    fn assign_group(&mut self, track_ids: &[i64]) -> usize {
        let index = match track_ids.iter().find_map(|id| self.owned.get(id).copied()) {
            Some(index) => index,
            None => {
                let mut index = 1;
                while !self.used.insert(index) {
                    index = index.saturating_add(1);
                }
                index
            }
        };
        for id in track_ids {
            self.owned.insert(*id, index);
        }
        index
    }

    fn assign(&mut self, track_id: i64) -> usize {
        if let Some(index) = self.owned.get(&track_id) {
            return *index;
        }
        let mut index = 1;
        while !self.used.insert(index) {
            index = index.saturating_add(1);
        }
        index
    }
}

fn inventory_collision_index(device_path: &str, collision_key: &str) -> Option<usize> {
    let (directory, file_name) = device_path.rsplit_once('/').unwrap_or(("", device_path));
    let file_stem = file_name
        .rsplit_once('.')
        .map_or(file_name, |(stem, _)| stem);
    let inventory_key = if directory.is_empty() {
        file_stem.to_lowercase()
    } else {
        format!("{directory}/{file_stem}").to_lowercase()
    };
    if inventory_key == collision_key {
        return Some(1);
    }
    let suffix = inventory_key.strip_prefix(collision_key)?;
    let index = suffix.strip_prefix(" (")?.strip_suffix(')')?.parse().ok()?;
    (index >= 2).then_some(index)
}

/// The collision key of a file named after its source's stem.
fn file_stem_key(metadata: &DevicePathMetadata) -> String {
    let path = device_file_path(metadata, None, 1);
    path.rsplit_once('.')
        .map_or(path.as_str(), |(stem, _)| stem)
        .to_lowercase()
}

fn path_stem_key(metadata: &DevicePathMetadata) -> String {
    let artist = if metadata.album_artist.trim().is_empty() {
        &metadata.artist
    } else {
        &metadata.album_artist
    };
    let number = metadata.track_number.unwrap_or(0);
    format!(
        "{}/{}/{number:02} {}",
        sanitize_component(artist, "Unknown Artist"),
        sanitize_component(&metadata.album, "Unknown Album"),
        sanitize_component(&metadata.title, "Untitled")
    )
    .to_lowercase()
}
