//! Task 1.4: distinguishes "the drive is unplugged" from "the file was
//! deleted" for a missing track, without any platform trait, GVolumeMonitor,
//! or `/proc/mounts` parsing.
//!
//! The whole mechanism rests on one fact that is already sitting in the
//! database: `tracks.device` (schema v2) is the `st_dev` of the file as it
//! was last seen by the scanner. When a removable drive is unmounted, its
//! mount point directory does not vanish — the *mount* goes away, and the
//! directory that used to be its mount point reverts to belonging to
//! whatever filesystem is underneath (typically the root filesystem). That
//! directory's `st_dev` therefore changes. So: walk up from the missing
//! file's path to the nearest directory that still exists, read its device,
//! and compare it against the device recorded in the database.
//!
//! - Device matches → the mount is still there, reachable, and the file
//!   genuinely isn't at that path anymore. `Deleted`.
//! - Device differs → we're looking at a *different* filesystem than the one
//!   this file lived on — the original mount is absent. `Unmounted`.
//! - No device was ever recorded → schema v1 predates this column, and the
//!   v10 migration backfills exactly these rows to `NULL` (see
//!   `db::SCHEMA_V10`'s doc comment). There is no evidence either way.
//!   `Unknown` — never treated as safely removable (see `MissingReason`'s
//!   own doc comment).
//!
//! This needs only two `stat` calls and a column that already exists, and —
//! crucially — it is fully testable without root and without mounting
//! anything: a test can plant a bogus device id in the database and get a
//! deterministic `Unmounted` verdict from pure arithmetic (`real_dev +
//! 99_999`), no loopback device or namespace required. That testability is
//! *why* this approach was chosen over a `GVolumeMonitor`-based trait (which
//! would live in the GTK shell anyway — `reprise-core` may never depend on
//! gtk4/libadwaita/gstreamer/zbus) or `/proc/mounts` parsing (Linux-only,
//! and it would need a platform abstraction to be unit-tested at all).
//!
//! Known limitation, intentionally not worked around: btrfs subvolumes and
//! bind mounts can share a single device id across what look like separate
//! mount points, so [`mount_point_of`] may walk higher than the "obvious"
//! mount point in those setups. That's acceptable here — the only thing the
//! resolved mount point is used for is grouping missing tracks by "what
//! disappears together when this mount goes away", and a too-high boundary
//! still groups correctly, it just groups a superset together as one unit.

use std::path::{Path, PathBuf};

use crate::models::MissingReason;

/// Returns `(ancestor_path, st_dev)` for the nearest ancestor of `path`
/// (starting at `path` itself) that can be `lstat`'d successfully.
///
/// Uses `symlink_metadata` (lstat), deliberately never `metadata` (stat):
/// if some ancestor component in the path is itself a symlink, `lstat`
/// reports the symlink's own device rather than following it to whatever
/// it points at. Following the symlink here would let an ancestor that
/// merely *points into* a different mount fabricate a foreign device id —
/// and thus a bogus `Unmounted` verdict — even though the symlink itself
/// sits on the original, still-mounted filesystem.
///
/// `Path::ancestors()` walks `path`, then each successive parent, ending at
/// `/` for an absolute path — so the walk is capped at the root without any
/// extra bookkeeping. Returns `None` only if even `/` can't be `lstat`'d,
/// which should not happen on a working Linux system.
fn nearest_existing_ancestor(path: &Path) -> Option<(PathBuf, u64)> {
    use std::os::unix::fs::MetadataExt;
    path.ancestors().find_map(|ancestor| {
        std::fs::symlink_metadata(ancestor)
            .ok()
            .map(|meta| (ancestor.to_path_buf(), meta.dev()))
    })
}

