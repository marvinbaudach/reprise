//! Unix filesystem facts used by the path-backed library source.

use std::path::{Path, PathBuf};

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
/// `tracks.path`/scan roots are [`super::source::LibrarySource::walk`] inputs
/// derived from that same root, so this isn't separately enforced here.
pub(crate) fn nearest_existing_ancestor(path: &Path) -> Option<(PathBuf, u64)> {
    // The walk-to-`/` guarantee above holds only for an absolute path, so the
    // requirement is asserted here, where it is actually needed, rather than at
    // the scanner — which has no such need and used to carry it anyway.
    //
    // A relative path is not a panic in release: `ancestors()` bottoms out at
    // `""`, which no `lstat` succeeds on, so this answers `None`, which
    // `reachability` turns into `MissingReason::Unknown`. That is the honest
    // outcome, and it is why a source with no filesystem ancestry — a SAF tree,
    // whose root is a content URI and therefore not absolute — degrades safely
    // here instead of lying.
    debug_assert!(
        path.is_absolute(),
        "the Unix source's ancestor walk assumes an absolute path; got {}",
        path.display()
    );
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

pub(crate) fn file_identity(metadata: &std::fs::Metadata) -> Option<(u64, u64)> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some((metadata.dev(), metadata.ino()))
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
