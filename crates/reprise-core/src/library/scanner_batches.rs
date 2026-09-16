//! Batch execution for scanner walks. Tag reads happen between the two writer
//! leases so a slow source never owns the database writer for the whole walk.

use std::path::{Path, PathBuf};

use super::entry::{self, EntryOutcome, EntryPlan};
use super::source::{
    self, LibraryEntry, LibrarySource, LibraryWalkControl, LibraryWalkError, LibraryWalkItem,
    LibraryWalkOrder,
};
use super::{
    mobile_sync, mount, record_walk_error, BatchProgress, LeaseMetrics, ScanError, ScanWriter,
    WalkState,
};

pub(super) const SCAN_LEASE_ITEMS: usize = 16;

struct PreparedItem {
    observed: Option<(PathBuf, bool)>,
    action: PreparedAction,
}

enum PreparedAction {
    Error(LibraryWalkError),
    Skip(EntryOutcome),
    Import {
        plan: EntryPlan,
        meta: Box<Option<Result<super::track_meta::MetaOutcome, ScanError>>>,
    },
}

// These independent scanner services stay explicit so their mutable lease-bound lifetimes remain visible.
#[allow(clippy::too_many_arguments)]
pub(super) fn walk_root_in_batches<'source>(
    source: &'source dyn LibrarySource,
    writer: &dyn ScanWriter,
    root: &Path,
    state: &mut WalkState,
    mobile_sync: &mut mobile_sync::MobileSyncDiscovery,
    mount_cache: &mut mount::MountPointCache<'source>,
    progress: &mut BatchProgress<'_>,
    leases: &mut LeaseMetrics,
) -> Result<(), ScanError> {
    let mut buffered = Vec::with_capacity(SCAN_LEASE_ITEMS);
    let mut walk_failure = None;
    source::walk_with(source, root, LibraryWalkOrder::Native, |item| {
        buffered.push(item);
        if buffered.len() < SCAN_LEASE_ITEMS {
            return LibraryWalkControl::Continue;
        }
        match process_batch(
            std::mem::take(&mut buffered),
            source,
            writer,
            root,
            state,
            mobile_sync,
            mount_cache,
            progress,
            leases,
        ) {
            Ok(()) => LibraryWalkControl::Continue,
            Err(error) => {
                walk_failure = Some(error);
                LibraryWalkControl::Stop
            }
        }
    });
    if let Some(error) = walk_failure {
        return Err(error);
    }
    if !buffered.is_empty() {
        process_batch(
            buffered,
            source,
            writer,
            root,
            state,
            mobile_sync,
            mount_cache,
            progress,
            leases,
        )?;
    }
    Ok(())
}

// These independent scanner services stay explicit so their mutable lease-bound lifetimes remain visible.
#[allow(clippy::too_many_arguments)]
fn process_batch<'source>(
    mut items: Vec<LibraryWalkItem>,
    source: &'source dyn LibrarySource,
    writer: &dyn ScanWriter,
    root: &Path,
    state: &mut WalkState,
    mobile_sync: &mut mobile_sync::MobileSyncDiscovery,
    mount_cache: &mut mount::MountPointCache<'source>,
    progress: &mut BatchProgress<'_>,
    leases: &mut LeaseMetrics,
) -> Result<(), ScanError> {
    let mut prepared = Vec::with_capacity(items.len());
    leases.run(writer, &mut |conn| {
        let tx = conn.unchecked_transaction()?;
        let mut scan = entry::EntryScan {
            source,
            tx: &tx,
            mount_cache,
        };
        for item in items.drain(..) {
            prepared.push(match item {
                LibraryWalkItem::Error(error) => PreparedItem {
                    observed: None,
                    action: PreparedAction::Error(error),
                },
                LibraryWalkItem::Entry(entry) => {
                    mobile_sync.observe(source, root, &entry);
                    let LibraryEntry {
                        path,
                        is_file,
                        metadata,
                    } = entry;
                    let action = if is_file {
                        match entry::classify_entry(&mut scan, &path, metadata)? {
                            EntryPlan::Skip(outcome) => PreparedAction::Skip(outcome),
                            plan @ EntryPlan::Import { .. } => PreparedAction::Import {
                                plan,
                                meta: Box::new(None),
                            },
                        }
                    } else {
                        PreparedAction::Skip(EntryOutcome::Directory)
                    };
                    PreparedItem {
                        observed: Some((path, !is_file)),
                        action,
                    }
                }
            });
        }
        tx.commit()?;
        Ok(())
    })?;

    for item in &mut prepared {
        if let PreparedAction::Import { plan, meta } = &mut item.action {
            let path = plan
                .import_path()
                .expect("an import plan always carries its source path");
            **meta = Some(super::track_meta::read_meta_with_fallback(source, path));
        }
    }

    let mut advanced_paths = Vec::new();
    leases.run(writer, &mut |conn| {
        let tx = conn.unchecked_transaction()?;
        let mut scan = entry::EntryScan {
            source,
            tx: &tx,
            mount_cache,
        };
        for item in prepared.drain(..) {
            if let Some((path, is_directory)) = &item.observed {
                state.trace.observed_paths.insert(path.clone());
                if *is_directory {
                    state.trace.dirs.insert(path.clone());
                }
            }
            let (path, outcome) = match item.action {
                PreparedAction::Error(error) => (
                    None,
                    record_walk_error(source, &tx, &mut state.trace.failed, root, &error)?,
                ),
                PreparedAction::Skip(outcome) => (
                    item.observed.as_ref().map(|(path, _)| path.as_path()),
                    outcome,
                ),
                PreparedAction::Import { plan, meta } => {
                    let path = item.observed.as_ref().map(|(path, _)| path.as_path());
                    let outcome = entry::apply_entry(
                        &mut scan,
                        plan,
                        (*meta).expect("metadata is read before the second lease"),
                    )?;
                    (path, outcome)
                }
            };
            if outcome.examined_audio_file() {
                if let Some(path) = path {
                    advanced_paths.push(path.to_path_buf());
                }
            }
            state.record(&outcome);
        }
        tx.commit()?;
        Ok(())
    })?;
    for path in advanced_paths {
        progress.advance(&path);
    }
    Ok(())
}
