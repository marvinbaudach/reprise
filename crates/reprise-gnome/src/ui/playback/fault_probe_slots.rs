//! Which faulted tracks may have a file probe in flight (`playback_faults.rs`).
//!
//! The pure half of the probe bound: the ledger of running probes and the
//! lookup that says which mount a path lives on. Nothing here touches GTK, and
//! nothing here touches the track's path.
//!
//! A probe holds a thread of GLib's shared task pool for as long as its kernel
//! call blocks, and on a dead hard NFS or SMB mount it never returns. The bound
//! is therefore per mount: a dead mount can pin at most
//! [`MAX_FAULT_PROBES_PER_MOUNT`] threads, and a fault on a healthy disk still
//! gets its probe. [`MAX_FAULT_PROBES_TOTAL`] is only the pool-safety ceiling
//! over all mounts together.
//!
//! ## Finding the mount without asking the mount
//!
//! `canonicalize`, `metadata`, `exists` and `statfs` on a track path block on a
//! dead mount, and this lookup runs on the main thread. So it is lexical: the
//! mount points come from `/proc/self/mountinfo` (kernel memory, never blocks)
//! and the mount of a path is the longest mount point that is a prefix of it on
//! whole path components, so `/mnt/nas2/x` is not on `/mnt/nas`.
//!
//! A path that cannot be placed (relative, containing `..`, or mountinfo
//! unreadable) falls under the one shared [`MountKey::Unresolved`] key, which is
//! the old global bound. A symlink into another mount is attributed to the
//! mount of the symlink itself; that can put a dead mount's probes under a
//! healthy key, so it degrades to the old global bound for that path, no worse.

use std::collections::HashMap;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

/// How many probes may be in flight on one mount.
pub(in crate::ui) const MAX_FAULT_PROBES_PER_MOUNT: usize = 2;

/// The ceiling over all mounts together. A dead mount costs one thread per
/// probe it holds, and covers and the other background work share the pool, so
/// even several dead mounts must not be able to fill it.
pub(in crate::ui) const MAX_FAULT_PROBES_TOTAL: usize = 8;

/// Zero-based index of the mount point in a mountinfo line.
const MOUNT_POINT_FIELD: usize = 4;

/// Length of an escape such as `\040`.
const ESCAPE_LEN: usize = 4;

/// The mount a path lives on, as far as its text says.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(in crate::ui) enum MountKey {
    /// The mount point, unescaped.
    Mount(String),
    /// Every path that could not be placed shares this one key.
    Unresolved,
}

/// Maps a track path to its mount on the main thread; a test replaces it to
/// put paths on different "mounts".
pub(in crate::ui) type MountResolver = Rc<dyn Fn(&Path) -> MountKey>;

/// The production resolver: `/proc/self/mountinfo`, read on each call so a
/// mount made since startup is seen.
pub(in crate::ui) fn procfs_mount_resolver() -> MountResolver {
    Rc::new(
        |path| match std::fs::read_to_string("/proc/self/mountinfo") {
            Ok(mountinfo) => mount_of(&mountinfo, path),
            Err(error) => {
                tracing::warn!(%error, "mountinfo is unreadable; probes share one bound");
                MountKey::Unresolved
            }
        },
    )
}

/// The mount of `path` among the mount points in `mountinfo`.
pub(in crate::ui) fn mount_of(mountinfo: &str, path: &Path) -> MountKey {
    if !path.is_absolute() || path.components().any(|part| part == Component::ParentDir) {
        return MountKey::Unresolved;
    }
    mount_points(mountinfo)
        .into_iter()
        .filter(|point| path.starts_with(point))
        .max_by_key(|point| point.components().count())
        .map_or(MountKey::Unresolved, |point| {
            MountKey::Mount(point.to_string_lossy().into_owned())
        })
}

/// The mount points listed in `mountinfo` (field 5 of each line), unescaped.
/// A line with fewer fields is skipped.
fn mount_points(mountinfo: &str) -> Vec<PathBuf> {
    mountinfo
        .lines()
        .filter_map(|line| line.split(' ').nth(MOUNT_POINT_FIELD))
        .map(unescape_mount_point)
        .collect()
}

/// Undoes the kernel's `\NNN` octal escapes (space, tab, newline, backslash).
/// Anything that is not three octal digits after a backslash stays as it is.
fn unescape_mount_point(field: &str) -> PathBuf {
    let bytes = field.as_bytes();
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

/// The probes in flight, by track id.
#[derive(Debug, Default)]
pub(in crate::ui) struct ProbeSlots {
    in_flight: HashMap<i64, MountKey>,
}

impl ProbeSlots {
    /// Takes a slot for `id` on `mount`; false when `id` already has one, the
    /// mount is at [`MAX_FAULT_PROBES_PER_MOUNT`] or all mounts together are at
    /// [`MAX_FAULT_PROBES_TOTAL`]. A refused claim changes nothing.
    pub(in crate::ui) fn claim(&mut self, id: i64, mount: MountKey) -> bool {
        if self.in_flight.contains_key(&id) || self.in_flight.len() >= MAX_FAULT_PROBES_TOTAL {
            return false;
        }
        let on_mount = self
            .in_flight
            .values()
            .filter(|held| **held == mount)
            .count();
        if on_mount >= MAX_FAULT_PROBES_PER_MOUNT {
            return false;
        }
        self.in_flight.insert(id, mount);
        true
    }

    pub(in crate::ui) fn release(&mut self, id: i64) {
        self.in_flight.remove(&id);
    }
}
