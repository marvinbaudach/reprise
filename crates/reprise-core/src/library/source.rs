//! Platform-neutral library-source residence contract.
//!
//! Core owns the comparison that distinguishes a deleted track from a
//! temporarily unreachable source. Concrete sources own only the stable token
//! that makes that comparison meaningful on their platform.

use std::path::{Path, PathBuf};

use crate::models::MissingReason;

/// The residence and reachability capability every library source provides.
///
/// A source without a stable residence token returns `None`. That documented
/// degradation produces [`MissingReason::Unknown`]; it never fabricates an
/// identity and never turns missing evidence into a destructive verdict.
pub trait LibrarySource: Send + Sync {
    /// Returns the stable residence token of the nearest reachable location at
    /// `at`, or `None` when this source cannot provide one.
    fn residence_token(&self, at: &Path) -> Option<i64>;

    /// Classifies why an item already known to be missing at `at` is missing,
    /// given the residence token recorded for it at scan time (`tracks.device`,
    /// `None` for a row that predates schema v2 or whose residence lookup
    /// failed on the last scan — see `library::scanner::file_stat`'s doc
    /// comment).
    ///
    /// - `stored` is `None` → there is nothing to compare against. `Unknown`
    ///   (see `MissingReason`'s own doc comment for why this must stay
    ///   `Unknown` rather than defaulting to either concrete reason: nothing
    ///   downstream may treat such a row as safely auto-removable without
    ///   re-verifying the item first).
    /// - The item's location reports the same token it was last seen under →
    ///   the source it lived on is present and reachable, and the item simply
    ///   isn't there anymore. `Deleted`.
    /// - It reports a *different* token → we are looking at a different source
    ///   than the one recorded for this item, which means the original one is
    ///   currently absent. `Unmounted`.
    /// - This source can supply no token at all → no evidence either way.
    ///   `Unknown`. Two unknowns are never evidence of each other.
    ///
    /// The token is `i64` because SQLite has no other integer type; it matches
    /// `Track::device` and `scanner::file_stat`'s storage cast.
    /// [`UnixLibrarySource`] round-trips exactly the `st_dev` bit pattern that
    /// cast away from `u64` on the way in, so this stays the same comparison
    /// Linux made before the trait existed.
    fn reachability(&self, at: &Path, stored: Option<i64>) -> MissingReason {
        let Some(stored) = stored else {
            return MissingReason::Unknown;
        };
        match self.residence_token(at) {
            Some(current) if current == stored => MissingReason::Deleted,
            Some(_) => MissingReason::Unmounted,
            None => MissingReason::Unknown,
        }
    }
}

/// A path-backed library source whose residence token is Unix `st_dev`.
///
/// On targets without Unix metadata this source reports no token, preserving
/// the contract's honest `Unknown` degradation instead of inventing a value.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnixLibrarySource;

impl LibrarySource for UnixLibrarySource {
    fn residence_token(&self, at: &Path) -> Option<i64> {
        nearest_existing_ancestor_dev(at).map(|device| device as i64)
    }
}

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
///
/// This "capped at `/`" guarantee holds only for an *absolute* `path` — for
/// a relative path, `ancestors()` instead bottoms out at `""` (`Path::new("")`
/// does not exist, so the walk would return `None` rather than ever reaching
/// `/`). Every caller in this codebase passes an absolute path: library
/// roots come from GTK's folder chooser (always absolute) and
/// `tracks.path`/scan roots are `walkdir::WalkDir::new(root)` inputs derived
/// from that same root, so this isn't separately enforced here.
pub(crate) fn nearest_existing_ancestor(path: &Path) -> Option<(PathBuf, u64)> {
    path.ancestors().find_map(|ancestor| {
        let metadata = std::fs::symlink_metadata(ancestor).ok()?;
        Some((ancestor.to_path_buf(), device_id(&metadata)?))
    })
}

/// Returns Unix `st_dev`, or `None` when the target has no stable device id.
pub(crate) fn device_id(metadata: &std::fs::Metadata) -> Option<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some(metadata.dev())
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        None
    }
}

/// `st_dev` of the nearest ancestor of `path` that currently exists,
/// starting the search at `path` itself. `lstat` (`symlink_metadata`) only —
/// see [`nearest_existing_ancestor`]'s doc comment for why this must never
/// follow symlinks. `None` only if even `/` can't be `lstat`'d.
pub(crate) fn nearest_existing_ancestor_dev(path: &Path) -> Option<u64> {
    nearest_existing_ancestor(path).map(|(_, device)| device)
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::MetadataExt;
    use std::path::Path;

    use super::{LibrarySource, UnixLibrarySource};
    use crate::models::MissingReason;

    fn dev_of(path: &Path) -> u64 {
        std::fs::symlink_metadata(path).unwrap().dev()
    }

    #[test]
    fn unix_source_uses_the_nearest_existing_ancestor_residence_token() {
        let dir = tempfile::tempdir().unwrap();
        let expected = dev_of(dir.path()) as i64;
        let missing_track = dir.path().join("missing/track.flac");

        assert_eq!(
            UnixLibrarySource.residence_token(&missing_track),
            Some(expected)
        );
    }

    /// The file's real device recorded: its directory still exists and still
    /// belongs to the same device, so the only honest conclusion is that the
    /// file itself was deleted.
    #[test]
    fn unix_source_reports_deleted_when_the_device_matches() {
        let dir = tempfile::tempdir().unwrap();
        let real_dev = dev_of(dir.path());
        let gone_path = dir.path().join("gone.flac");

        assert_eq!(
            UnixLibrarySource.reachability(&gone_path, Some(real_dev as i64)),
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
    fn unix_source_reports_unmounted_when_the_device_differs() {
        let dir = tempfile::tempdir().unwrap();
        let real_dev = dev_of(dir.path());
        let gone_path = dir.path().join("gone.flac");

        assert_eq!(
            UnixLibrarySource.reachability(&gone_path, Some(real_dev as i64 + 99_999)),
            MissingReason::Unmounted
        );
    }

    /// No recorded device (schema-v1 row, or a `stat` that failed on last
    /// scan) means there is no basis for a verdict at all — `Unknown`, never
    /// a guessed concrete reason.
    #[test]
    fn unix_source_reports_unknown_when_no_device_was_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let gone_path = dir.path().join("gone.flac");

        assert_eq!(
            UnixLibrarySource.reachability(&gone_path, None),
            MissingReason::Unknown
        );
    }

    /// A source whose residence token is a DocumentsProvider tree id rather
    /// than an `st_dev`, touching no filesystem at all. It exists to prove the
    /// classification in [`LibrarySource::reachability`] is a comparison of
    /// opaque tokens and carries no POSIX assumption — the property the
    /// Android SAF source will depend on.
    struct DocumentTreeSource {
        provider_tree_id: Option<&'static str>,
    }

    impl LibrarySource for DocumentTreeSource {
        fn residence_token(&self, _at: &Path) -> Option<i64> {
            self.provider_tree_id?.strip_prefix("tree-")?.parse().ok()
        }
    }

    #[test]
    fn a_non_posix_token_yields_the_same_triad() {
        let at = Path::new("content:/music/album/track.flac");
        let under = |tree| DocumentTreeSource {
            provider_tree_id: tree,
        };

        assert_eq!(
            under(Some("tree-41")).reachability(at, Some(41)),
            MissingReason::Deleted
        );
        assert_eq!(
            under(Some("tree-73")).reachability(at, Some(41)),
            MissingReason::Unmounted
        );
        assert_eq!(
            under(None).reachability(at, Some(41)),
            MissingReason::Unknown
        );
    }
}