/// `st_dev` of the nearest ancestor of `path` that currently exists,
/// starting the search at `path` itself. `lstat` (`symlink_metadata`) only —
/// see [`nearest_existing_ancestor`]'s doc comment for why this must never
/// follow symlinks. `None` only if even `/` can't be `lstat`'d.
///
/// `#[allow(dead_code)]`: Task 1.4 builds this mechanism in isolation; a
/// later task wires it into the scanner/watcher's missing-file handling as
/// the real caller. Exercised directly by this module's own tests in the
/// meantime — see the module doc comment for why that's sufficient (no
/// mounting, no root needed).
#[allow(dead_code)]
pub(crate) fn nearest_existing_ancestor_dev(path: &Path) -> Option<u64> {
    nearest_existing_ancestor(path).map(|(_, dev)| dev)
}

/// The highest (closest-to-root) ancestor of `path` that still shares
/// `path`'s device — i.e. that ancestor's mount point.
///
/// Starts at the nearest existing ancestor (see [`nearest_existing_ancestor`]
/// — `path` itself need not exist) and walks upward one parent at a time,
/// stopping as soon as a parent's device differs from the device we started
/// on, or the walk reaches `/`. The last directory that still matched is the
/// mount point.
///
/// Callers that classify many tracks under the same directory should
/// memoize this per parent directory rather than re-walking per file — this
/// function itself does no caching.
///
/// See this module's doc comment for the known btrfs-subvolume / bind-mount
/// limitation: this may resolve to an ancestor higher than the "obvious"
/// mount point when subvolumes/binds share a device id, which is accepted
/// because the only use of the result is grouping "what disappears
/// together".
///
/// `#[allow(dead_code)]`: see [`nearest_existing_ancestor_dev`]'s doc
/// comment — not wired into a caller until a later task.
#[allow(dead_code)]
pub(crate) fn mount_point_of(path: &Path) -> Option<PathBuf> {
    use std::os::unix::fs::MetadataExt;
    let (mut mount_point, device) = nearest_existing_ancestor(path)?;
    loop {
        let Some(parent) = mount_point.parent() else {
            // Reached "/" — nothing above it to compare against.
            return Some(mount_point);
        };
        let parent_device = std::fs::symlink_metadata(parent).ok().map(|m| m.dev());
        if parent_device != Some(device) {
            return Some(mount_point);
        }
        mount_point = parent.to_path_buf();
    }
}

