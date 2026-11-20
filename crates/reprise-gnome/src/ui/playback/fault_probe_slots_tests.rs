//! The probe ledger. The mountinfo lookup behind the production resolver is
//! tested in `reprise-platform-linux` (`mounts_tests.rs`).

use std::path::PathBuf;

use super::fault_probe_slots::{
    MountKey, ProbeSlots, MAX_FAULT_PROBES_PER_MOUNT, MAX_FAULT_PROBES_TOTAL,
};

fn mount(point: &str) -> MountKey {
    MountKey::Mount(PathBuf::from(point))
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
