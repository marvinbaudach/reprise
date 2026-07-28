//! Pure device-synchronization planning and progress state.
//!
//! This module deliberately owns no filesystem or platform handles. Linux
//! GIO/GVfs I/O lives in `reprise-platform-linux`; frontends feed validated
//! tracks into the queue and project immutable snapshots into their UI.

use std::collections::{HashSet, VecDeque};
use std::path::{Component, Path, PathBuf};

use crate::library::m3u::{M3uEntry, M3uExportEntry};

pub mod cap;
pub mod category_diff;
pub mod delta;
pub mod m3u;
pub mod mirror;
pub mod page;
pub mod podcasts;
pub mod profile;
pub mod sanitize;
pub mod selection;
pub mod settings;
pub mod snapshot;
pub mod storage;
pub mod targets;
pub mod transfer;

pub use cap::{items_to_evict, CapItem};
pub use category_diff::{
    aggregate_balance, apply_cap, candidate_source, project_category_reading, CandidateSource,
    CategoryDiff, CategoryReading, SyncBalance,
};
pub use delta::{compute_delta, SyncCandidate, SyncDelta};
pub use mirror::{
    plan_mirror, DesiredManagedFile, ManagedDeviceFile, ManagedRemoval, MirrorBlocker, MirrorInput,
    MirrorPlan, MirrorPlaylistProjection, MirrorPlaylistSnapshot, MirrorReplacement, MirrorTrack,
    MirrorWarning, PlaylistWrite, UnavailableTrack,
};
pub use page::{
    project_sync_page, SyncChangeSummary, SyncPageControls, SyncPageInput, SyncPageProjection,
    SyncPageState, SyncPageWarning, SyncPlaylistRow,
};
pub use profile::{
    project_playlist_sizes, Mp3Quality, PlaylistSizeProjection, PlaylistTargetSize, PlaylistTracks,
    TransferAction, TransferProfile, UnsupportedMp3Quality,
};
pub use selection::{
    select_episodes, summarize_playlist_selection, summarize_youtube_selection,
    EpisodeSelectionCandidate, EpisodeSelectionResult, EpisodeSelectionRule,
    PlaylistSelectionSummary, YoutubeChannelToggle, YoutubeSelectionSummary,
};
pub use settings::{
    DeviceFileRecord, DevicePlaylistRecord, DeviceSelection, DeviceSettings, SelectionSource,
};
pub use snapshot::load_mirror_playlist_snapshots;
pub use storage::{
    project_storage, storage_composition, DeviceStorageAccess, DeviceStorageInspection,
    DeviceStorageProjection, DeviceStorageSnapshot, StorageComposition, StorageKnowledge,
    StorageProjectionState,
};
pub use targets::{
    load_or_create_targets, load_target, save_target, target_storage_transition, StorageId,
    StorageTransition, SyncTarget, SyncTargetError, SyncTargetKind,
    PODCAST_EPISODES_DEFAULT_CAP_BYTES, YOUTUBE_AUDIO_DEFAULT_CAP_BYTES,
};

pub const REPRISE_DEVICE_DIR: &str = "Reprise";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManagedRoot {
    Music,
    Podcasts,
}

