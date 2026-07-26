//! Transfer decisions and device destinations independent of platform I/O.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::sanitize::{device_track_path, sanitize_component, DevicePathMetadata};
use super::SyncTrack;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferMode {
    Copy,
    TranscodeOpus { bitrate: u32 },
}

impl TransferMode {
    pub fn fingerprint(self) -> String {
        match self {
            Self::Copy => "source-copy-v1".into(),
            Self::TranscodeOpus { bitrate } => format!("legacy-opus-{bitrate}-v1"),
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

pub fn build_transfer_plan(tracks: Vec<SyncTrack>, opus_bitrate: u32) -> Vec<TransferPlanEntry> {
    build_transfer_plan_with_inventory(tracks, opus_bitrate, &[])
}

pub fn build_transfer_plan_with_inventory(
    tracks: Vec<SyncTrack>,
    opus_bitrate: u32,
    inventory: &[super::settings::DeviceFileRecord],
) -> Vec<TransferPlanEntry> {
    let mut collisions = HashMap::<String, CollisionSlots>::new();
    let mut indexed = tracks.into_iter().enumerate().collect::<Vec<_>>();
    indexed.sort_by_key(|(_, track)| track.id);
    let mut plan = indexed
        .into_iter()
        .map(|(index, track)| {
            let mode = transfer_mode(&track.source_path, opus_bitrate);
            let forced_extension =
                matches!(mode, TransferMode::TranscodeOpus { .. }).then_some("opus");
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
            let device_path = device_track_path(&metadata, forced_extension, collision_index);
            let expected_bytes = match mode {
                TransferMode::Copy => track.size_bytes,
                TransferMode::TranscodeOpus { bitrate } => transcode_size(&track, bitrate),
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

pub fn transfer_mode(path: &Path, opus_bitrate: u32) -> TransferMode {
    if opus_bitrate == 0 || !is_lossless(path) {
        TransferMode::Copy
    } else {
        TransferMode::TranscodeOpus {
            bitrate: opus_bitrate,
        }
    }
}

fn is_lossless(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "flac" | "wav" | "wave" | "aiff" | "aif" | "alac" | "wv"
            )
        })
}

fn transcode_size(track: &SyncTrack, bitrate: u32) -> u64 {
    u64::try_from(track.duration_ms.max(1))
        .unwrap_or(0)
        .saturating_mul(u64::from(bitrate))
        .div_ceil(8)
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
