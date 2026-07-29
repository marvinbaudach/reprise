//! The device-synchronization state machine.
//!
//! One [`DeviceSyncMachine`] owns exactly one synchronization run for exactly
//! one device. It is pure: it holds no cancellable, no database handle, no
//! file handle and no toolkit type. Callers answer each emitted [`Effect`]
//! with the matching [`Event`] and read [`DeviceSyncMachine::phase`] for the
//! projection their frontend renders.
//!
//! Because a machine owns a single run, a superseded run cannot deliver a late
//! event to it — the owner drops the machine instead. That replaces the
//! generation counters the GTK runtime needed while every device shared one
//! mutable state list.
//!
//! Cancellation is a plain flag. The platform keeps whatever cancellation
//! primitive its I/O needs; the machine simply stops emitting work.

use std::collections::HashSet;

use super::{
    DesiredManagedFile, DeviceFileRecord, ManagedRemoval, MirrorPlan, SelectionSource,
    TransferAction,
};

/// The step a run is currently working on.
///
/// The variants are ordered as the user meets them in the interface, not as
/// the run executes them — see [`SyncStep::Removing`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncStep {
    Removing,
    Transcoding,
    Copying,
    WritingPlaylists,
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
        bytes_done: u64,
        bytes_total: u64,
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
/// [`DeviceSyncMachine::transfers`] for transfers, `plan.playlist_writes`,
/// `plan.playlist_removals` and `plan.remove` for the rest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Delete leftover partial files under the device root.
    CleanPartials,
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
    },
    WritePlaylist {
        index: usize,
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
    Finished(SyncOutcome),
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
    },
}

/// The outcome of the effect the machine is waiting for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Start,
    PartialsCleaned(Result<(), String>),
    /// Carries the size of the produced temporary file.
    Transcoded(Result<u64, String>),
    /// Carries the number of bytes actually written to the device.
    TrackCopied(Result<u64, String>),
    FileRecorded(Result<(), String>),
    /// Bytes written so far for the copy in flight.
    CopyProgress {
        copied: u64,
    },
    PlaylistWritten(Result<(), String>),
    PlaylistRecorded(Result<(), String>),
    TrackRemoved(Result<(), String>),
    FileForgotten(Result<(), String>),
    ReplacedFileRemoved(Result<(), String>),
    PlaylistRemoved(Result<(), String>),
    PlaylistForgotten(Result<(), String>),
    Cancel,
}

/// One planned transfer together with the inventory row it supersedes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferOperation {
    pub desired: DesiredManagedFile,
    pub previous: Option<DeviceFileRecord>,
}

/// The effect the machine is currently waiting to hear back about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Awaiting {
    Start,
    Partials,
    Transcode(usize),
    Copy(usize),
    RecordFile(usize),
    WritePlaylist(usize),
    RecordPlaylist(usize),
    RemovePlaylist(usize),
    ForgetPlaylist(usize),
    RemoveTrack(usize),
    ForgetFile(usize),
    RemoveReplacedFile(usize),
    Done,
}

pub struct DeviceSyncMachine {
    device_id: String,
    plan: MirrorPlan,
    transfers: Vec<TransferOperation>,
    awaiting: Awaiting,
    phase: PlannedSyncPhase,
    cancelled: bool,
    terminal_error: Option<String>,
    failures: Vec<i64>,
    /// Device paths whose transfer failed. A playlist that would point at one
    /// of them must not be published.
    failed_device_paths: HashSet<String>,
    completed_bytes: u64,
    transcoded_bytes: Option<u64>,
    deferred_replacements: Vec<(String, i64)>,
    planned_playlist_sources: HashSet<SelectionSource>,
    successful_playlist_sources: HashSet<SelectionSource>,
    /// Set when a playlist file the device should no longer hold is still
    /// there because its deletion failed.
    stale_playlist_on_device: bool,
}