impl ManagedRoot {
    pub const fn components(self) -> [&'static str; 2] {
        match self {
            Self::Music => ["Music", REPRISE_DEVICE_DIR],
            Self::Podcasts => ["Podcasts", REPRISE_DEVICE_DIR],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncTrack {
    pub id: i64,
    pub source_path: PathBuf,
    pub original_name: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub track_number: Option<u32>,
    pub duration_ms: i64,
    pub bitrate_kbps: Option<u32>,
    pub size_bytes: u64,
    pub source_mtime: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncJob {
    pub id: u64,
    pub playlist: String,
    pub tracks: Vec<SyncTrack>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncPhase {
    Idle,
    Preparing,
    Copying,
    PausedDisconnected,
    Cancelling,
    Complete,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackOutcome {
    Copied,
    Skipped,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncSnapshot {
    pub phase: SyncPhase,
    pub current_name: Option<String>,
    pub current_bytes: u64,
    pub current_total: Option<u64>,
    pub completed_tracks: usize,
    pub total_tracks: usize,
    pub completed_bytes: u64,
    pub total_bytes: u64,
    pub queued_jobs: usize,
    pub copied: usize,
    pub skipped: usize,
    pub failed: usize,
    pub message: Option<String>,
}

impl SyncSnapshot {
    fn idle() -> Self {
        Self {
            phase: SyncPhase::Idle,
            current_name: None,
            current_bytes: 0,
            current_total: None,
            completed_tracks: 0,
            total_tracks: 0,
            completed_bytes: 0,
            total_bytes: 0,
            queued_jobs: 0,
            copied: 0,
            skipped: 0,
            failed: 0,
            message: None,
        }
    }

    fn for_job(job: &SyncJob, queued_jobs: usize) -> Self {
        Self {
            phase: SyncPhase::Preparing,
            total_tracks: job.tracks.len(),
            total_bytes: job.tracks.iter().map(|track| track.size_bytes).sum(),
            queued_jobs,
            ..Self::idle()
        }
    }
}

pub struct DeviceQueue {
    pending: VecDeque<SyncJob>,
    active: Option<SyncJob>,
    snapshot: SyncSnapshot,
    paused: bool,
}

impl Default for DeviceQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceQueue {
    pub fn new() -> Self {
        Self {
            pending: VecDeque::new(),
            active: None,
            snapshot: SyncSnapshot::idle(),
            paused: false,
        }
    }

    pub fn enqueue(&mut self, job: SyncJob) {
        self.pending.push_back(job);
        self.snapshot.queued_jobs = self.pending.len();
    }

    pub fn start_next(&mut self) -> Option<SyncJob> {
        if self.paused || self.active.is_some() {
            return None;
        }
        let job = self.pending.pop_front()?;
        self.snapshot = SyncSnapshot::for_job(&job, self.pending.len());
        self.active = Some(job.clone());
        Some(job)
    }

    pub fn begin_track(&mut self, name: &str, total: Option<u64>) {
        if self.active.is_none() || self.paused {
            return;
        }
        self.snapshot.phase = SyncPhase::Copying;
        self.snapshot.current_name = Some(name.to_string());
        self.snapshot.current_bytes = 0;
        self.snapshot.current_total = total;
        self.snapshot.message = None;
    }

    pub fn set_track_bytes(&mut self, copied: u64) {
        if self.snapshot.phase != SyncPhase::Copying {
            return;
        }
        let copied = self
            .snapshot
            .current_total
            .map_or(copied, |total| copied.min(total));
        self.snapshot.current_bytes = self.snapshot.current_bytes.max(copied);
    }

    pub fn finish_track(&mut self, outcome: TrackOutcome) {
        if self.active.is_none() || self.snapshot.current_name.is_none() {
            return;
        }
        self.snapshot.completed_tracks = self.snapshot.completed_tracks.saturating_add(1);
        self.snapshot.completed_bytes = self
            .snapshot
            .completed_bytes
            .saturating_add(self.snapshot.current_bytes)
            .min(self.snapshot.total_bytes);
        match outcome {
            TrackOutcome::Copied => self.snapshot.copied += 1,
            TrackOutcome::Skipped => self.snapshot.skipped += 1,
            TrackOutcome::Failed => self.snapshot.failed += 1,
        }
        self.snapshot.current_name = None;
        self.snapshot.current_bytes = 0;
        self.snapshot.current_total = None;
        self.snapshot.phase = SyncPhase::Preparing;
    }

    pub fn finish_job(&mut self) {
        if self.active.take().is_none() {
            return;
        }
        self.paused = false;
        self.snapshot.phase = SyncPhase::Complete;
        self.snapshot.current_name = None;
        self.snapshot.current_bytes = 0;
        self.snapshot.current_total = None;
        self.snapshot.queued_jobs = self.pending.len();
    }

    pub fn fail_job(&mut self, message: impl Into<String>) {
        if self.active.take().is_none() {
            return;
        }
        self.paused = false;
        self.snapshot.phase = SyncPhase::Failed;
        self.snapshot.current_name = None;
        self.snapshot.current_bytes = 0;
        self.snapshot.current_total = None;
        self.snapshot.queued_jobs = self.pending.len();
        self.snapshot.message = Some(message.into());
    }

    pub fn request_cancel(&mut self) {
        if self.active.is_some() {
            self.snapshot.phase = SyncPhase::Cancelling;
        }
    }

    pub fn pause_disconnected(&mut self) {
        if self.active.is_some() {
            self.paused = true;
            self.snapshot.phase = SyncPhase::PausedDisconnected;
        }
    }

    pub fn resume(&mut self) {
        if !self.paused {
            return;
        }
        self.paused = false;
        self.snapshot.phase = if self.active.is_some() {
            SyncPhase::Preparing
        } else {
            SyncPhase::Idle
        };
    }

    pub fn snapshot(&self) -> SyncSnapshot {
        self.snapshot.clone()
    }
}

pub fn safe_component(input: &str, fallback: &str) -> String {
    let replaced = input
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | '<' | '>' | ':' | '"' | '|' | '?' | '*'
                )
            {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    let trimmed =
        replaced.trim_matches(|character: char| character == '.' || character.is_whitespace());
    let safe = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
    if safe.is_empty() || matches!(safe.as_str(), "." | "..") {
        fallback.to_string()
    } else {
        safe
    }
}

pub fn track_relative_path(playlist: &str, track: &SyncTrack) -> String {
    let playlist = safe_component(playlist, "Playlist");
    let file_name = safe_component(&track.original_name, "track");
    format!("{playlist}/{}-{file_name}", track.id)
}

pub fn merge_playlist_entries(existing: &[M3uEntry], appended: &[M3uExportEntry]) -> String {
    let mut seen = HashSet::new();
    let mut output = String::from("#EXTM3U\n");
    for entry in existing {
        if safe_playlist_path(&entry.path) && seen.insert(entry.path.as_str()) {
            output.push_str(&entry.path);
            output.push('\n');
        }
    }
    for entry in appended {
        if safe_playlist_path(&entry.path) && seen.insert(entry.path.as_str()) {
            let display = entry
                .display
                .chars()
                .map(|character| {
                    if character.is_control() {
                        ' '
                    } else {
                        character
                    }
                })
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            output.push_str(&format!(
                "#EXTINF:{},{}\n{}\n",
                entry.duration_secs, display, entry.path
            ));
        }
    }
    output
}

fn safe_playlist_path(path: &str) -> bool {
    !path.is_empty()
        && !path.chars().any(char::is_control)
        && !Path::new(path).is_absolute()
        && Path::new(path)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

#[cfg(test)]
#[path = "device_sync/v1_tests.rs"]
mod v1_tests;

#[cfg(test)]
#[path = "device_sync/page_tests.rs"]
mod page_tests;
#[cfg(test)]
#[path = "device_sync/profile_tests.rs"]
mod profile_tests;
#[cfg(test)]
#[path = "device_sync/storage_tests.rs"]
mod storage_tests;

#[cfg(test)]
#[path = "device_sync/inventory_tests.rs"]
mod inventory_tests;

#[cfg(test)]
#[path = "device_sync/mirror_tests.rs"]
mod mirror_tests;

#[cfg(test)]
mod tests {
    use std::path::{Component, Path};

    use super::*;
    use crate::library::m3u::{M3uEntry, M3uExportEntry};

    fn track(id: i64, original_name: &str, size_bytes: u64) -> SyncTrack {
        SyncTrack {
            id,
            source_path: format!("/library/{original_name}").into(),
            original_name: original_name.to_string(),
            title: format!("Track {id}"),
            artist: "Artist".to_string(),
            album: "Album".to_string(),
            album_artist: "Artist".to_string(),
            track_number: Some(id.max(0) as u32),
            duration_ms: 42_000,
            bitrate_kbps: None,
            size_bytes,
            source_mtime: 1,
        }
    }

    fn job(id: u64, playlist: &str, ids: &[i64]) -> SyncJob {
        SyncJob {
            id,
            playlist: playlist.to_string(),
            tracks: ids
                .iter()
                .map(|id| track(*id, &format!("same-{id}.flac"), 100))
                .collect(),
        }
    }

    #[test]
    fn safe_component_preserves_unicode_and_removes_path_syntax() {
        assert_eq!(
            safe_component("  Übung / Mix\\One\n ", "Playlist"),
            "Übung Mix One"
        );
        assert_eq!(safe_component(".", "Playlist"), "Playlist");
        assert_eq!(safe_component("..", "Playlist"), "Playlist");
        assert_eq!(safe_component("///", "Playlist"), "Playlist");
    }

    #[test]
    fn safe_component_removes_cross_platform_reserved_punctuation() {
        assert_eq!(safe_component("A:<B>|C?*\"", "Playlist"), "A B C");
        assert_eq!(safe_component("  \t\n", "Fallback"), "Fallback");
    }

    #[test]
    fn track_targets_are_relative_traversal_free_and_id_stable() {
        let relative = track_relative_path("../Road / Mix", &track(17, "../song.flac", 12));
        assert_eq!(relative, "Road Mix/17-song.flac");
        assert!(!Path::new(&relative).is_absolute());
        assert!(Path::new(&relative)
            .components()
            .all(|part| !matches!(part, Component::ParentDir | Component::RootDir)));

        let other = track_relative_path("Road Mix", &track(18, "song.flac", 12));
        assert_ne!(relative, other);
    }

    #[test]
    fn empty_track_and_playlist_names_use_stable_fallbacks() {
        assert_eq!(
            track_relative_path("..", &track(7, "///", 1)),
            "Playlist/7-track"
        );
    }

    #[test]
    fn playlist_merge_preserves_existing_order_and_appends_unique_paths() {
        let existing = vec![
            M3uEntry {
                path: "Old/a.flac".into(),
            },
            M3uEntry {
                path: "Old/b.flac".into(),
            },
        ];
        let appended = vec![
            M3uExportEntry {
                path: "Old/b.flac".into(),
                duration_secs: 2,
                display: "Duplicate".into(),
            },
            M3uExportEntry {
                path: "New/c.flac".into(),
                duration_secs: 42,
                display: "Artist - New".into(),
            },
        ];

        assert_eq!(
            merge_playlist_entries(&existing, &appended),
            "#EXTM3U\nOld/a.flac\nOld/b.flac\n#EXTINF:42,Artist - New\nNew/c.flac\n"
        );
    }

    #[test]
    fn playlist_merge_deduplicates_existing_entries_too() {
        let existing = vec![
            M3uEntry {
                path: "a.flac".into(),
            },
            M3uEntry {
                path: "a.flac".into(),
            },
        ];
        assert_eq!(merge_playlist_entries(&existing, &[]), "#EXTM3U\na.flac\n");
    }

    #[test]
    fn empty_playlist_merge_is_a_valid_m3u8() {
        assert_eq!(merge_playlist_entries(&[], &[]), "#EXTM3U\n");
    }

    #[test]
    fn playlist_merge_rejects_unsafe_paths_and_flattens_display_lines() {
        let appended = vec![
            M3uExportEntry {
                path: "../outside.flac".into(),
                duration_secs: 1,
                display: "Unsafe".into(),
            },
            M3uExportEntry {
                path: "Mix/1-safe.flac".into(),
                duration_secs: 2,
                display: "Artist\nInjected - Title".into(),
            },
        ];
        assert_eq!(
            merge_playlist_entries(&[], &appended),
            "#EXTM3U\n#EXTINF:2,Artist Injected - Title\nMix/1-safe.flac\n"
        );
    }

    #[test]
    fn empty_queue_has_an_idle_snapshot_and_cannot_start() {
        let mut queue = DeviceQueue::new();
        assert!(queue.start_next().is_none());
        assert_eq!(queue.snapshot(), SyncSnapshot::idle());
    }

    #[test]
    fn same_device_jobs_start_strictly_fifo_and_never_overlap() {
        let mut queue = DeviceQueue::new();
        queue.enqueue(job(1, "A", &[1, 2]));
        queue.enqueue(job(2, "B", &[3]));
        queue.enqueue(job(3, "C", &[4]));

        assert_eq!(queue.start_next().unwrap().id, 1);
        assert!(queue.start_next().is_none());
        assert_eq!(queue.snapshot().queued_jobs, 2);
        queue.finish_job();
        assert_eq!(queue.start_next().unwrap().id, 2);
        queue.finish_job();
        assert_eq!(queue.start_next().unwrap().id, 3);
    }

    #[test]
    fn active_snapshot_reports_job_totals_and_late_enqueues() {
        let mut queue = DeviceQueue::new();
        queue.enqueue(job(1, "A", &[1, 2]));
        queue.start_next().unwrap();
        let started = queue.snapshot();
        assert_eq!((started.total_tracks, started.total_bytes), (2, 200));
        queue.enqueue(job(2, "B", &[3]));
        assert_eq!(queue.snapshot().queued_jobs, 1);
    }

    #[test]
    fn progress_is_monotone_clamped_and_counts_track_outcomes() {
        let mut queue = DeviceQueue::new();
        queue.enqueue(job(1, "A", &[1, 2, 3]));
        queue.start_next().unwrap();
        queue.begin_track("one.flac", Some(100));
        queue.set_track_bytes(60);
        queue.set_track_bytes(20);
        queue.set_track_bytes(999);
        let copying = queue.snapshot();
        assert_eq!(copying.phase, SyncPhase::Copying);
        assert_eq!(copying.current_bytes, 100);
        assert_eq!(copying.current_total, Some(100));

        queue.finish_track(TrackOutcome::Copied);
        queue.begin_track("two.flac", Some(100));
        queue.set_track_bytes(100);
        queue.finish_track(TrackOutcome::Skipped);
        queue.begin_track("three.flac", Some(100));
        queue.set_track_bytes(40);
        queue.finish_track(TrackOutcome::Failed);
        let done = queue.snapshot();
        assert_eq!(done.completed_tracks, 3);
        assert_eq!(done.completed_bytes, 240);
        assert_eq!((done.copied, done.skipped, done.failed), (1, 1, 1));
    }

    #[test]
    fn cancel_only_changes_an_active_job_and_preserves_waiting_jobs() {
        let mut queue = DeviceQueue::new();
        queue.request_cancel();
        assert_eq!(queue.snapshot().phase, SyncPhase::Idle);
        queue.enqueue(job(1, "A", &[1]));
        queue.enqueue(job(2, "B", &[2]));
        queue.start_next().unwrap();
        queue.request_cancel();
        assert_eq!(queue.snapshot().phase, SyncPhase::Cancelling);
        assert_eq!(queue.snapshot().queued_jobs, 1);
        queue.finish_job();
        assert_eq!(queue.start_next().unwrap().id, 2);
    }

    #[test]
    fn failed_job_records_message_and_allows_the_next_job() {
        let mut queue = DeviceQueue::new();
        queue.enqueue(job(1, "A", &[1]));
        queue.enqueue(job(2, "B", &[2]));
        queue.start_next().unwrap();
        queue.fail_job("playlist write failed");
        let failed = queue.snapshot();
        assert_eq!(failed.phase, SyncPhase::Failed);
        assert_eq!(failed.message.as_deref(), Some("playlist write failed"));
        assert_eq!(queue.start_next().unwrap().id, 2);
    }

    #[test]
    fn disconnect_pauses_the_active_job_until_resume() {
        let mut queue = DeviceQueue::new();
        queue.enqueue(job(1, "A", &[1]));
        queue.start_next().unwrap();
        queue.begin_track("one.flac", Some(100));
        queue.set_track_bytes(25);
        queue.pause_disconnected();
        assert_eq!(queue.snapshot().phase, SyncPhase::PausedDisconnected);
        assert!(queue.start_next().is_none());

        queue.resume();
        let resumed = queue.snapshot();
        assert_eq!(resumed.phase, SyncPhase::Preparing);
        assert_eq!(resumed.current_bytes, 25);
    }
}
