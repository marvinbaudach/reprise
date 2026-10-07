//! CUE files in the mirror plan (CUE-15).
//!
//! A file a CUE sheet cut into tracks reaches the device once. Every selected
//! track of it is planned on its own row, so the inventory still says per
//! track what is on the device, but all of them share the file's device path
//! and its bytes are counted once. One of them carries the transfer; the
//! others are recorded beside it (`SharedRecord`), after the copy or against
//! the file already on the device. A track that leaves the selection while
//! another still needs the file only loses its row. Beside the file sits a
//! sheet derived for the device (`DerivedCueWrite`), which the orphan pass
//! removes once the file is gone.

use std::collections::{HashMap, HashSet};

use super::{DesiredManagedFile, DeviceFileRecord, ManagedDeviceFile, MirrorPlan, SyncTrack};
use crate::device_sync::cue_files::CueSyncFile;
use crate::device_sync::derived_cue;

/// A selected track of a CUE file whose row shares another track's transfer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharedRecord {
    pub desired: DesiredManagedFile,
    /// The track whose transfer brings the file, or whose row already says the
    /// file is on the device.
    pub carrier_track_id: i64,
    /// The file's size on the device when it is already there; `None` when the
    /// carrier's transfer in this run decides it.
    pub resident_size: Option<u64>,
    /// The row this one replaces, if the track had one.
    pub previous: Option<DeviceFileRecord>,
}

/// The sheet derived for a CUE file on the device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DerivedCueWrite {
    pub track_id: i64,
    /// Where the sheet goes, beside its audio.
    pub device_path: String,
    /// The audio file the sheet describes.
    pub audio_device_path: String,
    pub contents: String,
    pub size_bytes: u64,
    pub existing_size_bytes: Option<u64>,
}

/// The CUE files of a selection, by track.
#[derive(Default)]
pub(super) struct CueGroups {
    files: Vec<CueSyncFile>,
    by_track: HashMap<i64, usize>,
}

impl CueGroups {
    pub(super) fn new<'a>(files: impl IntoIterator<Item = &'a CueSyncFile>) -> Self {
        let mut groups = Self::default();
        for file in files {
            if groups
                .files
                .iter()
                .any(|known| known.source_path == file.source_path)
            {
                continue;
            }
            let position = groups.files.len();
            for id in file.track_ids() {
                groups.by_track.insert(id, position);
            }
            groups.files.push(file.clone());
        }
        groups
    }

    /// The file a track belongs to, by track id, for the transfer plan.
    pub(super) fn by_track(&self) -> HashMap<i64, &CueSyncFile> {
        self.by_track
            .iter()
            .map(|(id, position)| (*id, &self.files[*position]))
            .collect()
    }

    /// Which file a track belongs to, as a key shared by its siblings.
    pub(super) fn group_of(&self, track_id: i64) -> Option<usize> {
        self.by_track.get(&track_id).copied()
    }

    pub(super) fn contains(&self, track_id: i64) -> bool {
        self.by_track.contains_key(&track_id)
    }

    /// The track a transfer of the whole file carries: the file's length, and
    /// the album as its title, since the transcode tags the whole file.
    pub(super) fn carrier_track(&self, track: &SyncTrack) -> SyncTrack {
        let Some(file) = self
            .group_of(track.id)
            .map(|position| &self.files[position])
        else {
            return track.clone();
        };
        let title = if file.album.trim().is_empty() {
            track
                .source_path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default()
        } else {
            file.album.clone()
        };
        SyncTrack {
            title,
            track_number: None,
            duration_ms: file.duration_ms,
            ..track.clone()
        }
    }

    /// Plans the derived sheet beside every CUE file this run keeps on the
    /// device, where it is missing or differs in size.
    pub(super) fn plan_cue_writes(
        &self,
        managed_files: &[ManagedDeviceFile],
        plan: &mut MirrorPlan,
    ) {
        let resident: HashMap<&str, u64> = managed_files
            .iter()
            .map(|file| (file.relative_path.as_str(), file.size_bytes))
            .collect();
        let mut planned = HashSet::new();
        let mut target_bytes = 0_u64;
        for desired in &plan.desired_files {
            let Some(position) = self.group_of(desired.track.id) else {
                continue;
            };
            if !planned.insert(desired.device_path.clone()) {
                continue;
            }
            let contents = derived_cue::render(&self.files[position], &desired.device_path);
            let size_bytes = contents.len() as u64;
            target_bytes = target_bytes.saturating_add(size_bytes);
            let device_path = derived_cue::sheet_path(&desired.device_path);
            let existing_size_bytes = resident.get(device_path.as_str()).copied();
            if existing_size_bytes == Some(size_bytes) {
                continue;
            }
            plan.cue_writes.push(DerivedCueWrite {
                track_id: desired.track.id,
                device_path,
                audio_device_path: desired.device_path.clone(),
                contents,
                size_bytes,
                existing_size_bytes,
            });
        }
        let written = plan
            .cue_writes
            .iter()
            .map(|write| write.size_bytes)
            .fold(0_u64, u64::saturating_add);
        plan.transfer_bytes = plan.transfer_bytes.saturating_add(written);
        plan.target_bytes = plan.target_bytes.saturating_add(target_bytes);
    }

    /// The derived sheets this plan keeps: beside every CUE file it wants or
    /// retains, so the orphan pass removes a sheet only with its file.
    pub(super) fn kept_sheet_paths(&self, plan: &MirrorPlan) -> Vec<String> {
        let wanted = plan
            .desired_files
            .iter()
            .filter(|file| self.contains(file.track.id))
            .map(|file| file.device_path.as_str());
        let retained = plan
            .retained_unavailable
            .iter()
            .chain(&plan.retained_stable)
            .filter(|file| self.contains(file.track_id))
            .map(|file| file.device_path.as_str());
        wanted
            .chain(retained)
            .map(derived_cue::sheet_path)
            .collect()
    }
}
