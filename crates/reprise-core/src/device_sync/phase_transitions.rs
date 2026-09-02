//! Construction of externally visible synchronization phases.

use super::ledger::WorkLedger;
use super::machine::{PlannedSyncPhase, SyncStep, TransferOperation};
use super::{DevicePlaylistRecord, ManagedRemoval, TransferAction};

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
        return syncing(
            ledger,
            SyncStep::Removing,
            playlist_removal_activity(removal),
        );
    }
    if let Some(removal) = plan.remove.first() {
        return syncing(ledger, SyncStep::Removing, removal_activity(removal));
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

/// The reverse of `sanitize::device_track_path` — good enough to name a file
/// this crate itself wrote, and honest about the paths it did not.
pub(super) fn removal_activity(removal: &ManagedRemoval) -> String {
    let path = match removal {
        ManagedRemoval::Inventory(file) => &file.device_path,
        ManagedRemoval::Orphan(file) => &file.relative_path,
    };
    removal_name(path)
}

pub(super) fn removal_name(path: &str) -> String {
    let components = path.split('/').collect::<Vec<_>>();
    if components.len() < 2 {
        return path.to_owned();
    }
    let Some(file_name) = components.last() else {
        return path.to_owned();
    };
    let Some((stem, _)) = file_name.rsplit_once('.') else {
        return path.to_owned();
    };
    let without_collision = strip_collision_suffix(stem);
    let title = strip_track_number(without_collision);
    if title.is_empty() || title == without_collision {
        return path.to_owned();
    }
    match components.first().filter(|_| components.len() >= 3) {
        Some(artist) if !artist.is_empty() => format!("{title} — {artist}"),
        _ => title.to_owned(),
    }
}

fn strip_track_number(value: &str) -> &str {
    let Some((number, title)) = value.split_once(' ') else {
        return value;
    };
    if number.len() >= 2 && number.bytes().all(|byte| byte.is_ascii_digit()) {
        title
    } else {
        value
    }
}

fn strip_collision_suffix(value: &str) -> &str {
    let Some((title, suffix)) = value.rsplit_once(" (") else {
        return value;
    };
    let Some(index) = suffix.strip_suffix(')') else {
        return value;
    };
    if !index.is_empty() && index.bytes().all(|byte| byte.is_ascii_digit()) {
        title
    } else {
        value
    }
}

/// A playlist is named by the source the user picked, never by its file.
pub(super) fn playlist_removal_activity(record: &DevicePlaylistRecord) -> String {
    if record.source_name.is_empty() {
        record.device_path.clone()
    } else {
        record.source_name.clone()
    }
}

pub(super) fn removal_track_id(removal: &ManagedRemoval) -> Option<i64> {
    match removal {
        ManagedRemoval::Inventory(file) => Some(file.track_id),
        ManagedRemoval::Orphan(_) => None,
    }
}
