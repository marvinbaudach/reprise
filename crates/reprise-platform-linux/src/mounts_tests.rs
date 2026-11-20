//! The mountinfo lookup.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use super::mount_point_in;

const MOUNTINFO: &str = "\
22 28 0:21 / /sys rw,nosuid shared:7 - sysfs sysfs rw
28 1 8:2 / / rw,relatime shared:1 - ext4 /dev/sda2 rw
40 28 0:35 / /mnt/nas rw,relatime shared:20 - nfs4 nas:/export rw
41 28 0:36 / /mnt/nas2 rw,relatime shared:21 - nfs4 nas:/export2 rw
42 40 0:37 / /mnt/nas/deep rw,relatime shared:22 - nfs4 nas:/deep rw
43 28 8:3 / /run/media/me/My\\040Disk\\011Tab rw,relatime - vfat /dev/sdb1 rw
44 28 8:4 / /data/back\\134slash rw,relatime - ext4 /dev/sdb2 rw
";

fn mount(point: &str) -> Option<PathBuf> {
    Some(PathBuf::from(point))
}

fn on(path: &str) -> Option<PathBuf> {
    mount_point_in(MOUNTINFO.as_bytes(), Path::new(path))
}

#[test]
fn a_path_is_on_the_longest_mount_point_that_prefixes_it() {
    assert_eq!(on("/mnt/nas/music/a.flac"), mount("/mnt/nas"));
    assert_eq!(on("/mnt/nas/deep/music/a.flac"), mount("/mnt/nas/deep"));
    assert_eq!(on("/home/me/Music/a.flac"), mount("/"));
}

#[test]
fn a_mount_point_matches_on_whole_path_components_only() {
    assert_eq!(on("/mnt/nas2/x.flac"), mount("/mnt/nas2"));
    assert_eq!(on("/mnt/nas3/x.flac"), mount("/"));
    assert_eq!(on("/mnt/nas"), mount("/mnt/nas"));
}

#[test]
fn octal_escapes_in_a_mount_point_are_undone() {
    assert_eq!(
        on("/run/media/me/My Disk\tTab/a.flac"),
        mount("/run/media/me/My Disk\tTab")
    );
    assert_eq!(on("/data/back\\slash/a.flac"), mount("/data/back\\slash"));
}

#[test]
fn a_non_utf8_line_elsewhere_does_not_hide_the_valid_mounts() {
    let mut mountinfo = MOUNTINFO.as_bytes().to_vec();
    mountinfo.extend_from_slice(b"45 28 8:5 / /mnt/caf\xe9 rw - ext4 /dev/s\xffdc rw\n");
    let valid = |path: &str| mount_point_in(&mountinfo, Path::new(path));
    assert_eq!(valid("/mnt/nas/music/a.flac"), mount("/mnt/nas"));
    assert_eq!(valid("/home/me/Music/a.flac"), mount("/"));
}

#[test]
fn a_non_utf8_mount_point_is_kept_byte_for_byte() {
    let mountinfo = b"28 1 8:2 / / rw - ext4 /dev/sda2 rw\n\
45 28 8:5 / /mnt/caf\xe9 rw - ext4 /dev/sdc rw\n\
46 28 8:6 / /mnt/caf\xc3\xa9 rw - ext4 /dev/sdd rw\n";
    let latin1 = PathBuf::from(OsStr::from_bytes(b"/mnt/caf\xe9"));
    let utf8 = PathBuf::from("/mnt/caf\u{e9}");
    assert_eq!(
        mount_point_in(mountinfo, &latin1.join("a.flac")),
        Some(latin1)
    );
    assert_eq!(
        mount_point_in(mountinfo, &utf8.join("a.flac")),
        Some(utf8),
        "two mounts that differ in bytes stay two mount points"
    );
}

#[test]
fn a_path_that_cannot_be_placed_falls_under_the_shared_key() {
    assert_eq!(on("relative/a.flac"), None);
    assert_eq!(on("/mnt/nas/../nas2/a.flac"), None);
    assert_eq!(mount_point_in(b"", Path::new("/mnt/nas/a.flac")), None);
    assert_eq!(
        mount_point_in(b"not mountinfo at all\n\ngarbage", Path::new("/a.flac")),
        None
    );
    // No mount point covers the path (no "/" entry): no mount either.
    let no_root = "40 28 0:35 / /mnt/nas rw - nfs4 nas:/export rw\n";
    assert_eq!(
        mount_point_in(no_root.as_bytes(), Path::new("/home/a.flac")),
        None
    );
}

#[test]
fn an_entry_whose_mount_point_is_not_absolute_is_ignored() {
    // Two spaces leave the mount-point field empty; an empty path is a prefix
    // of every path and must not become a key.
    let empty_field = b"28 1 8:2 /  rw - ext4 /dev/sda2 rw\n";
    assert_eq!(mount_point_in(empty_field, Path::new("/a.flac")), None);
    let relative = b"28 1 8:2 / mnt/nas rw - ext4 /dev/sda2 rw\n";
    assert_eq!(mount_point_in(relative, Path::new("/mnt/nas/a.flac")), None);
    // A good entry beside the garbage one still wins.
    let mixed = b"28 1 8:2 /  rw - ext4 /dev/sda2 rw\n\
29 1 8:3 / / rw - ext4 /dev/sda3 rw\n";
    assert_eq!(mount_point_in(mixed, Path::new("/a.flac")), mount("/"));
}
