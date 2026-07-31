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
