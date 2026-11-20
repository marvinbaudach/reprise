//! The probe ledger and the mountinfo lookup behind it.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use super::fault_probe_slots::{
    mount_of, MountKey, ProbeSlots, MAX_FAULT_PROBES_PER_MOUNT, MAX_FAULT_PROBES_TOTAL,
};

const MOUNTINFO: &str = "\
22 28 0:21 / /sys rw,nosuid shared:7 - sysfs sysfs rw
28 1 8:2 / / rw,relatime shared:1 - ext4 /dev/sda2 rw
40 28 0:35 / /mnt/nas rw,relatime shared:20 - nfs4 nas:/export rw
41 28 0:36 / /mnt/nas2 rw,relatime shared:21 - nfs4 nas:/export2 rw
42 40 0:37 / /mnt/nas/deep rw,relatime shared:22 - nfs4 nas:/deep rw
43 28 8:3 / /run/media/me/My\\040Disk\\011Tab rw,relatime - vfat /dev/sdb1 rw
44 28 8:4 / /data/back\\134slash rw,relatime - ext4 /dev/sdb2 rw
";

fn mount(point: &str) -> MountKey {
    MountKey::Mount(PathBuf::from(point))
}

fn on(path: &str) -> MountKey {
    mount_of(MOUNTINFO.as_bytes(), Path::new(path))
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
    let valid = |path: &str| mount_of(&mountinfo, Path::new(path));
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
        mount_of(mountinfo, &latin1.join("a.flac")),
        MountKey::Mount(latin1)
    );
    assert_eq!(
        mount_of(mountinfo, &utf8.join("a.flac")),
        MountKey::Mount(utf8),
        "two mounts that differ in bytes stay two keys"
    );
}

#[test]
fn a_path_that_cannot_be_placed_falls_under_the_shared_key() {
    assert_eq!(on("relative/a.flac"), MountKey::Unresolved);
    assert_eq!(on("/mnt/nas/../nas2/a.flac"), MountKey::Unresolved);
    assert_eq!(
        mount_of(b"", Path::new("/mnt/nas/a.flac")),
        MountKey::Unresolved
    );
    assert_eq!(
        mount_of(b"not mountinfo at all\n\ngarbage", Path::new("/a.flac")),
        MountKey::Unresolved
    );
    // No mount point covers the path (no "/" entry): same shared key.
    let no_root = "40 28 0:35 / /mnt/nas rw - nfs4 nas:/export rw\n";
    assert_eq!(
        mount_of(no_root.as_bytes(), Path::new("/home/a.flac")),
        MountKey::Unresolved
    );
}

#[test]
fn a_mount_accepts_its_bound_and_refuses_the_next_probe() {
    let mut slots = ProbeSlots::default();
    let dead = mount("/mnt/nas");
    for id in 0..MAX_FAULT_PROBES_PER_MOUNT as i64 {
        assert!(slots.claim(id, dead.clone()));
    }
    assert!(!slots.claim(100, dead));
}

#[test]
fn a_full_mount_does_not_block_another_mount() {
    let mut slots = ProbeSlots::default();
    for id in 0..MAX_FAULT_PROBES_PER_MOUNT as i64 {
        assert!(slots.claim(id, mount("/mnt/nas")));
    }
    assert!(slots.claim(100, mount("/")));
}

#[test]
fn a_track_already_probed_is_refused_even_with_a_free_slot() {
    let mut slots = ProbeSlots::default();
    assert!(slots.claim(7, mount("/")));
    assert!(!slots.claim(7, mount("/")));
}

#[test]
fn a_refused_claim_leaves_the_probe_it_collided_with_in_place() {
    let mut slots = ProbeSlots::default();
    assert!(slots.claim(7, mount("/mnt/nas")));
    assert!(!slots.claim(7, mount("/")));
    // Still holding its /mnt/nas slot: one more fits there, not two.
    assert!(slots.claim(8, mount("/mnt/nas")));
    assert!(!slots.claim(9, mount("/mnt/nas")));
}

#[test]
fn releasing_a_probe_frees_its_slot_and_its_id() {
    let mut slots = ProbeSlots::default();
    let dead = mount("/mnt/nas");
    for id in 0..MAX_FAULT_PROBES_PER_MOUNT as i64 {
        assert!(slots.claim(id, dead.clone()));
    }
    slots.release(0);
    assert!(slots.claim(0, dead.clone()), "the id can be probed again");
    assert!(!slots.claim(100, dead));
}

#[test]
fn the_total_ceiling_holds_across_mounts() {
    let mut slots = ProbeSlots::default();
    let mounts = MAX_FAULT_PROBES_TOTAL.div_ceil(MAX_FAULT_PROBES_PER_MOUNT);
    let mut id = 0;
    for point in 0..mounts {
        for _ in 0..MAX_FAULT_PROBES_PER_MOUNT {
            if id < MAX_FAULT_PROBES_TOTAL as i64 {
                assert!(slots.claim(id, mount(&format!("/m{point}"))));
                id += 1;
            }
        }
    }
    assert!(!slots.claim(1000, mount("/one-more-mount")));
}

#[test]
fn releasing_a_probe_lowers_its_mounts_count_and_no_other() {
    let mut slots = ProbeSlots::default();
    assert!(slots.claim(1, mount("/mnt/nas")));
    assert!(slots.claim(2, mount("/mnt/nas")));
    assert!(slots.claim(3, mount("/")));
    assert_eq!(slots.held_on(&mount("/mnt/nas")), 2);
    slots.release(1);
    assert_eq!(slots.held_on(&mount("/mnt/nas")), 1);
    assert_eq!(slots.held_on(&mount("/")), 1);
    slots.release(2);
    slots.release(2);
    assert_eq!(slots.held_on(&mount("/mnt/nas")), 0);
}

#[test]
fn a_refused_claim_adds_nothing_to_the_count() {
    let mut slots = ProbeSlots::default();
    for id in 0..MAX_FAULT_PROBES_PER_MOUNT as i64 {
        assert!(slots.claim(id, mount("/mnt/nas")));
    }
    assert!(!slots.claim(100, mount("/mnt/nas")));
    assert_eq!(
        slots.held_on(&mount("/mnt/nas")),
        MAX_FAULT_PROBES_PER_MOUNT
    );
}

#[test]
fn releasing_an_unknown_id_is_harmless() {
    let mut slots = ProbeSlots::default();
    slots.release(5);
    assert!(slots.claim(5, mount("/")));
}