/// Classifies why a track at `path` is missing, given the device id that was
/// recorded for it at scan time (`tracks.device`, `None` for a row that
/// predates schema v2 or whose `stat` failed on last scan — see
/// `library::scanner::file_stat`'s doc comment).
///
/// - `stored_device` is `None` → there is nothing to compare against.
///   `Unknown` (see `MissingReason`'s own doc comment for why this must stay
///   `Unknown` rather than defaulting to either concrete reason).
/// - The nearest existing ancestor of `path` shares `stored_device` → the
///   filesystem the file lived on is present and reachable, and the file
///   simply isn't there anymore. `Deleted`.
/// - The nearest existing ancestor's device differs → we're standing on a
///   *different* filesystem than the one recorded for this file, which
///   means the original mount is currently absent. `Unmounted`.
/// - Even `/` couldn't be `lstat`'d (see [`nearest_existing_ancestor_dev`])
///   → no evidence either way. `Unknown`.
///
/// `stored_device` is `i64` (SQLite's only integer type, matching
/// `Track::device` and `scanner::file_stat`'s `dev as i64` storage cast) and
/// is cast back to `u64` for the comparison — round-tripping the same bit
/// pattern `file_stat` cast away from `u64` on the way in.
///
/// `#[allow(dead_code)]`: see [`nearest_existing_ancestor_dev`]'s doc
/// comment — not wired into a caller until a later task.
#[allow(dead_code)]
pub(crate) fn classify_missing(stored_device: Option<i64>, path: &Path) -> MissingReason {
    let Some(stored_device) = stored_device else {
        return MissingReason::Unknown;
    };
    let stored_device = stored_device as u64;
    match nearest_existing_ancestor_dev(path) {
        Some(current_device) if current_device == stored_device => MissingReason::Deleted,
        Some(_) => MissingReason::Unmounted,
        None => MissingReason::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    fn dev_of(path: &Path) -> u64 {
        std::fs::symlink_metadata(path).unwrap().dev()
    }

    /// `classify_missing` with the file's real device recorded: the file's
    /// directory still exists and still belongs to the same device, so the
    /// only honest conclusion is that the file itself was deleted.
    #[test]
    fn classify_missing_returns_deleted_when_device_matches() {
        let dir = tempfile::tempdir().unwrap();
        let real_dev = dev_of(dir.path());
        let gone_path = dir.path().join("gone.flac");

        assert_eq!(
            classify_missing(Some(real_dev as i64), &gone_path),
            MissingReason::Deleted
        );
    }

    /// A stored device that doesn't match anything on this filesystem
    /// fabricates exactly the situation an unmounted drive produces: the
    /// nearest existing ancestor belongs to a different device than the one
    /// recorded. `real_dev + 99_999` is never going to collide with a real
    /// `st_dev` in a test environment, so this is deterministic without
    /// mounting or unmounting anything.
    #[test]
    fn classify_missing_returns_unmounted_when_device_differs() {
        let dir = tempfile::tempdir().unwrap();
        let real_dev = dev_of(dir.path());
        let gone_path = dir.path().join("gone.flac");

        assert_eq!(
            classify_missing(Some(real_dev as i64 + 99_999), &gone_path),
            MissingReason::Unmounted
        );
    }

    /// No recorded device (schema-v1 row, or a `stat` that failed on last
    /// scan) means there is no basis for a verdict at all — `Unknown`, never
    /// a guessed concrete reason.
    #[test]
    fn classify_missing_returns_unknown_when_device_not_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let gone_path = dir.path().join("gone.flac");

        assert_eq!(classify_missing(None, &gone_path), MissingReason::Unknown);
    }

    /// `mount_point_of`'s invariant, asserted rather than a hardcoded path
    /// so this test passes regardless of the machine's filesystem layout:
    /// the result is a prefix of the input path, shares the input's device,
    /// and is either `/` itself or has a parent that sits on a *different*
    /// device (otherwise the walk would have continued past it).
    #[test]
    fn mount_point_of_satisfies_prefix_and_device_boundary_invariant() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a/b/c");
        std::fs::create_dir_all(&nested).unwrap();
        let file_path = nested.join("track.flac");
        std::fs::write(&file_path, b"not really audio").unwrap();

        let mount_point = mount_point_of(&file_path).unwrap();

        assert!(
            file_path.starts_with(&mount_point),
            "{mount_point:?} must be a prefix of {file_path:?}"
        );
        assert_eq!(dev_of(&mount_point), dev_of(&file_path));
        match mount_point.parent() {
            None => {} // mount_point == "/"
            Some(parent) => {
                assert_ne!(
                    dev_of(parent),
                    dev_of(&mount_point),
                    "walk should have continued past {mount_point:?} onto {parent:?} \
                     if they share a device"
                );
            }
        }
    }

    /// A symlinked ancestor must count as "existing" via its own `lstat`
    /// identity, never by following it to its target. Proven with a
    /// *dangling* symlink — its target doesn't exist, so `lstat` on the
    /// symlink itself still succeeds, while `stat` (following it) would fail
    /// with `ENOENT`. If the walk ever used `metadata` instead of
    /// `symlink_metadata`, this ancestor would look "missing" and the walk
    /// would skip past it to `dir`, returning a different ancestor
    /// altogether — so asserting the returned ancestor *is* `link` itself
    /// (not `dir`, and not an error) is a real regression check, not a
    /// tautology, without needing a second real filesystem/device to prove
    /// it.
    #[test]
    fn nearest_existing_ancestor_dev_uses_lstat_not_stat_on_symlink_ancestor() {
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(dir.path().join("nowhere"), &link).unwrap();

        // `link` itself exists (as a symlink) and is the nearest existing
        // ancestor of a path underneath it, even though nothing underneath
        // it can possibly exist (its target is nowhere at all).
        let missing_child = link.join("gone.flac");

        let (ancestor_path, ancestor_dev) = nearest_existing_ancestor(&missing_child).unwrap();

        assert_eq!(ancestor_path, link);
        assert_eq!(ancestor_dev, dev_of(&link));
    }
}
