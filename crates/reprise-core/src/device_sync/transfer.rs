//! Transfer decisions and device destinations independent of platform I/O.

use std::collections::HashMap;
use std::path::Path;

use super::sanitize::{device_track_path, sanitize_component, DevicePathMetadata};
use super::SyncTrack;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferMode {
    Copy,
    TranscodeOpus { bitrate: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferPlanEntry {
    pub track: SyncTrack,
    pub device_path: String,
    pub expected_bytes: u64,
    pub mode: TransferMode,
}

pub fn build_transfer_plan(tracks: Vec<SyncTrack>, opus_bitrate: u32) -> Vec<TransferPlanEntry> {
    let mut collisions = HashMap::<String, usize>::new();
    tracks
        .into_iter()
        .map(|track| {
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
                .and_modify(|count| *count += 1)
                .or_insert(1);
            let device_path = device_track_path(&metadata, forced_extension, *collision_index);
            let expected_bytes = match mode {
                TransferMode::Copy => track.size_bytes,
                TransferMode::TranscodeOpus { bitrate } => transcode_size(&track, bitrate),
            };
            TransferPlanEntry {
                track,
                device_path,
                expected_bytes,
                mode,
            }
        })
        .collect()
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
    u64::try_from(track.duration_ms.max(0))
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
