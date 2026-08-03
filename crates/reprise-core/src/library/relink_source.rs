//! Source-query helpers shared by single-file and folder relinking.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::library::scanner::ScanError;
use crate::library::source::{
    self, LibraryLinkMode, LibraryPathPresence, LibrarySource, LibraryWalkControl, LibraryWalkItem,
    LibraryWalkOrder,
};

pub(super) struct FileFacts {
    pub(super) mtime: i64,
    pub(super) size: u64,
    pub(super) identity: Option<(u64, u64)>,
}

impl FileFacts {
    pub(super) fn from_metadata(
        metadata: &crate::library::source::LibraryPathMetadata,
    ) -> Option<Self> {
        if !metadata.is_file {
            return None;
        }
        Some(Self {
            mtime: metadata
                .modified
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |duration| duration.as_secs() as i64),
            size: metadata.size?,
            identity: metadata.identity,
        })
    }
}

pub(super) fn file_facts(source: &dyn LibrarySource, path: &Path) -> Option<FileFacts> {
    match source.probe(path, LibraryLinkMode::Follow) {
        LibraryPathPresence::Present(metadata) => FileFacts::from_metadata(&metadata),
        LibraryPathPresence::Absent | LibraryPathPresence::Unknown => None,
    }
}

pub(super) fn count_folder_audio_files(
    source: &dyn LibrarySource,
    folder: &Path,
    cancel: &AtomicBool,
) -> Result<Option<u32>, ScanError> {
    if cancel.load(Ordering::Acquire) {
        return Ok(None);
    }
    let mut total = 0_u32;
    let mut outcome: Result<Option<()>, ScanError> = Ok(Some(()));
    source::walk_with(source, folder, LibraryWalkOrder::Native, |item| {
        if cancel.load(Ordering::Acquire) {
            outcome = Ok(None);
            return LibraryWalkControl::Stop;
        }
        match item {
            LibraryWalkItem::Entry(entry) => {
                if entry.is_file && crate::library::scanner::is_audio_file(&entry.path) {
                    total = total.saturating_add(1);
                }
                LibraryWalkControl::Continue
            }
            LibraryWalkItem::Error(error) => {
                outcome = Err(std::io::Error::other(error.detail).into());
                LibraryWalkControl::Stop
            }
        }
    });
    match outcome? {
        Some(()) => Ok(Some(total)),
        None => Ok(None),
    }
}
