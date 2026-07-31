//! The one atomic, never-overwriting publisher shared by the two writers
//! Reprise is allowed to run inside the user's music collection: the `.lrc`
//! lyrics sidecar (`LYR-7`) and the album `cover.<ext>` (`COVER-1`).
//!
//! Both had an identical copy of this dance — write a temporary file next to
//! the target, `fsync` it, then link it into place — and a divergence between
//! the two would be a silent data-safety difference in the collection, not a
//! cosmetic one. Having it exactly once is the point of this module.
//!
//! ## Why link-then-unlink rather than `rename`
//!
//! `rename` replaces its destination; `fs::hard_link` never does. The target
//! file belongs to the user, so replacing it is the one thing that must be
//! impossible — even against a concurrent writer that creates the file
//! between the caller's existence check and this publication. The kernel
//! closes that race for us: `link(2)` fails with `EEXIST`, which we report as
//! [`Published::AlreadyPresent`].

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

const TEMP_CREATE_ATTEMPTS: usize = 16;

/// How long a writeback suppresses the library watcher for the paths it
/// touches (`library::watcher::ignore_path`).
///
/// Both targets live *inside* the watched library root, so without this every
/// publication emits four relevant inotify events — temporary create,
/// temporary modify, target create, temporary delete — and each one re-arms
/// the watcher's two-second debounce into another full
/// `scanner::scan_folder(root)` over the whole collection. A library-wide
/// lyrics batch would turn into a rescan storm.
///
/// Deliberately far longer than the tag editor's five seconds
/// (`library::tag_mutation::IGNORE_DURATION`): that writer touches one file
/// it already has open, while a cover payload runs up to `MAX_IMAGE_BYTES`
/// and may land on a USB disk or an NFS mount. The window has to still be
/// open when the *last* event of that write reaches the watcher thread.
const WATCHER_IGNORE: Duration = Duration::from_secs(60);

/// What a publication did. A refused publication (target already there) is a
/// normal outcome, not an error — see the module doc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Published {
    Written,
    AlreadyPresent,
}

/// Publishes `payload` at `target` without ever replacing an existing file.
///
/// The caller stays responsible for deciding *whether* `target` may be
/// written at all (a track must exist beside the sidecar, an album directory
/// must hold no folder image yet) and for logging the returned error in its
/// own words — every failure here is best-effort and must stay silent to the
/// user.
pub(crate) fn publish(target: &Path, payload: &[u8]) -> io::Result<Published> {
    publish_with(target, payload, |from, to| fs::hard_link(from, to))
}

/// [`publish`] with the link step injected, so the fallback path can be
/// tested on filesystems that do implement hard links.
fn publish_with(
    target: &Path,
    payload: &[u8],
    link: impl Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<Published> {
    ignore(target);
    let (temporary, mut file) = create_temporary(target)?;
    if let Err(error) = file.write_all(payload).and_then(|()| file.sync_all()) {
        drop(file);
        remove_temporary(&temporary);
        return Err(error);
    }
    drop(file);

    // Re-armed rather than relied upon from above: the payload write itself
    // may have taken longer than one window on slow media, and the link and
    // the unlink still have to be invisible to the watcher.
    ignore(target);
    ignore(&temporary);
    match link(&temporary, target) {
        Ok(()) => {
            remove_temporary(&temporary);
            Ok(Published::Written)
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            remove_temporary(&temporary);
            Ok(Published::AlreadyPresent)
        }
        Err(error) => {
            remove_temporary(&temporary);
            Err(error)
        }
    }
}

fn create_temporary(target: &Path) -> io::Result<(PathBuf, File)> {
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("writeback");
    for _ in 0..TEMP_CREATE_ATTEMPTS {
        let temporary =
            target.with_file_name(format!(".{name}.reprise-{:016x}.tmp", fastrand::u64(..)));
        // Armed *before* the create, never after: the inotify event exists
        // from the moment the file does.
        ignore(&temporary);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not reserve a unique writeback temporary file",
    ))
}

fn ignore(path: &Path) {
    crate::library::watcher::ignore_path(path, WATCHER_IGNORE);
}

fn remove_temporary(temporary: &Path) {
    if let Err(error) = fs::remove_file(temporary) {
        tracing::warn!(
            path = %temporary.display(),
            %error,
            "could not remove a writeback temporary file"
        );
    }
}

#[cfg(test)]
#[path = "writeback_publish_tests.rs"]
mod tests;
