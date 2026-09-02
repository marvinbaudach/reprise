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

use super::ledger::WorkLedger;
use super::phase_transitions;
use super::{
    AnalysisSidecarWrite, DesiredManagedFile, DeviceFileRecord, MirrorPlan, SelectionSource,
    TransferAction,
};

#[path = "machine_sidecars.rs"]
mod sidecars;

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
    WritingPlaylists,
    WritingTrackMetadata,
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
/// [`DeviceSyncMachine::transfers`] for transfers, `plan.playlist_writes`,
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
    /// Carries the number of bytes actually written to the device.
    TrackCopied(Result<u64, String>),
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

/// The effect the machine is currently waiting to hear back about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Awaiting {
    Start,
    Partials,
    Transcode(usize),
    Copy(usize),
    RecordFile(usize),
    WriteAnalysis(usize),
    WriteLyrics(usize),
    WritePlaylist(usize),
    RecordPlaylist(usize),
    RemovePlaylist(usize),
    ForgetPlaylist(usize),
    RemoveTrack(usize),
    ForgetFile(usize),
    RemoveReplacedFile(usize),
    WriteTrackMetadataList,
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
    /// Device paths absent because their transfer never published a file. A
    /// playlist that would point at one of them must omit that entry.
    absent_device_paths: HashSet<String>,
    ledger: WorkLedger,
    writes_track_metadata_list: bool,
    copied_bytes: Option<u64>,
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
        let ledger = sidecars::work_ledger(&plan, false);
        Self {
            device_id,
            plan,
            transfers,
            awaiting: Awaiting::Start,
            phase: PlannedSyncPhase::Idle,
            cancelled: false,
            terminal_error: None,
            failures: Vec::new(),
            absent_device_paths: HashSet::new(),
            ledger,
            writes_track_metadata_list: false,
            copied_bytes: None,
            transcoded_bytes: None,
            deferred_replacements: Vec::new(),
            planned_playlist_sources,
            successful_playlist_sources: HashSet::new(),
            stale_playlist_on_device: false,
        }
    }

    pub fn with_track_metadata_list(mut self) -> Self {
        self.writes_track_metadata_list = true;
        self.ledger = sidecars::work_ledger(&self.plan, true);
        self
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

    pub fn bytes_done(&self) -> u64 {
        self.ledger.bytes_done()
    }

    pub fn bytes_total(&self) -> u64 {
        self.ledger.bytes_total()
    }

    pub fn units_done(&self) -> u32 {
        self.ledger.done()
    }

    pub fn units_total(&self) -> u32 {
        self.ledger.total()
    }

    pub fn verified_sources(&self) -> Vec<SelectionSource> {
        let mut sources = self
            .successful_playlist_sources
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        sources.sort();
        sources
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
                vec![Effect::CleanPartials(self.plan.partial_paths.clone())]
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
                    self.fail_unpublished_transfer(index);
                    self.ledger.complete_unit(0);
                    self.advance_past_transfer(index)
                }
            },
            (Awaiting::Copy(index), Event::TrackCopied(result)) => match result {
                Ok(device_size) => {
                    self.copied_bytes = Some(device_size);
                    self.awaiting = Awaiting::RecordFile(index);
                    vec![Effect::RecordFile { index, device_size }]
                }
                Err(_) => {
                    // A copy that fails because the run was cancelled is not a
                    // failure of the track.
                    if !self.cancelled {
                        self.fail_unpublished_transfer(index);
                    }
                    self.ledger.complete_unit(0);
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
                self.ledger
                    .complete_unit(self.copied_bytes.take().unwrap_or_default());
                self.advance_past_transfer(index)
            }
            (Awaiting::WriteAnalysis(index), Event::AnalysisWritten(result)) => {
                match result {
                    Ok(bytes) => self.ledger.complete_unit(bytes),
                    Err(error) => {
                        self.terminal_error = Some(error);
                        self.ledger.complete_unit(0);
                    }
                }
                self.enter_analysis_writes(index + 1)
            }
            (Awaiting::WriteLyrics(index), Event::LyricsWritten(result)) => {
                match result {
                    Ok(bytes) => self.ledger.complete_unit(bytes),
                    Err(error) => {
                        self.terminal_error = Some(error);
                        self.ledger.complete_unit(0);
                    }
                }
                self.enter_lyrics_writes(index + 1)
            }
            (Awaiting::WritePlaylist(index), Event::PlaylistWritten(result)) => match result {
                Ok(()) => {
                    self.awaiting = Awaiting::RecordPlaylist(index);
                    vec![Effect::RecordPlaylist { index }]
                }
                Err(_) => {
                    self.fail_playlist();
                    self.ledger.complete_unit(0);
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
                self.ledger.complete_unit(0);
                self.enter_playlist_writes(index + 1)
            }
            (Awaiting::RemovePlaylist(index), Event::PlaylistRemoved(result)) => match result {
                Ok(()) => {
                    let source = &self.plan.playlist_removals[index].source;
                    if self.planned_playlist_sources.contains(source) {
                        self.ledger.complete_unit(0);
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
                    self.ledger.complete_unit(0);
                    self.enter_playlist_removals(index + 1)
                }
            },
            (Awaiting::ForgetPlaylist(index), Event::PlaylistForgotten(result)) => {
                if result.is_err() {
                    self.fail_playlist();
                }
                self.ledger.complete_unit(0);
                self.enter_playlist_removals(index + 1)
            }
            (Awaiting::RemoveTrack(index), Event::TrackRemoved(result)) => match result {
                Ok(()) => match phase_transitions::removal_track_id(&self.plan.remove[index]) {
                    Some(_) => {
                        self.awaiting = Awaiting::ForgetFile(index);
                        vec![Effect::ForgetFile { index }]
                    }
                    None => {
                        self.ledger.complete_unit(0);
                        self.enter_removals(index + 1)
                    }
                },
                Err(_) => {
                    let track_id =
                        phase_transitions::removal_track_id(&self.plan.remove[index]).unwrap_or(-1);
                    self.fail_track(track_id);
                    self.ledger.complete_unit(0);
                    self.enter_removals(index + 1)
                }
            },
            (Awaiting::ForgetFile(index), Event::FileForgotten(result)) => {
                if result.is_err() {
                    if let Some(track_id) =
                        phase_transitions::removal_track_id(&self.plan.remove[index])
                    {
                        self.fail_track(track_id);
                    }
                }
                self.ledger.complete_unit(0);
                self.enter_removals(index + 1)
            }
            (Awaiting::RemoveReplacedFile(index), Event::ReplacedFileRemoved(result)) => {
                if result.is_err() {
                    self.fail_track(self.deferred_replacements[index].1);
                }
                self.enter_deferred_removals(index + 1)
            }
            (Awaiting::WriteTrackMetadataList, Event::TrackMetadataListWritten(result)) => {
                if let Err(error) = result {
                    self.terminal_error = Some(error);
                }
                self.ledger.complete_unit(0);
                self.finish()
            }
            _ => Vec::new(),
        }
    }

    fn observe_copy_progress(&mut self, copied: u64) {
        if !matches!(
            self.awaiting,
            Awaiting::Copy(_) | Awaiting::WriteAnalysis(_) | Awaiting::WriteLyrics(_)
        ) {
            return;
        }
        self.ledger.observe_unit_bytes(copied);
        if let PlannedSyncPhase::Syncing {
            unit_bytes_done,
            unit_bytes_total,
            ..
        } = &mut self.phase
        {
            *unit_bytes_done = self.ledger.unit_bytes_done();
            *unit_bytes_total = self.ledger.unit_bytes_total();
        }
    }

    fn enter_transfers(&mut self, from: usize) -> Vec<Effect> {
        if self.cancelled {
            return self.finish();
        }
        let Some(operation) = self.transfers.get(from) else {
            return self.enter_analysis_writes(0);
        };
        self.transcoded_bytes = None;
        self.copied_bytes = None;
        match operation.desired.action {
            TransferAction::CopyOriginal => self.start_copy(from),
            action @ (TransferAction::TranscodeOpus160 | TransferAction::TranscodeMp3(_)) => {
                self.ledger.begin_unit(0);
                self.phase = phase_transitions::syncing(
                    &self.ledger,
                    SyncStep::Transcoding,
                    phase_transitions::transfer_activity(&self.transfers[from]),
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
        self.ledger.begin_unit(bytes);
        self.phase = phase_transitions::syncing(
            &self.ledger,
            SyncStep::Copying,
            phase_transitions::transfer_activity(&self.transfers[index]),
        );
        self.awaiting = Awaiting::Copy(index);
        vec![Effect::CopyTrack {
            index,
            source,
            bytes,
        }]
    }

    fn advance_past_transfer(&mut self, index: usize) -> Vec<Effect> {
        self.enter_transfers(index + 1)
    }

    fn enter_playlists(&mut self) -> Vec<Effect> {
        if self.cancelled {
            return self.finish();
        }
        self.enter_playlist_writes(0)
    }

    /// Paths a planned playlist must omit because their tracks never arrived.
    ///
    /// This is the whole reason a failed transfer touches playlists at all: a
    /// published playlist must not reference a file that is not on the device.
    /// Playlists that reference nothing lost receive an empty set and keep
    /// their pre-rendered contents byte-identical.
    fn failed_paths_in_playlist(&self, index: usize) -> Vec<String> {
        self.plan.playlist_writes[index]
            .entries
            .iter()
            .filter(|entry| self.absent_device_paths.contains(&entry.relative_path))
            .map(|entry| entry.relative_path.clone())
            .collect()
    }

    fn enter_playlist_writes(&mut self, from: usize) -> Vec<Effect> {
        if self.cancelled {
            return self.finish();
        }
        let Some(write) = self.plan.playlist_writes.get(from) else {
            return self.enter_playlist_removals(0);
        };
        let omit_relative_paths = self.failed_paths_in_playlist(from);
        self.ledger.begin_unit(0);
        self.phase = phase_transitions::syncing(
            &self.ledger,
            SyncStep::WritingPlaylists,
            write.source_name.clone(),
        );
        self.awaiting = Awaiting::WritePlaylist(from);
        vec![Effect::WritePlaylist {
            index: from,
            omit_relative_paths,
        }]
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
                self.ledger.complete_unit(0);
                continue;
            }
            self.ledger.begin_unit(0);
            self.phase = phase_transitions::syncing(
                &self.ledger,
                SyncStep::Removing,
                phase_transitions::playlist_removal_activity(&self.plan.playlist_removals[index]),
            );
            self.awaiting = Awaiting::RemovePlaylist(index);
            return vec![Effect::RemovePlaylist { index }];
        }
        self.begin_removals()
    }

    /// The gate in front of the removal stage.
    ///
    /// A removal is safe only once no playlist that could name the file is
    /// still on the device in an outdated form. That covers two cases: a
    /// planned playlist whose write failed, and an obsolete playlist whose
    /// deletion failed. The machine does not know any of their contents, so it
    /// holds every removal back rather than guess. A playlist covering a lost
    /// track is republished without that entry and therefore opens the gate.
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
        self.ledger.begin_unit(0);
        self.phase = phase_transitions::syncing(
            &self.ledger,
            SyncStep::Removing,
            phase_transitions::removal_activity(removal),
        );
        self.awaiting = Awaiting::RemoveTrack(from);
        vec![Effect::RemoveTrack { index: from }]
    }

    fn enter_deferred_removals(&mut self, from: usize) -> Vec<Effect> {
        if self.cancelled {
            return self.finish();
        }
        let Some((device_path, _)) = self.deferred_replacements.get(from) else {
            return self.enter_track_metadata_list();
        };
        let device_path = device_path.clone();
        // The replacement already owns this counted unit; deletion adds none.
        let activity = phase_transitions::removal_name(&device_path);
        self.phase = phase_transitions::syncing(&self.ledger, SyncStep::Removing, activity);
        self.awaiting = Awaiting::RemoveReplacedFile(from);
        vec![Effect::RemoveReplacedFile { device_path }]
    }

    fn enter_track_metadata_list(&mut self) -> Vec<Effect> {
        if !self.writes_track_metadata_list {
            return self.finish();
        }
        self.ledger.begin_unit(0);
        self.phase = phase_transitions::syncing(
            &self.ledger,
            SyncStep::WritingTrackMetadata,
            super::track_metadata_list::FILE_NAME.to_owned(),
        );
        self.awaiting = Awaiting::WriteTrackMetadataList;
        vec![Effect::WriteTrackMetadataList]
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
            verified_sources: self.verified_sources(),
        })]
    }

    fn fail_track(&mut self, track_id: i64) {
        self.failures.push(track_id);
    }

    fn fail_transfer(&mut self, index: usize) {
        self.failures.push(self.transfers[index].desired.track.id);
    }

    fn fail_unpublished_transfer(&mut self, index: usize) {
        let device_path = self.transfers[index].desired.device_path.clone();
        self.fail_transfer(index);
        self.absent_device_paths.insert(device_path);
    }

    /// A playlist failure has no track to blame, so it is recorded under the
    /// same sentinel the GTK runtime used.
    fn fail_playlist(&mut self) {
        self.failures.push(-1);
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
