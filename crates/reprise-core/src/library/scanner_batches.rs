//! Batch execution for scanner walks. Tag reads happen between the two writer
//! leases so a slow source never owns the database writer for the whole walk.

use std::path::{Path, PathBuf};

use super::cue_sheets::CueDirectories;
use super::entry::{self, EntryOutcome, EntryPlan, ImportPlan};
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
        plan: Box<ImportPlan>,
        meta: Box<Option<Result<super::track_meta::MetaOutcome, ScanError>>>,
    },
}

// These independent scanner services stay explicit so their mutable lease-bound lifetimes remain visible.
#[expect(
    clippy::too_many_arguments,
    reason = "independent scanner services keep their lease-bound lifetimes explicit"
)]
pub(super) fn walk_root_in_batches<'source>(
    source: &'source dyn LibrarySource,
    writer: &dyn ScanWriter,
    root: &Path,
    state: &mut WalkState,
    mobile_sync: &mut mobile_sync::MobileSyncDiscovery,
    mount_cache: &mut mount::MountPointCache<'source>,
    cues: &mut CueDirectories,
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
            cues,
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
            cues,
            progress,
            leases,
        )?;
    }
    Ok(())
}

// These independent scanner services stay explicit so their mutable lease-bound lifetimes remain visible.
#[expect(
    clippy::too_many_arguments,
    reason = "independent scanner services keep their lease-bound lifetimes explicit"
)]
fn process_batch<'source>(
    mut items: Vec<LibraryWalkItem>,
    source: &'source dyn LibrarySource,
    writer: &dyn ScanWriter,
    root: &Path,
    state: &mut WalkState,
    mobile_sync: &mut mobile_sync::MobileSyncDiscovery,
    mount_cache: &mut mount::MountPointCache<'source>,
    cues: &mut CueDirectories,
    progress: &mut BatchProgress<'_>,
    leases: &mut LeaseMetrics,
) -> Result<(), ScanError> {
    // The directories of this batch's audio are listed, and their CUE sheets
    // read, before the writer is leased: that is source I/O, and a slow source
    // must not own the writer while it answers.
    for item in &items {
        if let LibraryWalkItem::Entry(entry) = item {
            if entry.is_file && super::is_audio_file(&entry.path) {
                if let Some(directory) = source.parent_of(&entry.path) {
                    cues.discover(source, &directory);
                }
            }
        }
    }
    let mut prepared = Vec::with_capacity(items.len());
    // Both batch leases read the catalog before they write it, so each opens
    // IMMEDIATE (the second only when it has something to write): a rival
    // commit between that read and the write would otherwise fail the lock
    // upgrade with SQLITE_BUSY_SNAPSHOT, which `busy_timeout` never retries.
    // Tag reads stay between the two leases.
    leases.run(writer, &mut |conn| {
        let tx = crate::events::immediate_transaction(conn)?;
        let mut scan = entry::EntryScan {
            source,
            tx: &tx,
            mount_cache,
            cues: &mut *cues,
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
                            EntryPlan::Import(plan) => PreparedAction::Import {
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
            let path = plan.path();
            if let Some(sheet) = plan.governing() {
                cues.ensure_parsed(source, sheet, path);
            }
            **meta = Some(super::track_meta::read_meta_with_fallback(source, path));
        }
    }

    let mut advanced_paths = Vec::new();
    // A batch that only skips unchanged files writes nothing, so it keeps the
    // deferred transaction that never takes the lock.
    let writes = prepared
        .iter()
        .any(|item| !matches!(item.action, PreparedAction::Skip(_)));
    leases.run(writer, &mut |conn| {
        let tx = if writes {
            crate::events::immediate_transaction(conn)?
        } else {
            conn.unchecked_transaction()?
        };
        let mut scan = entry::EntryScan {
            source,
            tx: &tx,
            mount_cache,
            cues: &mut *cues,
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
                        &plan,
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
