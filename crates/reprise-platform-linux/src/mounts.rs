//! Which mount a path lives on, read from the kernel's mount table.
//!
//! `canonicalize`, `metadata`, `exists` and `statfs` on a path block when the
//! path sits on a dead hard NFS or SMB mount, so a caller that must not block
//! (the GTK main thread) cannot use them to find the mount. This lookup is
//! lexical instead: the mount points come from `/proc/self/mountinfo` (kernel
//! memory, never blocks) and the mount of a path is the longest mount point that
//! is a prefix of it on whole path components, so `/mnt/nas2/x` is not on
//! `/mnt/nas`.
//!
//! A path that cannot be placed (relative, containing `..`, no mount point
//! covers it, or mountinfo unreadable) has no mount: the lookup returns `None`.
//!
//! The lookup never follows symlinks, because resolving one would ask the
//! mount. A path is keyed to the mount that holds the path as written.

use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// The mount table of this process.
const MOUNTINFO_PATH: &str = "/proc/self/mountinfo";

/// Zero-based index of the mount point in a mountinfo line.
const MOUNT_POINT_FIELD: usize = 4;

/// Length of an escape such as `\040`.
const ESCAPE_LEN: usize = 4;

/// Set once the unreadable-mountinfo warning has been logged, so a mountinfo
/// that stays unreadable does not log on every lookup.
static UNREADABLE_LOGGED: AtomicBool = AtomicBool::new(false);

/// The mount point `path` lives on, from `/proc/self/mountinfo`, read on each
/// call so a mount made since startup is seen. `None` when the path cannot be
/// placed or the table is unreadable (logged once per process).
pub fn mount_point_of(path: &Path) -> Option<PathBuf> {
    match std::fs::read(MOUNTINFO_PATH) {
        Ok(mountinfo) => mount_point_in(&mountinfo, path),
        Err(error) => {
            if !UNREADABLE_LOGGED.swap(true, Ordering::Relaxed) {
                tracing::warn!(%error, "mountinfo is unreadable; mount lookups find no mount");
            }
            None
        }
    }
}

/// The mount point of `path` among those in `mountinfo`: the longest one that
/// prefixes it. The table is read as bytes: the kernel escapes only space, tab,
/// newline and backslash, so a mount point or device name may legally hold other
/// non-UTF-8 bytes. The lines are walked once and only the best match is kept.
pub fn mount_point_in(mountinfo: &[u8], path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() || path.components().any(|part| part == Component::ParentDir) {
        return None;
    }
    mountinfo
        .split(|byte| *byte == b'\n')
        .filter_map(|line| line.split(|byte| *byte == b' ').nth(MOUNT_POINT_FIELD))
        .map(unescape_mount_point)
        .filter(|point| point.is_absolute() && path.starts_with(point))
        .max_by_key(|point| point.components().count())
}

/// Undoes the kernel's `\NNN` octal escapes (space, tab, newline, backslash).
/// Anything that is not three octal digits after a backslash stays as it is.
fn unescape_mount_point(bytes: &[u8]) -> PathBuf {
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        let escaped = (byte == b'\\')
            .then(|| bytes.get(at + 1..at + 4))
            .flatten()
            .and_then(octal_byte);
        match escaped {
            Some(decoded) => {
                out.push(decoded);
                at += ESCAPE_LEN;
            }
            None => {
                out.push(byte);
                at += 1;
            }
        }
    }
    PathBuf::from(OsString::from_vec(out))
}

fn octal_byte(digits: &[u8]) -> Option<u8> {
    digits
        .iter()
        .try_fold(0u32, |value, digit| {
            digit
                .is_ascii_digit()
                .then(|| u32::from(digit - b'0'))
                .filter(|octal| *octal < 8)
                .map(|octal| value * 8 + octal)
        })
        .and_then(|value| u8::try_from(value).ok())
}

#[cfg(test)]
#[path = "mounts_tests.rs"]
mod tests;
