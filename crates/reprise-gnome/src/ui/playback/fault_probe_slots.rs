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
//! the old global bound.
//!
//! Known limitation: the lookup never follows symlinks, because resolving one
//! would ask the mount. A path is therefore keyed to the mount that holds the
//! path as written. A library reached through a symlink such as
//! `~/Music -> /mnt/nas/Music` is keyed to the mount of `~/Music` (the home
//! disk), not to the NAS it points at. Its probes then share a key with
//! everything else on the home disk, so a dead NAS can use up that key's slots
//! and starve a healthy file on the same disk. For those paths the home-disk key
//! acts like the old shared bound, no worse than before.

use std::collections::HashMap;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

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
    /// The mount point, unescaped, byte for byte.
    Mount(PathBuf),
    /// Every path that could not be placed shares this one key.
    Unresolved,
}

/// Maps a track path to its mount on the main thread; a test replaces it to
/// put paths on different "mounts".
pub(in crate::ui) type MountResolver = Rc<dyn Fn(&Path) -> MountKey>;

/// Set once the unreadable-mountinfo warning has been logged, so a mountinfo
/// that stays unreadable does not log on every fault.
static UNREADABLE_LOGGED: AtomicBool = AtomicBool::new(false);

/// The production resolver: `/proc/self/mountinfo`, read on each call so a
/// mount made since startup is seen. It is read as bytes: the kernel escapes
/// only space, tab, newline and backslash, so a mount point or device name may
/// legally hold other non-UTF-8 bytes.
pub(in crate::ui) fn procfs_mount_resolver() -> MountResolver {
    Rc::new(|path| match std::fs::read("/proc/self/mountinfo") {
        Ok(mountinfo) => mount_of(&mountinfo, path),
        Err(error) => {
            if !UNREADABLE_LOGGED.swap(true, Ordering::Relaxed) {
                tracing::warn!(%error, "mountinfo is unreadable; probes share one bound");
            }
            MountKey::Unresolved
        }
    })
}

/// The mount of `path` among the mount points in `mountinfo`: the longest one
/// that prefixes it. The lines are walked once and only the best match is kept.
pub(in crate::ui) fn mount_of(mountinfo: &[u8], path: &Path) -> MountKey {
    if !path.is_absolute() || path.components().any(|part| part == Component::ParentDir) {
        return MountKey::Unresolved;
    }
    mountinfo
        .split(|byte| *byte == b'\n')
        .filter_map(|line| line.split(|byte| *byte == b' ').nth(MOUNT_POINT_FIELD))
        .map(unescape_mount_point)
        .filter(|point| path.starts_with(point))
        .max_by_key(|point| point.components().count())
        .map_or(MountKey::Unresolved, MountKey::Mount)
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

/// The probes in flight, by track id.
#[derive(Debug, Default)]
pub(in crate::ui) struct ProbeSlots {
    in_flight: HashMap<i64, MountKey>,
    /// How many of `in_flight` sit on each mount; a mount with none has no entry.
    per_mount: HashMap<MountKey, usize>,
}

impl ProbeSlots {
    /// Takes a slot for `id` on `mount`; false when `id` already has one, the
    /// mount is at [`MAX_FAULT_PROBES_PER_MOUNT`] or all mounts together are at
    /// [`MAX_FAULT_PROBES_TOTAL`]. A refused claim changes nothing.
    pub(in crate::ui) fn claim(&mut self, id: i64, mount: MountKey) -> bool {
        if self.in_flight.contains_key(&id) || self.in_flight.len() >= MAX_FAULT_PROBES_TOTAL {
            return false;
        }
        if self.held_on(&mount) >= MAX_FAULT_PROBES_PER_MOUNT {
            return false;
        }
        *self.per_mount.entry(mount.clone()).or_default() += 1;
        self.in_flight.insert(id, mount);
        true
    }

    pub(in crate::ui) fn release(&mut self, id: i64) {
        let Some(mount) = self.in_flight.remove(&id) else {
            return;
        };
        if let Some(on_mount) = self.per_mount.get_mut(&mount) {
            *on_mount = on_mount.saturating_sub(1);
            if *on_mount == 0 {
                self.per_mount.remove(&mount);
            }
        }
    }

    /// The probes held on `mount`.
    pub(in crate::ui) fn held_on(&self, mount: &MountKey) -> usize {
        self.per_mount.get(mount).copied().unwrap_or(0)
    }
}
