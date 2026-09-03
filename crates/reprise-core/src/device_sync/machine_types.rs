use super::{DesiredManagedFile, DeviceFileRecord, SelectionSource};
use crate::device_sync::TransferAction;

/// The step a run is currently working on.
///
/// The variants are ordered as the user meets them in the interface, not as
/// the run executes them — see [`SyncStep::Removing`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncStep {
    Removing,
    Transcoding,
    Copying,
    WritingAnalysis,
    WritingLyrics,
    WritingPlaylists,
    WritingTrackMetadata,
}

impl SyncStep {
    pub fn reports_transfer_rate(self) -> bool {
        matches!(
            self,
            Self::Copying | Self::WritingAnalysis | Self::WritingLyrics
        )
    }
}

/// The externally visible progress of a run.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum PlannedSyncPhase {
    #[default]
    Idle,
    ComputingDelta,
    Syncing {
        step: SyncStep,
        done: u32,
        total: u32,
        current_track: String,
        unit_bytes_done: u64,
        unit_bytes_total: u64,
    },
    Finishing,
}

/// Where the bytes of a copy come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferSource {
    /// The track's own file, copied unchanged.
    Original,
    /// The temporary file produced by the preceding [`Effect::Transcode`].
    Transcoded,
}

/// Work the owner must perform on the machine's behalf.
///
/// Every variant that names an `index` indexes the collection its step walks:
/// [`super::DeviceSyncMachine::transfers`] for transfers, `plan.playlist_writes`,
/// `plan.playlist_removals` and `plan.remove` for the rest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Delete leftover partial files under the device root.
    CleanPartials(Vec<String>),
    Transcode {
        index: usize,
        action: TransferAction,
    },
    CopyTrack {
        index: usize,
        source: TransferSource,
        bytes: u64,
    },
    /// Write the inventory row for a copied track.
    RecordFile {
        index: usize,
        device_size: u64,
        device_path: String,
    },
    WriteAnalysis {
        index: usize,
    },
    WriteLyrics {
        index: usize,
    },
    WritePlaylist {
        index: usize,
        omit_relative_paths: Vec<String>,
    },
    /// Write the inventory row for a written playlist.
    RecordPlaylist {
        index: usize,
    },
    RemoveTrack {
        index: usize,
    },
    /// Drop the inventory row of a removed track.
    ForgetFile {
        index: usize,
    },
    /// Delete a path that a replacement superseded.
    RemoveReplacedFile {
        device_path: String,
    },
    RemovePlaylist {
        index: usize,
    },
    /// Drop the inventory row of a playlist that is no longer mirrored.
    ForgetPlaylist {
        index: usize,
    },
    WriteTrackMetadataList,
    Finished(SyncOutcome),
}

/// Facts reported by the backend after one track reached the device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopiedTrack {
    pub device_size: u64,
    pub device_path: String,
}

/// How a run ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncOutcome {
    Completed {
        /// The playlist sources whose contents the owner should verify.
        verified_sources: Vec<SelectionSource>,
    },
    Cancelled,
    Failed {
        /// Set when a whole stage failed rather than individual tracks. The
        /// frontend composes the message it shows; wording is not the core's
        /// business, so a run that merely lost tracks reports only their ids.
        terminal_error: Option<String>,
        failed_tracks: Vec<i64>,
        /// Playlist sources that were still safely published during the run.
        verified_sources: Vec<SelectionSource>,
    },
}

/// The outcome of the effect the machine is waiting for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Start,
    PartialsCleaned(Result<(), String>),
    /// Carries the size of the produced temporary file.
    Transcoded(Result<u64, String>),
    /// Carries the number of bytes and relative path actually written.
    TrackCopied(Result<CopiedTrack, String>),
    FileRecorded(Result<(), String>),
    /// Bytes written so far for the copy in flight.
    CopyProgress {
        copied: u64,
    },
    AnalysisWritten(Result<u64, String>),
    LyricsWritten(Result<u64, String>),
    PlaylistWritten(Result<(), String>),
    PlaylistRecorded(Result<(), String>),
    TrackRemoved(Result<(), String>),
    FileForgotten(Result<(), String>),
    ReplacedFileRemoved(Result<(), String>),
    PlaylistRemoved(Result<(), String>),
    PlaylistForgotten(Result<(), String>),
    TrackMetadataListWritten(Result<(), String>),
    Cancel,
}

/// One planned transfer together with the inventory row it supersedes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferOperation {
    pub desired: DesiredManagedFile,
    pub previous: Option<DeviceFileRecord>,
}
