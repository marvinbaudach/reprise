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
/// Reprise's own temporary files, and nothing else, are named
/// `.reprise-<16 hex digits>.tmp`.
const TEMP_PREFIX: &str = ".reprise-";
const TEMP_SUFFIX: &str = ".tmp";

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

/// How long a temporary file has to sit untouched before [`sweep_leftovers`]
/// treats it as abandoned. An hour is far beyond any single publication —
/// even 20 MB onto a slow USB stick — so a live writer's file can never be
/// mistaken for a leftover, in this process or another.
const LEFTOVER_MAX_AGE: Duration = Duration::from_secs(60 * 60);

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
    if let Some(directory) = target.parent() {
        sweep_leftovers(directory, LEFTOVER_MAX_AGE);
    }
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
        // Anything else means this filesystem would not link the file into
        // place — see `publish_directly`.
        Err(_) => {
            let published = publish_directly(target, payload);
            remove_temporary(&temporary);
            published
        }
    }
}

/// Publishes `payload` at `target` by creating the target itself with
/// `O_EXCL`, for filesystems whose VFS has no `->link` at all.
///
/// Linux `vfat`, `exfat` and `ntfs3`, and the FUSE MTP/gvfs mounts phones
/// present, answer `link(2)` with `EPERM`/`EOPNOTSUPP` — and those are
/// exactly the filesystems external music drives and players use. Without
/// this, the whole payload was written and fsynced and then thrown away on
/// every single attempt, forever, with no signal to anyone.
///
/// `O_EXCL` is honoured on vfat and exfat, so the one guarantee that matters
/// survives: an existing file of the user's still cannot be replaced. What is
/// lost is content atomicity — a crash mid-write can leave a short file
/// behind. That is the right trade against a feature that is silently inert.
/// `rename` would have kept atomicity and is *not* an option: it replaces its
/// destination.
fn publish_directly(target: &Path, payload: &[u8]) -> io::Result<Published> {
    let mut file = match OpenOptions::new().write(true).create_new(true).open(target) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            return Ok(Published::AlreadyPresent)
        }
        Err(error) => return Err(error),
    };
    if let Err(error) = file.write_all(payload).and_then(|()| file.sync_all()) {
        drop(file);
        // The half-written file is ours — `create_new` proved nothing was
        // there — and a truncated cover or sidecar would be served as if it
        // were the real thing.
        let _ = fs::remove_file(target);
        return Err(error);
    }
    Ok(Published::Written)
}

/// Reserves a temporary file beside `target`.
///
/// The name is a fixed 29 bytes and deliberately carries nothing of the
/// target's own name: prefixing it made the temporary ~30 bytes longer than
/// the target, so any track whose filename ran past ~225 bytes — routine for
/// classical and live-set names — failed the very first `open` with
/// `ENAMETOOLONG` and could never receive a sidecar at all. Uniqueness comes
/// from the 64 random bits plus `create_new`, not from the target's name.
fn create_temporary(target: &Path) -> io::Result<(PathBuf, File)> {
    for _ in 0..TEMP_CREATE_ATTEMPTS {
        let temporary = target.with_file_name(temporary_name(fastrand::u64(..)));
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

fn temporary_name(token: u64) -> String {
    format!("{TEMP_PREFIX}{token:016x}{TEMP_SUFFIX}")
}

/// Removes Reprise's own abandoned temporary files from `directory`.
///
/// The happy path and every error path unlink the temporary themselves, but
/// a process that dies in between — the window is seconds for a 20 MB cover
/// on USB or NFS — leaves one lying in the user's album folder, and nothing
/// else in the crate ever looked for them. On FAT and NTFS the leading dot
/// carries no hidden semantics, so they are plainly visible junk.
///
/// Two deliberate narrowings, because this deletes files inside the
/// collection: only names matching Reprise's own exact pattern
/// ([`is_temporary_name`]) are ever considered, and only regular files
/// (`DirEntry::metadata` does not follow symlinks) that have not been
/// modified for `max_age` — long enough that no live writer, in this process
/// or another, can still own them.
fn sweep_leftovers(directory: &Path, max_age: Duration) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        if !entry.file_name().to_str().is_some_and(is_temporary_name) {
            continue;
        }
        let is_abandoned = entry.metadata().is_ok_and(|metadata| {
            metadata.is_file()
                && metadata
                    .modified()
                    .ok()
                    .and_then(|modified| now.duration_since(modified).ok())
                    .is_some_and(|age| age >= max_age)
        });
        if !is_abandoned {
            continue;
        }
        if let Err(error) = fs::remove_file(entry.path()) {
            tracing::warn!(
                path = %entry.path().display(),
                %error,
                "could not sweep an abandoned writeback temporary file"
            );
        }
    }
}

fn is_temporary_name(name: &str) -> bool {
    name.strip_prefix(TEMP_PREFIX)
        .and_then(|rest| rest.strip_suffix(TEMP_SUFFIX))
        .is_some_and(|token| {
            token.len() == 16 && token.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
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
