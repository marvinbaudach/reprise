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

    /// Classifies an already-missing item using its token from the last scan.
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

    use super::{LibrarySource, UnixLibrarySource};

    #[test]
    fn unix_source_uses_the_nearest_existing_ancestor_residence_token() {
        let dir = tempfile::tempdir().unwrap();
        let expected = std::fs::symlink_metadata(dir.path()).unwrap().dev() as i64;
        let missing_track = dir.path().join("missing/track.flac");

        assert_eq!(
            UnixLibrarySource.residence_token(&missing_track),
            Some(expected)
        );
    }
}
