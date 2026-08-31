//! Construction of externally visible synchronization phases.

use super::ledger::WorkLedger;
use super::machine::{PlannedSyncPhase, SyncStep, TransferOperation};
use super::{ManagedRemoval, TransferAction};

pub(super) fn syncing(
    ledger: &WorkLedger,
    step: SyncStep,
    current_track: String,
) -> PlannedSyncPhase {
    PlannedSyncPhase::Syncing {
        step,
        done: ledger.done(),
        total: ledger.total(),
        current_track,
        unit_bytes_done: ledger.unit_bytes_done(),
        unit_bytes_total: ledger.unit_bytes_total(),
    }
}

pub(super) fn opening(
    transfers: &[TransferOperation],
    plan: &super::MirrorPlan,
    ledger: &WorkLedger,
    writes_track_metadata_list: bool,
) -> PlannedSyncPhase {
    if let Some(operation) = transfers.first() {
        let step = match operation.desired.action {
            TransferAction::CopyOriginal => SyncStep::Copying,
            TransferAction::TranscodeOpus160 | TransferAction::TranscodeMp3(_) => {
                SyncStep::Transcoding
            }
        };
        let mut opening = ledger.clone();
        let unit_bytes = matches!(operation.desired.action, TransferAction::CopyOriginal)
            .then_some(operation.desired.target_bytes)
            .unwrap_or(0);
        opening.begin_unit(unit_bytes);
        return syncing(&opening, step, transfer_activity(operation));
    }
    if let Some(write) = plan.analysis_writes.first() {
        let mut opening = ledger.clone();
        opening.begin_unit(write.size_bytes);
        return syncing(
            &opening,
            SyncStep::WritingAnalysis,
            write.device_path.clone(),
        );
    }
    if let Some(write) = plan.playlist_writes.first() {
        return syncing(
            ledger,
            SyncStep::WritingPlaylists,
            write.source_name.clone(),
        );
    }
    if let Some(removal) = plan.playlist_removals.first() {
        return syncing(ledger, SyncStep::Removing, removal.device_path.clone());
    }
    if let Some(removal) = plan.remove.first() {
        return syncing(ledger, SyncStep::Removing, removal_path(removal));
    }
    if writes_track_metadata_list {
        return syncing(
            ledger,
            SyncStep::WritingTrackMetadata,
            super::track_metadata_list::FILE_NAME.to_owned(),
        );
    }
    syncing(ledger, SyncStep::Removing, String::new())
}

pub(super) fn transfer_activity(operation: &TransferOperation) -> String {
    let track = &operation.desired.track;
    let artist = track.artist.trim();
    if artist.is_empty() {
        track.title.clone()
    } else {
        format!("{} — {artist}", track.title)
    }
}

pub(super) fn removal_path(removal: &ManagedRemoval) -> String {
    match removal {
        ManagedRemoval::Inventory(file) => file.device_path.clone(),
        ManagedRemoval::Orphan(file) => file.relative_path.clone(),
    }
}

pub(super) fn removal_track_id(removal: &ManagedRemoval) -> Option<i64> {
    match removal {
        ManagedRemoval::Inventory(file) => Some(file.track_id),
        ManagedRemoval::Orphan(_) => None,
    }
}
