//! Task 1.4's Unix mount-point grouping and the Linux residence evidence used
//! by [`super::source::UnixLibrarySource`]. Missing-item classification itself
//! is platform-neutral behind [`super::source::LibrarySource`].
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
//! 99_999`), no loopback device or namespace required. The source trait keeps
//! that testable comparison while allowing non-POSIX sources to supply a
//! different residence token.
//!
//! Known limitation, intentionally not worked around: btrfs subvolumes and
//! bind mounts can share a single device id across what look like separate
//! mount points, so [`mount_point_of`] may walk higher than the "obvious"
//! mount point in those setups. That's acceptable here — the only thing the
//! resolved mount point is used for is grouping missing tracks by "what
//! disappears together when this mount goes away", and a too-high boundary
//! still groups correctly, it just groups a superset together as one unit.

use std::path::{Path, PathBuf};

use super::source::{device_id, nearest_existing_ancestor, LibrarySource};
use crate::models::MissingReason;

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
pub(crate) fn mount_point_of(path: &Path) -> Option<PathBuf> {
    let (mut mount_point, device) = nearest_existing_ancestor(path)?;
    loop {
        let Some(parent) = mount_point.parent() else {
            // Reached "/" — nothing above it to compare against.
            return Some(mount_point);
        };
        let parent_device = std::fs::symlink_metadata(parent)
            .ok()
            .and_then(|metadata| device_id(&metadata));
        if parent_device != Some(device) {
            return Some(mount_point);
        }
        mount_point = parent.to_path_buf();
    }
}

/// Classifies why an item at `path` is missing through its library source.
///
/// The stored value remains SQLite `i64`; the Unix source round-trips the
/// same `st_dev` bit pattern the scanner stores today, while another source
/// may derive the token from its own stable residence identity.
pub(crate) fn classify_missing(
    source: &dyn LibrarySource,
    stored: Option<i64>,
    path: &Path,
) -> MissingReason {
    source.reachability(path, stored)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::source::{LibrarySource, UnixLibrarySource};
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
            classify_missing(&UnixLibrarySource, Some(real_dev as i64), &gone_path),
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
            classify_missing(
                &UnixLibrarySource,
                Some(real_dev as i64 + 99_999),
                &gone_path,
            ),
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

        assert_eq!(
            classify_missing(&UnixLibrarySource, None, &gone_path),
            MissingReason::Unknown
        );
    }

    struct DocumentTreeSource {
        provider_tree_id: Option<&'static str>,
    }

    impl LibrarySource for DocumentTreeSource {
        fn residence_token(&self, _at: &Path) -> Option<i64> {
            self.provider_tree_id?.strip_prefix("tree-")?.parse().ok()
        }
    }

    #[test]
    fn classifier_supports_the_same_triad_with_a_provider_tree_token() {
        let at = Path::new("content:/music/album/track.flac");

        assert_eq!(
            classify_missing(
                &DocumentTreeSource {
                    provider_tree_id: Some("tree-41"),
                },
                Some(41),
                at,
            ),
            MissingReason::Deleted
        );
        assert_eq!(
            classify_missing(
                &DocumentTreeSource {
                    provider_tree_id: Some("tree-73"),
                },
                Some(41),
                at,
            ),
            MissingReason::Unmounted
        );
        assert_eq!(
            classify_missing(
                &DocumentTreeSource {
                    provider_tree_id: None,
                },
                Some(41),
                at,
            ),
            MissingReason::Unknown
        );
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
