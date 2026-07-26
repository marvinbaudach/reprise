//! Transfer decisions and device destinations independent of platform I/O.

use super::sanitize::{device_track_path, sanitize_component, DevicePathMetadata};
use super::{Mp3Quality, SyncTrack, TransferAction, TransferProfile};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferMode {
    Copy,
    TranscodeMp3 { quality: Mp3Quality },
}

impl TransferMode {
    pub fn fingerprint(self) -> String {
        match self {
            Self::Copy => "copy-original-mp3-v1".into(),
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
    let mut collisions = HashMap::<String, CollisionSlots>::new();
    let mut indexed = tracks.into_iter().enumerate().collect::<Vec<_>>();
    indexed.sort_by_key(|(_, track)| track.id);
    let mut plan = indexed
        .into_iter()
        .map(|(index, track)| {
            let mode = match profile.action_for(&track) {
                TransferAction::CopyOriginal => TransferMode::Copy,
                TransferAction::TranscodeMp3(quality) => TransferMode::TranscodeMp3 { quality },
            };
            let metadata = DevicePathMetadata {
                album_artist: track.album_artist.clone(),
                artist: track.artist.clone(),
                album: track.album.clone(),
                track_number: track.track_number,
                title: track.title.clone(),
                source_path: track.source_path.clone(),
            };
            let collision_key = path_stem_key(&metadata);
            let collision_index = collisions
                .entry(collision_key)
                .or_insert_with_key(|key| CollisionSlots::from_inventory(key, inventory))
                .assign(track.id);
            let device_path = device_track_path(&metadata, Some("mp3"), collision_index);
            let expected_bytes = profile.estimated_target_bytes(&track);
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

pub(super) fn stable_device_paths(
    tracks: &[SyncTrack],
    forced_extension: &str,
    inventory: &[super::settings::DeviceFileRecord],
) -> HashMap<i64, String> {
    let mut collisions = HashMap::<String, CollisionSlots>::new();
    let mut tracks = tracks.iter().collect::<Vec<_>>();
    tracks.sort_by_key(|track| track.id);
    let mut paths = HashMap::new();
    for track in tracks {
        if paths.contains_key(&track.id) {
            continue;
        }
        let metadata = DevicePathMetadata {
            album_artist: track.album_artist.clone(),
            artist: track.artist.clone(),
            album: track.album.clone(),
            track_number: track.track_number,
            title: track.title.clone(),
            source_path: track.source_path.clone(),
        };
        let collision_key = path_stem_key(&metadata);
        let collision_index = collisions
            .entry(collision_key)
            .or_insert_with_key(|key| CollisionSlots::from_inventory(key, inventory))
            .assign(track.id);
        paths.insert(
            track.id,
            device_track_path(&metadata, Some(forced_extension), collision_index),
        );
    }
    paths
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
