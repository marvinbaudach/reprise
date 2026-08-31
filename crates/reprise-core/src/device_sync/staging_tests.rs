use super::*;

#[test]
fn staged_bytes_land_at_a_path_that_reads_back_exactly() {
    let path = stage_bytes("device-7", 42, "analysis", b"encoded-sidecar").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"encoded-sidecar");
    discard(&path);
}

#[test]
fn two_stages_of_the_same_track_never_share_a_path() {
    let first = stage_bytes("device-7", 42, "analysis", b"first").unwrap();
    let second = stage_bytes("device-7", 42, "analysis", b"second").unwrap();
    assert_ne!(first, second);
    assert_eq!(std::fs::read(&first).unwrap(), b"first");
    assert_eq!(std::fs::read(&second).unwrap(), b"second");
    discard(&first);
    discard(&second);
}

#[test]
fn a_temporary_name_sanitizes_the_device_and_names_this_process() {
    let path = temporary_path("Pixel/7 ../Pro", 9, "opus");
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    assert_eq!(path.parent(), Some(staging_dir().as_path()));
    assert!(!name.contains('/'), "{name} carries a path separator");
    assert!(
        name.starts_with(&format!(
            "reprise-sync-{}-Pixel 7 .. Pro-9-",
            std::process::id(),
        )),
        "unexpected staged name {name}"
    );
    assert!(name.ends_with(".opus"), "unexpected staged name {name}");
}

#[test]
fn discarding_removes_the_file_and_forgives_one_that_is_already_gone() {
    let path = stage_bytes("device-7", 1, "track-metadata", b"list").unwrap();
    discard(&path);
    assert!(!path.exists());
    discard(&path);
}

#[test]
fn a_write_that_cannot_land_answers_with_an_error_and_no_path() {
    // The extension is the only part of a staged name a caller controls, so
    // it is also the only way a test can aim the write at a directory that
    // does not exist. What matters is the answer, not the odd input: an
    // `Err` carries no path, so a caller has nothing to hand to a backend.
    let failure = stage_bytes("device-7", 42, "no-such-directory/analysis", b"encoded");
    assert!(failure.is_err(), "the write should not have succeeded");
}

#[test]
fn storage_full_names_the_local_staging_directory_not_the_device() {
    let directory = std::env::temp_dir().join("reprise-staging-full-test");
    let error = stage_bytes_with(
        &directory,
        "device-7",
        42,
        "analysis",
        b"encoded",
        |_, _| Err(std::io::Error::from(std::io::ErrorKind::StorageFull)),
    )
    .unwrap_err();
    let message = error.to_string();

    assert!(message.contains("local staging directory"), "{message}");
    assert!(
        message.contains(&directory.display().to_string()),
        "{message}"
    );
    assert!(!message.to_lowercase().contains("device"), "{message}");
}
