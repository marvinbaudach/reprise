//! Modern transfer profiles and pure target-size projections.

use std::collections::HashSet;

use super::{SelectionSource, SyncTrack};

/// Room for encoder rounding and container structures in addition to
/// every source-derived byte (including an embedded cover) reserved below.
const TRANSCODE_CONTAINER_RESERVE_BYTES: u64 = 64 * 1_024;

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
pub enum Mp3Quality {
    #[default]
    Kbps256,
}

impl Mp3Quality {
    pub const ALL: [Self; 1] = [Self::Kbps256];

    pub const fn kbps(self) -> u32 {
        match self {
            Self::Kbps256 => 256,
        }
    }

    const fn fingerprint(self) -> &'static str {
        match self {
            Self::Kbps256 => "mp3-cbr-256-v1",
        }
    }
}

impl TryFrom<u32> for Mp3Quality {
    type Error = UnsupportedMp3Quality;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            256 => Ok(Self::Kbps256),
            _ => Err(UnsupportedMp3Quality(value)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unsupported MP3 quality: {0} kbps")]
pub struct UnsupportedMp3Quality(pub u32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TransferProfile {
    #[default]
    Opus160,
    Mp3(Mp3Quality),
    Original,
}

impl TransferProfile {
    pub const ALL: [Self; 3] = [
        Self::Opus160,
        Self::Mp3(Mp3Quality::Kbps256),
        Self::Original,
    ];

    pub const fn storage_value(self) -> &'static str {
        match self {
            Self::Opus160 => "opus_160",
            Self::Mp3(Mp3Quality::Kbps256) => "mp3_256",
            Self::Original => "original",
        }
    }

    pub fn from_storage_value(value: &str) -> Option<Self> {
        match value {
            "opus_160" => Some(Self::Opus160),
            "mp3_256" => Some(Self::Mp3(Mp3Quality::Kbps256)),
            "original" => Some(Self::Original),
            _ => None,
        }
    }

    pub const fn fingerprint(self) -> &'static str {
        match self {
            Self::Opus160 => "opus-vbr-160-v1",
            Self::Mp3(quality) => quality.fingerprint(),
            Self::Original => "copy-original-v1",
        }
    }

    pub fn action_for(self, track: &SyncTrack) -> TransferAction {
        match self {
            Self::Original => TransferAction::CopyOriginal,
            Self::Opus160 if is_known_lossless(&track.source_path) => {
                TransferAction::TranscodeOpus160
            }
            Self::Mp3(quality) if is_known_lossless(&track.source_path) => {
                TransferAction::TranscodeMp3(quality)
            }
            Self::Opus160 | Self::Mp3(_) => TransferAction::CopyOriginal,
        }
    }

    pub fn estimated_target_bytes(self, track: &SyncTrack) -> u64 {
        match self.action_for(track) {
            TransferAction::CopyOriginal => track.size_bytes,
            TransferAction::TranscodeOpus160 => estimated_transcode_bytes(track, 160),
            TransferAction::TranscodeMp3(quality) => {
                estimated_transcode_bytes(track, quality.kbps())
            }
        }
    }

    pub fn output_fingerprint(self, track: &SyncTrack) -> &'static str {
        match self.action_for(track) {
            TransferAction::CopyOriginal => "copy-original-v1",
            TransferAction::TranscodeOpus160 => Self::Opus160.fingerprint(),
            TransferAction::TranscodeMp3(quality) => quality.fingerprint(),
        }
    }
}

fn estimated_transcode_bytes(track: &SyncTrack, bitrate_kbps: u32) -> u64 {
    let Ok(duration_ms) = u64::try_from(track.duration_ms) else {
        return u64::MAX;
    };
    if duration_ms == 0 {
        return u64::MAX;
    }
    let audio_payload = duration_ms
        .saturating_mul(u64::from(bitrate_kbps))
        .div_ceil(8);
    // Tags and cover art are copied from the source. Reserving the complete
    // source size bounds all of that source-derived data without parsing
    // files in the pure planner. The fixed tail covers container structure
    // and encoder rounding.
    audio_payload
        .saturating_add(track.size_bytes)
        .saturating_add(TRANSCODE_CONTAINER_RESERVE_BYTES)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferAction {
    CopyOriginal,
    TranscodeOpus160,
    TranscodeMp3(Mp3Quality),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaylistTracks {
    pub source: SelectionSource,
    pub name: String,
    pub tracks: Vec<SyncTrack>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaylistTargetSize {
    pub source: SelectionSource,
    pub name: String,
    pub entry_count: usize,
    pub unique_track_count: usize,
    pub target_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaylistSizeProjection {
    pub playlists: Vec<PlaylistTargetSize>,
    pub unique_track_count: usize,
    pub target_bytes: u64,
}

pub fn project_playlist_sizes(
    playlists: &[PlaylistTracks],
    profile: TransferProfile,
) -> PlaylistSizeProjection {
    let mut union = HashSet::new();
    let mut union_bytes = 0_u64;
    let playlists = playlists
        .iter()
        .map(|playlist| {
            let mut unique = HashSet::new();
            let mut target_bytes = 0_u64;
            for track in &playlist.tracks {
                if unique.insert(track.id) {
                    target_bytes =
                        target_bytes.saturating_add(profile.estimated_target_bytes(track));
                }
                if union.insert(track.id) {
                    union_bytes = union_bytes.saturating_add(profile.estimated_target_bytes(track));
                }
            }
            PlaylistTargetSize {
                source: playlist.source.clone(),
                name: playlist.name.clone(),
                entry_count: playlist.tracks.len(),
                unique_track_count: unique.len(),
                target_bytes,
            }
        })
        .collect();

    PlaylistSizeProjection {
        playlists,
        unique_track_count: union.len(),
        target_bytes: union_bytes,
    }
}

fn is_known_lossless(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            ["flac", "wav", "wave", "aif", "aiff", "alac"]
                .iter()
                .any(|candidate| extension.eq_ignore_ascii_case(candidate))
        })
}
