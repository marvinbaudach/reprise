use std::path::Path;

use super::{is_within, paths_within_temp_root, Unresolvable};

#[test]
fn a_resolvable_path_inside_the_root_is_within_it_under_either_policy() {
    let root = tempfile::tempdir().unwrap();
    let inside = root.path().join("inside.flac");
    std::fs::write(&inside, b"scratch").unwrap();

    assert!(is_within(root.path(), &inside, Unresolvable::Outside));
    assert!(is_within(
        root.path(),
        &inside,
        Unresolvable::CompareAsWritten
    ));
}

#[test]
fn a_resolvable_path_beside_the_root_is_outside_it_under_either_policy() {
    let root = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let outside = elsewhere.path().join("outside.flac");
    std::fs::write(&outside, b"scratch").unwrap();

    assert!(!is_within(root.path(), &outside, Unresolvable::Outside));
    assert!(!is_within(
        root.path(),
        &outside,
        Unresolvable::CompareAsWritten
    ));
}

#[test]
fn an_unresolvable_path_is_refused_by_the_guard_and_compared_as_written_by_the_warning() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let missing_inside = root.join("missing.flac");
    let missing_outside = root.parent().unwrap().join("no-such-tree/gone.flac");

    assert!(!is_within(&root, &missing_inside, Unresolvable::Outside));
    assert!(is_within(
        &root,
        &missing_inside,
        Unresolvable::CompareAsWritten
    ));
    assert!(!is_within(
        &root,
        &missing_outside,
        Unresolvable::CompareAsWritten
    ));
}

#[test]
fn an_unresolvable_root_splits_the_two_policies_the_same_way() {
    let directory = tempfile::tempdir().unwrap();
    let missing_root = directory.path().join("never-created");
    let below = missing_root.join("track.flac");

    assert!(!is_within(&missing_root, &below, Unresolvable::Outside));
    assert!(is_within(
        &missing_root,
        &below,
        Unresolvable::CompareAsWritten
    ));
}

#[cfg(unix)]
#[test]
fn containment_follows_a_symlink_instead_of_the_name_it_was_given() {
    let root = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let target = elsewhere.path().join("secret.flac");
    std::fs::write(&target, b"scratch").unwrap();
    let link = root.path().join("looks-inside.flac");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    assert!(!is_within(root.path(), &link, Unresolvable::Outside));
    assert!(!is_within(
        root.path(),
        &link,
        Unresolvable::CompareAsWritten
    ));
}

#[test]
fn smoke_guard_accepts_only_existing_files_inside_temporary_root() {
    let root = tempfile::tempdir().unwrap();
    let inside = root.path().join("inside.flac");
    std::fs::write(&inside, b"scratch").unwrap();
    let outside_root = tempfile::tempdir().unwrap();
    let outside = outside_root.path().join("outside.flac");
    std::fs::write(&outside, b"scratch").unwrap();

    assert!(paths_within_temp_root(
        root.path(),
        std::slice::from_ref(&inside),
    ));
    assert!(!paths_within_temp_root(root.path(), &[inside, outside]));
    assert!(!paths_within_temp_root(
        root.path(),
        &[root.path().join("missing.flac")],
    ));
}

#[test]
fn smoke_guard_refuses_a_root_outside_the_system_temporary_directory() {
    // The filesystem root contains every existing file, so only the
    // temporary-directory clause can reject this.
    let file_in_temp = tempfile::NamedTempFile::new().unwrap();
    let paths = vec![file_in_temp.path().to_path_buf()];

    assert!(!paths_within_temp_root(Path::new("/"), &paths));
}
