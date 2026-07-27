//! Pure DB-diff calculation for a connected device.

use std::collections::{HashMap, HashSet};

use super::settings::DeviceFileRecord;

const ESTIMATED_USB_BYTES_PER_SECOND: u64 = 5 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncCandidate {
    pub track_id: i64,
    pub device_path: String,
    pub transfer_bytes: u64,
    pub source_mtime: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncDelta {
    pub to_copy: Vec<i64>,
    pub to_remove: Vec<i64>,
    pub bytes: u64,
    pub est_secs: u32,
}

impl SyncDelta {
    pub fn add_transfer_bytes(&mut self, bytes: u64) {
        self.bytes = self.bytes.saturating_add(bytes);
        self.est_secs = estimated_seconds(self.bytes);
    }
}

pub fn compute_delta(
    selected: &[SyncCandidate],
    files: &[DeviceFileRecord],
    remove_deleted: bool,
) -> SyncDelta {
    let existing = files
        .iter()
        .map(|file| (file.track_id, file))
        .collect::<HashMap<_, _>>();
    let mut seen = HashSet::new();
    let mut to_copy = Vec::new();
    let mut bytes = 0_u64;
    for candidate in selected {
        if !seen.insert(candidate.track_id) {
            continue;
        }
        let current = existing.get(&candidate.track_id);
        let unchanged = current.is_some_and(|file| {
            file.device_path == candidate.device_path
                && file.mtime == candidate.source_mtime
                && file.size == candidate.transfer_bytes
        });
        if !unchanged {
            to_copy.push(candidate.track_id);
            bytes = bytes.saturating_add(candidate.transfer_bytes);
        }
    }

    let selected_ids = selected
        .iter()
        .map(|candidate| candidate.track_id)
        .collect::<HashSet<_>>();
    let to_remove = if remove_deleted {
        files
            .iter()
            .filter(|file| !file.pinned && !selected_ids.contains(&file.track_id))
            .map(|file| file.track_id)
            .collect()
    } else {
        Vec::new()
    };
    let est_secs = estimated_seconds(bytes);
    SyncDelta {
        to_copy,
        to_remove,
        bytes,
        est_secs,
    }
}

fn estimated_seconds(bytes: u64) -> u32 {
    if bytes == 0 {
        0
    } else {
        bytes
            .div_ceil(ESTIMATED_USB_BYTES_PER_SECOND)
            .min(u64::from(u32::MAX)) as u32
    }
}
