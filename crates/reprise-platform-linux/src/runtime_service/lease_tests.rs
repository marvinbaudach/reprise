//! The lease's one promise: at most one holder, and losing is immediate.

use std::path::Path;

use super::{lease_path, LeaseError, RuntimeLease};

fn lease_in(directory: &Path) -> std::path::PathBuf {
    lease_path(directory)
}

#[test]
fn the_lease_lives_in_a_reprise_subdirectory_of_the_runtime_dir() {
    let path = lease_path(Path::new("/run/user/1000"));

    assert_eq!(path, Path::new("/run/user/1000/reprise/runtime.lock"));
}

#[test]
fn run_1_a_second_claim_loses_immediately_instead_of_waiting() {
    let directory = tempfile::tempdir().expect("a temporary runtime directory");
    let path = lease_in(directory.path());
    let _held = RuntimeLease::claim_at(&path).expect("the first claim wins");

    let second = RuntimeLease::claim_at(&path);

    assert!(
        matches!(second, Err(LeaseError::Held)),
        "a loser must find out at once and exit; waiting for a runtime it is \
         not going to become is the deadlock this design avoids"
    );
}

#[test]
fn releasing_the_lease_lets_the_next_process_claim_it() {
    let directory = tempfile::tempdir().expect("a temporary runtime directory");
    let path = lease_in(directory.path());

    let held = RuntimeLease::claim_at(&path).expect("the first claim wins");
    drop(held);

    RuntimeLease::claim_at(&path).expect("the lease is free again");
}

#[test]
fn the_lease_file_records_the_owner_for_diagnostics() {
    let directory = tempfile::tempdir().expect("a temporary runtime directory");
    let path = lease_in(directory.path());

    let held = RuntimeLease::claim_at(&path).expect("the claim wins");

    let contents = std::fs::read_to_string(held.path()).expect("the lease file is readable");
    assert!(contents.contains(&format!("pid={}", std::process::id())));
    assert!(contents.contains(&format!(
        "protocol={}",
        reprise_runtime_protocol::PROTOCOL_VERSION
    )));
}

#[test]
fn a_loser_does_not_truncate_the_owners_diagnostics() {
    let directory = tempfile::tempdir().expect("a temporary runtime directory");
    let path = lease_in(directory.path());
    let held = RuntimeLease::claim_at(&path).expect("the first claim wins");
    let before = std::fs::read_to_string(held.path()).expect("readable");

    let _ = RuntimeLease::claim_at(&path);

    assert_eq!(
        std::fs::read_to_string(held.path()).expect("still readable"),
        before,
        "the file is written only after the lock is won, so a losing process \
         cannot leave the owner's entry empty"
    );
}

#[test]
fn the_lease_creates_its_directory_rather_than_requiring_one() {
    let directory = tempfile::tempdir().expect("a temporary runtime directory");
    let path = lease_in(&directory.path().join("nested"));

    RuntimeLease::claim_at(&path).expect("the claim creates what it needs");

    assert!(path.exists());
}
