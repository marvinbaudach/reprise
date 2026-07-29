//! Design 7d's device folder browser (`MTP-31`/`MTP-32`).
//!
//! Split out of `device_sync_tests.rs` when the dev merge pushed that file past
//! the 800-line gate. Every operation here runs against a local temp directory
//! standing in for the device root - no real or simulated phone.

use std::fs;

use reprise_core::device_sync::browser::StorageKind;

use super::target_browser::derive_storage_id;
use super::tests::{fixture, run};
use super::*;

// Design 7d's device folder browser (`MTP-31`/`MTP-32`). Every operation
// below runs against a local temp directory standing in for the device
// root, exactly like the fixtures above — no real or simulated phone.

#[test]
fn derive_storage_id_is_stable_for_the_same_name_and_differs_for_different_names() {
    assert_eq!(
        derive_storage_id("Internal shared storage"),
        derive_storage_id("Internal shared storage")
    );
    assert_ne!(
        derive_storage_id("Internal shared storage"),
        derive_storage_id("SD card")
    );
}

#[test]
fn browser_lists_storage_volumes_classified_internal_and_removable() {
    let (temp, storage) = fixture();
    fs::create_dir_all(temp.path().join("Internal shared storage")).unwrap();
    fs::create_dir_all(temp.path().join("SD card")).unwrap();
    fs::write(temp.path().join("not-a-volume.txt"), b"ignore").unwrap();

    let mut volumes = run(storage.list_storage_volumes()).unwrap();
    volumes.sort_by(|left, right| left.name.cmp(&right.name));

    assert_eq!(
        volumes.len(),
        2,
        "the loose file must not appear as a volume"
    );
    assert_eq!(volumes[0].name, "Internal shared storage");
    assert_eq!(volumes[0].kind, StorageKind::Internal);
    assert_eq!(volumes[1].name, "SD card");
    assert_eq!(volumes[1].kind, StorageKind::Removable);
}

#[test]
fn browser_lists_only_immediate_child_folders_of_a_path() {
    let (temp, storage) = fixture();
    fs::create_dir_all(temp.path().join("Internal/Music/Reprise")).unwrap();
    fs::create_dir_all(temp.path().join("Internal/Music/Podcasts")).unwrap();
    fs::write(temp.path().join("Internal/Music/loose.mp3"), b"audio").unwrap();
    let internal_id = derive_storage_id("Internal");

    let mut folders = run(storage.list_child_folders(internal_id, "/Music")).unwrap();
    folders.sort();

    assert_eq!(folders, vec!["Podcasts".to_string(), "Reprise".to_string()]);
}

#[test]
fn browser_creates_a_new_folder_and_rejects_a_duplicate_name() {
    let (temp, storage) = fixture();
    fs::create_dir_all(temp.path().join("Internal/Music")).unwrap();
    let internal_id = derive_storage_id("Internal");

    run(storage.create_child_folder(internal_id, "/Music", "Reprise-YouTube")).unwrap();
    assert!(temp.path().join("Internal/Music/Reprise-YouTube").is_dir());

    let duplicate = run(storage.create_child_folder(internal_id, "/Music", "Reprise-YouTube"));
    assert!(matches!(duplicate, Err(DeviceIoError::FolderAlreadyExists)));
}

#[test]
fn browser_reports_a_distinct_error_when_a_device_refuses_creation_at_the_storage_root() {
    let (temp, storage) = fixture();
    let root = temp.path().join("Locked");
    fs::create_dir_all(&root).unwrap();
    let mut permissions = fs::metadata(&root).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&root, permissions).unwrap();
    let locked_id = derive_storage_id("Locked");

    let result = run(storage.create_child_folder(locked_id, "", "Music"));

    fs::set_permissions(&root, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();

    assert!(
        matches!(result, Err(DeviceIoError::CannotCreateAtStorageRoot(_))),
        "expected the root-creation refusal to surface distinctly, got {result:?}"
    );
}

#[test]
fn browser_moves_a_folder_and_its_contents_to_a_new_path_on_the_same_storage() {
    let (temp, storage) = fixture();
    fs::create_dir_all(temp.path().join("Internal/Music/Reprise-YouTube")).unwrap();
    fs::write(
        temp.path().join("Internal/Music/Reprise-YouTube/song.opus"),
        b"audio",
    )
    .unwrap();
    let internal_id = derive_storage_id("Internal");

    run(storage.move_child_folder(internal_id, "/Music/Reprise-YouTube", "/Music/YT")).unwrap();

    assert!(!temp.path().join("Internal/Music/Reprise-YouTube").exists());
    assert!(temp.path().join("Internal/Music/YT/song.opus").is_file());
}
