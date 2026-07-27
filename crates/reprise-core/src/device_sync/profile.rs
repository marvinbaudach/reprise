//! MP3 transfer profiles and pure target-size projections.

use std::collections::HashSet;

use super::{SelectionSource, SyncTrack};

/// Room for MP3 frame rounding and ID3 container structures in addition to
/// every source-derived byte (including an embedded cover) reserved below.
const MP3_CONTAINER_RESERVE_BYTES: u64 = 64 * 1_024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mp3Quality {
    Kbps128,
    Kbps192,
    #[default]
    Kbps256,
    Kbps320,
}

impl Mp3Quality {
    pub const ALL: [Self; 4] = [Self::Kbps128, Self::Kbps192, Self::Kbps256, Self::Kbps320];

    pub const fn kbps(self) -> u32 {
        match self {
            Self::Kbps128 => 128,
            Self::Kbps192 => 192,
            Self::Kbps256 => 256,
            Self::Kbps320 => 320,
        }
    }

    const fn fingerprint(self) -> &'static str {
        match self {
            Self::Kbps128 => "mp3-cbr-128-v1",
            Self::Kbps192 => "mp3-cbr-192-v1",
            Self::Kbps256 => "mp3-cbr-256-v1",
            Self::Kbps320 => "mp3-cbr-320-v1",
        }
    }
}

impl TryFrom<u32> for Mp3Quality {
    type Error = UnsupportedMp3Quality;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            128 => Ok(Self::Kbps128),
            192 => Ok(Self::Kbps192),
            256 => Ok(Self::Kbps256),
            320 => Ok(Self::Kbps320),
            _ => Err(UnsupportedMp3Quality(value)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unsupported MP3 quality: {0} kbps")]
pub struct UnsupportedMp3Quality(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferProfile {
    Mp3(Mp3Quality),
}

impl Default for TransferProfile {
    fn default() -> Self {
        Self::Mp3(Mp3Quality::default())
    }
}

impl TransferProfile {
    pub const fn fingerprint(self) -> &'static str {
        match self {
            Self::Mp3(quality) => quality.fingerprint(),
        }
    }

    pub fn action_for(self, track: &SyncTrack) -> TransferAction {
        match self {
            Self::Mp3(quality)
                if is_mp3(&track.source_path)
                    && track
                        .bitrate_kbps
                        .is_some_and(|bitrate| bitrate > 0 && bitrate <= quality.kbps()) =>
            {
                TransferAction::CopyOriginal
            }
            Self::Mp3(quality) => TransferAction::TranscodeMp3(quality),
        }
    }

    pub fn estimated_target_bytes(self, track: &SyncTrack) -> u64 {
        match self.action_for(track) {
            TransferAction::CopyOriginal => track.size_bytes,
            TransferAction::TranscodeMp3(quality) => {
                let Ok(duration_ms) = u64::try_from(track.duration_ms) else {
                    return u64::MAX;
                };
                if duration_ms == 0 {
                    return u64::MAX;
                }
                let audio_payload = duration_ms
                    .saturating_mul(u64::from(quality.kbps()))
                    .div_ceil(8);
                // Tags and cover art are copied from the source. Reserving the
                // complete source size bounds all of that source-derived data
                // without parsing files in the pure planner. The fixed tail
                // covers ID3/frame structure and encoder rounding.
                audio_payload
                    .saturating_add(track.size_bytes)
                    .saturating_add(MP3_CONTAINER_RESERVE_BYTES)
            }
        }
    }

    pub fn output_fingerprint(self, track: &SyncTrack) -> &'static str {
        match self.action_for(track) {
            TransferAction::CopyOriginal => "copy-original-mp3-v1",
            TransferAction::TranscodeMp3(quality) => quality.fingerprint(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferAction {
    CopyOriginal,
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

fn is_mp3(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("mp3"))
}