impl DeviceSyncMachine {
    pub fn new(device_id: String, plan: MirrorPlan) -> Self {
        let transfers = transfer_operations(&plan);
        let planned_playlist_sources = plan
            .playlist_writes
            .iter()
            .map(|write| write.source.clone())
            .collect();
        Self {
            device_id,
            plan,
            transfers,
            awaiting: Awaiting::Start,
            phase: PlannedSyncPhase::Idle,
            cancelled: false,
            terminal_error: None,
            failures: Vec::new(),
            failed_device_paths: HashSet::new(),
            completed_bytes: 0,
            transcoded_bytes: None,
            deferred_replacements: Vec::new(),
            planned_playlist_sources,
            successful_playlist_sources: HashSet::new(),
            stale_playlist_on_device: false,
        }
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn plan(&self) -> &MirrorPlan {
        &self.plan
    }

    pub fn transfers(&self) -> &[TransferOperation] {
        &self.transfers
    }

    pub fn phase(&self) -> &PlannedSyncPhase {
        &self.phase
    }

    pub fn failed_tracks(&self) -> &[i64] {
        &self.failures
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    /// Applies one outcome and returns the work it unlocked.
    ///
    /// An event that does not match the effect in flight is ignored, so a
    /// duplicate or late answer cannot advance the run twice.
    pub fn dispatch(&mut self, event: Event) -> Vec<Effect> {
        match (self.awaiting, event) {
            (_, Event::Cancel) => {
                self.cancelled = true;
                Vec::new()
            }
            (_, Event::CopyProgress { copied }) => {
                self.observe_copy_progress(copied);
                Vec::new()
            }
            (Awaiting::Start, Event::Start) => {
                self.phase = self.opening_phase();
                self.awaiting = Awaiting::Partials;
                vec![Effect::CleanPartials]
            }
            (Awaiting::Partials, Event::PartialsCleaned(result)) => {
                if let Err(error) = result {
                    self.terminal_error =
                        Some(format!("could not clean partial sync files: {error}"));
                    return self.finish();
                }
                self.enter_transfers(0)
            }
            (Awaiting::Transcode(index), Event::Transcoded(result)) => match result {
                Ok(bytes) => {
                    self.transcoded_bytes = Some(bytes);
                    self.start_copy(index)
                }
                Err(_) => {
                    self.fail_transfer(index);
                    self.advance_past_transfer(index)
                }
            },
            (Awaiting::Copy(index), Event::TrackCopied(result)) => match result {
                Ok(device_size) => {
                    self.awaiting = Awaiting::RecordFile(index);
                    vec![Effect::RecordFile { index, device_size }]
                }
                Err(_) => {
                    // A copy that fails because the run was cancelled is not a
                    // failure of the track.
                    if !self.cancelled {
                        self.fail_transfer(index);
                    }
                    self.advance_past_transfer(index)
                }
            },
            (Awaiting::RecordFile(index), Event::FileRecorded(result)) => {
                match result {
                    Ok(()) => {
                        let operation = &self.transfers[index];
                        if let Some(previous) = &operation.previous {
                            if previous.device_path != operation.desired.device_path {
                                self.deferred_replacements.push((
                                    previous.device_path.clone(),
                                    operation.desired.track.id,
                                ));
                            }
                        }
                    }
                    Err(_) => self.fail_transfer(index),
                }
                self.advance_past_transfer(index)
            }
            (Awaiting::WritePlaylist(index), Event::PlaylistWritten(result)) => match result {
                Ok(()) => {
                    self.awaiting = Awaiting::RecordPlaylist(index);
                    vec![Effect::RecordPlaylist { index }]
                }
                Err(_) => {
                    self.fail_playlist();
                    self.enter_playlist_writes(index + 1)
                }
            },
            (Awaiting::RecordPlaylist(index), Event::PlaylistRecorded(result)) => {
                match result {
                    Ok(()) => {
                        let source = self.plan.playlist_writes[index].source.clone();
                        self.successful_playlist_sources.insert(source);
                    }
                    Err(_) => self.fail_playlist(),
                }
                self.enter_playlist_writes(index + 1)
            }
            (Awaiting::RemovePlaylist(index), Event::PlaylistRemoved(result)) => match result {
                Ok(()) => {
                    let source = &self.plan.playlist_removals[index].source;
                    if self.planned_playlist_sources.contains(source) {
                        self.enter_playlist_removals(index + 1)
                    } else {
                        self.awaiting = Awaiting::ForgetPlaylist(index);
                        vec![Effect::ForgetPlaylist { index }]
                    }
                }
                Err(_) => {
                    // The obsolete playlist is still on the device, and its
                    // entries may name files the removal stage would delete.
                    self.stale_playlist_on_device = true;
                    self.fail_playlist();
                    self.enter_playlist_removals(index + 1)
                }
            },
            (Awaiting::ForgetPlaylist(index), Event::PlaylistForgotten(result)) => {
                if result.is_err() {
                    self.fail_playlist();
                }
                self.enter_playlist_removals(index + 1)
            }
            (Awaiting::RemoveTrack(index), Event::TrackRemoved(result)) => match result {
                Ok(()) => match removal_track_id(&self.plan.remove[index]) {
                    Some(_) => {
                        self.awaiting = Awaiting::ForgetFile(index);
                        vec![Effect::ForgetFile { index }]
                    }
                    None => self.enter_removals(index + 1),
                },
                Err(_) => {
                    let track_id = removal_track_id(&self.plan.remove[index]).unwrap_or(-1);
                    self.fail_track(track_id);
                    self.enter_removals(index + 1)
                }
            },
            (Awaiting::ForgetFile(index), Event::FileForgotten(result)) => {
                if result.is_err() {
                    if let Some(track_id) = removal_track_id(&self.plan.remove[index]) {
                        self.fail_track(track_id);
                    }
                }
                self.enter_removals(index + 1)
            }
            (Awaiting::RemoveReplacedFile(index), Event::ReplacedFileRemoved(result)) => {
                if result.is_err() {
                    self.fail_track(self.deferred_replacements[index].1);
                }
                self.enter_deferred_removals(index + 1)
            }
            _ => Vec::new(),
        }
    }

    fn observe_copy_progress(&mut self, copied: u64) {
        let Awaiting::Copy(index) = self.awaiting else {
            return;
        };
        let estimated = self.transfers[index].desired.target_bytes;
        let total = self.plan.transfer_bytes;
        let done = self
            .completed_bytes
            .saturating_add(copied.min(estimated))
            .min(total);
        if let PlannedSyncPhase::Syncing {
            bytes_done,
            bytes_total,
            ..
        } = &mut self.phase
        {
            *bytes_done = (*bytes_done).max(done);
            *bytes_total = total;
        }
    }

    fn enter_transfers(&mut self, from: usize) -> Vec<Effect> {
        if self.cancelled {
            return self.finish();
        }
        let Some(operation) = self.transfers.get(from) else {
            return self.enter_playlists();
        };
        self.transcoded_bytes = None;
        match operation.desired.action {
            TransferAction::CopyOriginal => self.start_copy(from),
            action @ (TransferAction::TranscodeOpus160 | TransferAction::TranscodeMp3(_)) => {
                self.phase = self.syncing_phase(
                    SyncStep::Transcoding,
                    from,
                    self.transfers.len(),
                    self.transfer_activity(from),
                    self.completed_bytes,
                );
                self.awaiting = Awaiting::Transcode(from);
                vec![Effect::Transcode {
                    index: from,
                    action,
                }]
            }
        }
    }

    fn start_copy(&mut self, index: usize) -> Vec<Effect> {
        let (source, bytes) = match self.transcoded_bytes {
            Some(bytes) => (TransferSource::Transcoded, bytes),
            None => (
                TransferSource::Original,
                self.transfers[index].desired.target_bytes,
            ),
        };
        self.phase = self.syncing_phase(
            SyncStep::Copying,
            index,
            self.transfers.len(),
            self.transfer_activity(index),
            self.completed_bytes,
        );
        self.awaiting = Awaiting::Copy(index);
        vec![Effect::CopyTrack {
            index,
            source,
            bytes,
        }]
    }

    fn advance_past_transfer(&mut self, index: usize) -> Vec<Effect> {
        self.completed_bytes = self
            .completed_bytes
            .saturating_add(self.transfers[index].desired.target_bytes);
        self.enter_transfers(index + 1)
    }

    fn enter_playlists(&mut self) -> Vec<Effect> {
        if self.cancelled {
            return self.finish();
        }
        self.enter_playlist_writes(0)
    }

    /// Whether a planned playlist would point at a track that never arrived.
    ///
    /// This is the whole reason a failed transfer touches playlists at all: a
    /// published playlist must not reference a file that is not on the device.
    /// Playlists that reference nothing lost are unaffected.
    fn playlist_references_a_failed_transfer(&self, index: usize) -> bool {
        if self.failed_device_paths.is_empty() {
            return false;
        }
        self.plan.playlist_writes[index]
            .entries
            .iter()
            .any(|entry| self.failed_device_paths.contains(&entry.relative_path))
    }

    fn enter_playlist_writes(&mut self, from: usize) -> Vec<Effect> {
        if self.cancelled {
            return self.finish();
        }
        let Some(write) = self.plan.playlist_writes.get(from) else {
            return self.enter_playlist_removals(0);
        };
        if self.playlist_references_a_failed_transfer(from) {
            return self.enter_playlist_writes(from + 1);
        }
        self.phase = self.syncing_phase(
            SyncStep::WritingPlaylists,
            from,
            self.plan.playlist_writes.len(),
            write.source_name.clone(),
            self.plan.transfer_bytes,
        );
        self.awaiting = Awaiting::WritePlaylist(from);
        vec![Effect::WritePlaylist { index: from }]
    }

    fn enter_playlist_removals(&mut self, from: usize) -> Vec<Effect> {
        if self.cancelled {
            return self.finish();
        }
        for index in from..self.plan.playlist_removals.len() {
            let source = &self.plan.playlist_removals[index].source;
            // A playlist that is still mirrored, but whose write did not
            // succeed, must keep its stale file rather than lose it.
            if self.planned_playlist_sources.contains(source)
                && !self.successful_playlist_sources.contains(source)
            {
                continue;
            }
            self.awaiting = Awaiting::RemovePlaylist(index);
            return vec![Effect::RemovePlaylist { index }];
        }
        self.begin_removals()
    }

    /// The gate in front of the removal stage.
    ///
    /// A removal is safe only once no playlist that could name the file is
    /// still on the device in an outdated form. That covers two cases: a
    /// planned playlist that was not rewritten — because its write failed, or
    /// because it was held back for pointing at a track that never arrived —
    /// and an obsolete playlist whose deletion failed. The machine does not
    /// know any of their contents, so it holds every removal back rather than
    /// guess.
    ///
    /// A failed transfer that holds back no playlist therefore does not block
    /// the removals, which is where this differs from the blanket
    /// "any failure stops everything" rule it replaces.
    fn begin_removals(&mut self) -> Vec<Effect> {
        if self.cancelled {
            return self.finish();
        }
        let every_playlist_republished = self
            .planned_playlist_sources
            .iter()
            .all(|source| self.successful_playlist_sources.contains(source));
        if !every_playlist_republished || self.stale_playlist_on_device {
            return self.finish();
        }
        self.enter_removals(0)
    }

    fn enter_removals(&mut self, from: usize) -> Vec<Effect> {
        if self.cancelled {
            return self.finish();
        }
        let Some(removal) = self.plan.remove.get(from) else {
            return self.enter_deferred_removals(0);
        };
        self.phase = self.syncing_phase(
            SyncStep::Removing,
            from,
            self.plan.remove.len(),
            removal_path(removal),
            0,
        );
        self.awaiting = Awaiting::RemoveTrack(from);
        vec![Effect::RemoveTrack { index: from }]
    }

    fn enter_deferred_removals(&mut self, from: usize) -> Vec<Effect> {
        if self.cancelled {
            return self.finish();
        }
        let Some((device_path, _)) = self.deferred_replacements.get(from) else {
            return self.finish();
        };
        let device_path = device_path.clone();
        self.awaiting = Awaiting::RemoveReplacedFile(from);
        vec![Effect::RemoveReplacedFile { device_path }]
    }

    fn finish(&mut self) -> Vec<Effect> {
        self.awaiting = Awaiting::Done;
        self.failures.sort_unstable();
        self.failures.dedup();
        self.phase = PlannedSyncPhase::Finishing;

        if self.terminal_error.is_none() && self.failures.is_empty() && !self.cancelled {
            let verified_sources = self
                .plan
                .playlist_writes
                .iter()
                .map(|write| write.source.clone())
                .collect();
            return vec![Effect::Finished(SyncOutcome::Completed {
                verified_sources,
            })];
        }

        self.phase = PlannedSyncPhase::Idle;
        if self.cancelled {
            return vec![Effect::Finished(SyncOutcome::Cancelled)];
        }
        vec![Effect::Finished(SyncOutcome::Failed {
            terminal_error: self.terminal_error.clone(),
            failed_tracks: self.failures.clone(),
        })]
    }

    /// The phase a run shows before its first step reports anything.
    ///
    /// Partial cleanup runs first but has no step of its own, so the run opens
    /// on whichever step will actually do the first visible work. Naming a
    /// step that runs later — as this did with `Removing` — tells the user the
    /// run is doing something it has not started.
    fn opening_phase(&self) -> PlannedSyncPhase {
        if !self.transfers.is_empty() {
            let step = match self.transfers[0].desired.action {
                TransferAction::CopyOriginal => SyncStep::Copying,
                TransferAction::TranscodeOpus160 | TransferAction::TranscodeMp3(_) => {
                    SyncStep::Transcoding
                }
            };
            return self.syncing_phase(step, 0, self.transfers.len(), self.transfer_activity(0), 0);
        }
        if let Some(write) = self.plan.playlist_writes.first() {
            return self.syncing_phase(
                SyncStep::WritingPlaylists,
                0,
                self.plan.playlist_writes.len(),
                write.source_name.clone(),
                0,
            );
        }
        self.syncing_phase(
            SyncStep::Removing,
            0,
            self.plan.remove.len(),
            self.plan
                .remove
                .first()
                .map(removal_path)
                .unwrap_or_default(),
            0,
        )
    }

    fn fail_track(&mut self, track_id: i64) {
        self.failures.push(track_id);
    }

    /// Records a failed transfer under both the track that was lost and the
    /// device path that will therefore not exist.
    fn fail_transfer(&mut self, index: usize) {
        let desired = &self.transfers[index].desired;
        self.failures.push(desired.track.id);
        self.failed_device_paths.insert(desired.device_path.clone());
    }

    /// A playlist failure has no track to blame, so it is recorded under the
    /// same sentinel the GTK runtime used.
    fn fail_playlist(&mut self) {
        self.failures.push(-1);
    }

    fn transfer_activity(&self, index: usize) -> String {
        let track = &self.transfers[index].desired.track;
        track_activity(&track.title, &track.artist)
    }

    fn syncing_phase(
        &self,
        step: SyncStep,
        done: usize,
        total: usize,
        current_track: String,
        bytes_done: u64,
    ) -> PlannedSyncPhase {
        let bytes_total = self.plan.transfer_bytes;
        PlannedSyncPhase::Syncing {
            step,
            done: u32::try_from(done).unwrap_or(u32::MAX),
            total: u32::try_from(total).unwrap_or(u32::MAX),
            current_track,
            bytes_done: bytes_done.min(bytes_total),
            bytes_total,
        }
    }
}

/// Copies and replacements form one queue ordered by track id, so a run walks
/// the device in a stable order whatever the plan's internal grouping was.
fn transfer_operations(plan: &MirrorPlan) -> Vec<TransferOperation> {
    let mut operations = plan
        .copy
        .iter()
        .cloned()
        .map(|desired| TransferOperation {
            desired,
            previous: None,
        })
        .chain(
            plan.replace
                .iter()
                .cloned()
                .map(|replacement| TransferOperation {
                    desired: replacement.desired,
                    previous: Some(replacement.existing),
                }),
        )
        .collect::<Vec<_>>();
    operations.sort_by_key(|operation| operation.desired.track.id);
    operations
}

fn removal_path(removal: &ManagedRemoval) -> String {
    match removal {
        ManagedRemoval::Inventory(file) => file.device_path.clone(),
        ManagedRemoval::Orphan(file) => file.relative_path.clone(),
    }
}

fn removal_track_id(removal: &ManagedRemoval) -> Option<i64> {
    match removal {
        ManagedRemoval::Inventory(file) => Some(file.track_id),
        ManagedRemoval::Orphan(_) => None,
    }
}

fn track_activity(title: &str, artist: &str) -> String {
    let artist = artist.trim();
    if artist.is_empty() {
        title.to_string()
    } else {
        format!("{title} — {artist}")
    }
}
