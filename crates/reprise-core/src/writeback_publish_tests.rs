use std::fs;

use tempfile::TempDir;

use super::*;

#[test]
fn publication_writes_the_payload_and_leaves_no_temporary_file() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("cover.jpg");

    assert_eq!(publish(&target, b"payload").unwrap(), Published::Written);
    assert_eq!(fs::read(&target).unwrap(), b"payload");
    assert_eq!(directory_entries(dir.path()), ["cover.jpg"]);
}

#[test]
fn publication_never_replaces_a_file_that_appeared_after_the_check() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("cover.jpg");
    fs::write(&target, b"the user's own file").unwrap();

    assert_eq!(
        publish(&target, b"payload").unwrap(),
        Published::AlreadyPresent
    );
    assert_eq!(fs::read(&target).unwrap(), b"the user's own file");
    assert_eq!(directory_entries(dir.path()), ["cover.jpg"]);
}

/// What Linux vfat/exfat/ntfs3 and the FUSE MTP mounts answer `link(2)` with.
fn no_hard_links(_: &std::path::Path, _: &std::path::Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "Operation not permitted",
    ))
}

#[test]
fn a_filesystem_without_hard_links_still_publishes_the_payload() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("cover.jpg");

    assert_eq!(
        publish_with(&target, b"payload", no_hard_links).unwrap(),
        Published::Written
    );
    assert_eq!(fs::read(&target).unwrap(), b"payload");
    assert_eq!(directory_entries(dir.path()), ["cover.jpg"]);
}

#[test]
fn the_no_hard_link_fallback_still_never_replaces_an_existing_file() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("cover.jpg");
    fs::write(&target, b"the user's own file").unwrap();

    assert_eq!(
        publish_with(&target, b"payload", no_hard_links).unwrap(),
        Published::AlreadyPresent
    );
    assert_eq!(fs::read(&target).unwrap(), b"the user's own file");
    assert_eq!(directory_entries(dir.path()), ["cover.jpg"]);
}

#[test]
fn a_long_target_name_still_leaves_room_for_the_temporary_file() {
    let dir = TempDir::new().unwrap();
    // 249 bytes: inside NAME_MAX, and an ordinary length for a classical or
    // live-set track. A temporary derived from this name would not be.
    let target = dir.path().join(format!("{}.lrc", "a".repeat(245)));

    assert_eq!(publish(&target, b"payload").unwrap(), Published::Written);
    assert_eq!(fs::read(&target).unwrap(), b"payload");
}

#[test]
fn a_publication_suppresses_the_library_watcher_for_every_path_it_touches() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("cover.jpg");

    assert_eq!(publish(&target, b"payload").unwrap(), Published::Written);

    assert!(
        crate::library::watcher::is_ignored(&target),
        "the published file must not re-arm the watcher's debounce"
    );
    let temporaries = crate::library::watcher::ignored_paths()
        .into_iter()
        .filter(|path| path.parent() == Some(dir.path()) && path != &target)
        .collect::<Vec<_>>();
    assert_eq!(
        temporaries.len(),
        1,
        "the temporary file's create/modify/delete events must be ignored too, got {temporaries:?}"
    );
}

#[cfg(unix)]
#[test]
fn a_failing_publication_leaves_no_temporary_file() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();

    let result = publish(&dir.path().join("cover.jpg"), b"payload");

    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(result.is_err());
    assert!(directory_entries(dir.path()).is_empty());
}

fn directory_entries(dir: &std::path::Path) -> Vec<String> {
    let mut names = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    names.sort();
    names
}
