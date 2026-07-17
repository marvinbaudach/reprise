//! Task 1.6: per-scan memoized `mount_point` resolution for
//! `scan_folder_inner`'s insert/upsert arm, move-detection arm, and
//! fast-path-restore branch (all in `scanner.rs`). Split into its own file
//! purely to keep `scanner.rs` itself under the project's 800-line rule,
//! same rationale as `scanner_vanish.rs`'s own module doc comment —
//! `scanner.rs` declares this via `#[path = "scanner_mount.rs"] mod mount;`,
//! so this is still the crate-private `crate::library::scanner::mount`
//! module.
//!
//! ## Why the mount point must be recorded now, not derived later
//!
//! Given a path like `/media/nas-music/Rock/x.flac`, nothing in the string
//! itself says whether the mount is `/media/nas-music` or `/media` — both
//! are plausible mount points for the same file. `mounts::mount_point_of`
//! can answer that question by walking the filesystem and comparing device
//! ids, but only while the drive is still mounted: the moment it's the
//! missing-drive case this whole self-healing-list feature exists for,
//! `/proc/mounts` has no entry to walk, and the ancestor directories above
//! the vanished file resolve to whatever filesystem is now underneath
//! (typically the root filesystem) rather than the original drive. The
//! mount point is knowable only while the file is present and reachable —
//! so the scan records it as a fact at that moment, rather than trying to
//! recompute it later when the evidence is already gone. `tracks.mount_
//! point` (schema v10) is where that fact is kept.
//!
//! ## Memoization
//!
//! `mounts::mount_point_of` costs one `lstat` per path component walked —
//! O(depth) per file. A library scan touches every file under a root, and
//! files in the same directory always resolve to the same mount point (the
//! mount point is a property of the directory, not of the individual file),
//! so [`MountPointCache`] memoizes the result per `path.parent()` rather
//! than per file: a folder with a thousand tracks pays for one walk instead
//! of a thousand. The cache is scoped to a single scan (owned locally by
//! `scan_folder_inner`, never persisted) — a longer-lived cache would risk
//! serving a stale answer across scans if a mount changed between them,
//! which defeats the whole point of recording state that must stay current.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::mounts;

/// Per-scan cache: `path.parent()` → the `mount_point` value to record for
/// every file directly inside that directory. See this module's doc
/// comment for why memoizing on the parent (rather than the full path) is
/// both correct — the mount point is a property of the directory, not the
/// file — and the whole reason this exists.
#[derive(Default)]
pub(super) struct MountPointCache {
    by_parent: HashMap<PathBuf, Option<String>>,
}

impl MountPointCache {
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Resolves the `mount_point` column value to store for `path`,
    /// consulting (and populating) the per-parent-directory cache. `None`
    /// only in the same case `mounts::mount_point_of` itself returns `None`
    /// — even `/` couldn't be `lstat`'d, which should not happen on a
    /// working Linux system; see that function's own doc comment.
    pub(super) fn resolve(&mut self, path: &Path) -> Option<String> {
        let Some(parent) = path.parent() else {
            // No parent component at all (bare "/") — not a shape any real
            // audio file path takes, but handled directly rather than
            // panicking or caching under a placeholder key.
            return mounts::mount_point_of(path).map(|p| path_to_string(&p));
        };
        if let Some(cached) = self.by_parent.get(parent) {
            return cached.clone();
        }
        let resolved = mounts::mount_point_of(path).map(|p| path_to_string(&p));
        self.by_parent
            .insert(parent.to_path_buf(), resolved.clone());
        resolved
    }
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
