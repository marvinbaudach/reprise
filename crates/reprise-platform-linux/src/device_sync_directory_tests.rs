use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};

use super::*;

#[test]
fn managed_write_names_destination_directory_creation_failures() {
    let (temp, storage) = fixture();
    if fs::metadata(temp.path()).unwrap().uid() == 0 {
        eprintln!("skipped permission-based test under a root test runner");
        return;
    }
    let managed_root = temp.path().join("Music/Reprise");
    fs::create_dir_all(&managed_root).unwrap();
    fs::write(temp.path().join("source.flac"), b"audio").unwrap();
    fs::set_permissions(&managed_root, fs::Permissions::from_mode(0o500)).unwrap();

    let result = run(storage.replace_managed(
        None,
        "/Music/Reprise",
        &gio::File::for_path(temp.path().join("source.flac")),
        "Blocked/song.flac",
        5,
        &gio::Cancellable::new(),
        |_, _| {},
    ));

    fs::set_permissions(&managed_root, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(matches!(
        &result,
        Err(DeviceIoError::DuringWrite {
            step: WriteStep::CreateDirectories,
            ..
        })
    ));
    assert!(result
        .unwrap_err()
        .to_string()
        .starts_with("creating the destination directory failed: device I/O failed:"));
}

#[test]
fn managed_write_adopts_a_resident_directory_that_only_differs_in_case() {
    let (temp, storage) = fixture();
    if fs::metadata(temp.path()).unwrap().uid() == 0 {
        eprintln!("skipped permission-based test under a root test runner");
        return;
    }
    let artist = temp.path().join("Music/Reprise/Emmure");
    let resident = artist.join("Speaker Of The Dead");
    fs::create_dir_all(&resident).unwrap();
    fs::write(temp.path().join("source.flac"), b"audio").unwrap();
    fs::set_permissions(&artist, fs::Permissions::from_mode(0o500)).unwrap();

    let outcome = run(storage.replace_managed(
        None,
        "/Music/Reprise",
        &gio::File::for_path(temp.path().join("source.flac")),
        "Emmure/Speaker of the Dead/13 song.flac",
        5,
        &gio::Cancellable::new(),
        |_, _| {},
    ));

    fs::set_permissions(&artist, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(
        outcome.unwrap(),
        CopyOutcome::Copied {
            relative_path: "Emmure/Speaker Of The Dead/13 song.flac".into()
        }
    );
    assert_eq!(fs::read(resident.join("13 song.flac")).unwrap(), b"audio");
    assert!(!artist.join("Speaker of the Dead").exists());
}

#[test]
fn managed_write_refuses_to_choose_between_two_fold_equal_directories() {
    let (temp, storage) = fixture();
    if fs::metadata(temp.path()).unwrap().uid() == 0 {
        eprintln!("skipped permission-based test under a root test runner");
        return;
    }
    let artist = temp.path().join("Music/Reprise/Emmure");
    fs::create_dir_all(artist.join("Speaker Of The Dead")).unwrap();
    fs::create_dir_all(artist.join("SPEAKER OF THE DEAD")).unwrap();
    fs::write(temp.path().join("source.flac"), b"audio").unwrap();
    fs::set_permissions(&artist, fs::Permissions::from_mode(0o500)).unwrap();

    let (result, warnings) = capture_warnings(|| {
        run(storage.replace_managed(
            None,
            "/Music/Reprise",
            &gio::File::for_path(temp.path().join("source.flac")),
            "Emmure/Speaker of the Dead/13 song.flac",
            5,
            &gio::Cancellable::new(),
            |_, _| {},
        ))
    });

    fs::set_permissions(&artist, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(matches!(
        result,
        Err(DeviceIoError::DuringWrite {
            step: WriteStep::CreateDirectories,
            ..
        })
    ));
    assert!(!artist
        .join("Speaker Of The Dead")
        .join("13 song.flac")
        .exists());
    assert!(!artist
        .join("SPEAKER OF THE DEAD")
        .join("13 song.flac")
        .exists());
    assert!(
        warnings.contains("refused to choose between fold-equal resident directories"),
        "{warnings}"
    );
}

#[test]
fn playlist_write_follows_the_resident_spelling_of_its_target_folder() {
    let (temp, storage) = fixture();
    if fs::metadata(temp.path()).unwrap().uid() == 0 {
        eprintln!("skipped permission-based test under a root test runner");
        return;
    }
    let music = temp.path().join("Music");
    let resident = music.join("REPRISE");
    fs::create_dir_all(&resident).unwrap();
    fs::set_permissions(&music, fs::Permissions::from_mode(0o500)).unwrap();

    let result =
        run(storage.replace_playlist(None, "/Music/Reprise", "Road", b"#EXTM3U\n".to_vec()));

    fs::set_permissions(&music, fs::Permissions::from_mode(0o700)).unwrap();
    result.unwrap();
    assert_eq!(
        fs::read_to_string(resident.join("Road.m3u8")).unwrap(),
        "#EXTM3U\n"
    );
    assert!(!music.join("Reprise").exists());
}

#[test]
fn pre_cancelled_copy_stops_before_touching_the_existing_target() {
    let (temp, storage) = fixture();
    fs::write(temp.path().join("source.flac"), vec![1_u8; 1024]).unwrap();
    let cancellable = gio::Cancellable::new();
    cancellable.cancel();
    let target = temp.path().join("Music/Reprise/Road/1-source.flac");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, b"old").unwrap();
    let result = run(storage.replace_managed(
        None,
        "/Music/Reprise",
        &gio::File::for_path(temp.path().join("source.flac")),
        "Road/1-source.flac",
        1024,
        &cancellable,
        |_copied, _total| {},
    ));
    assert!(matches!(
        &result,
        Err(DeviceIoError::DuringWrite {
            step: WriteStep::CreateDirectories,
            source,
        }) if matches!(source.as_ref(), DeviceIoError::Io(error) if error.matches(gio::IOErrorEnum::Cancelled))
    ));
    assert!(result
        .unwrap_err()
        .to_string()
        .starts_with("creating the destination directory failed: device I/O failed:"));
    assert_eq!(fs::read(target).unwrap(), b"old");
}

#[test]
fn cancelled_directory_creation_returns_before_the_retry_delay() {
    let (temp, storage) = fixture();
    if fs::metadata(temp.path()).unwrap().uid() == 0 {
        eprintln!("skipped permission-based test under a root test runner");
        return;
    }
    let managed_root = temp.path().join("Music/Reprise");
    fs::create_dir_all(&managed_root).unwrap();
    fs::write(temp.path().join("source.flac"), b"audio").unwrap();
    fs::set_permissions(&managed_root, fs::Permissions::from_mode(0o500)).unwrap();
    let cancellable = gio::Cancellable::new();
    cancellable.cancel();
    let started = std::time::Instant::now();

    let result = run(storage.replace_managed(
        None,
        "/Music/Reprise",
        &gio::File::for_path(temp.path().join("source.flac")),
        "Blocked/song.flac",
        5,
        &cancellable,
        |_, _| {},
    ));

    let elapsed = started.elapsed();
    fs::set_permissions(&managed_root, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(matches!(
        &result,
        Err(DeviceIoError::DuringWrite {
            step: WriteStep::CreateDirectories,
            source,
        }) if matches!(source.as_ref(), DeviceIoError::Io(error) if error.matches(gio::IOErrorEnum::Cancelled))
    ));
    assert!(
        elapsed < std::time::Duration::from_millis(200),
        "cancelled directory pass waited {elapsed:?}"
    );
}

#[test]
fn delete_track_is_scoped_to_the_managed_root_and_reports_absence() {
    let (temp, storage) = fixture();
    fs::create_dir_all(temp.path().join("Music/Reprise/Road")).unwrap();
    fs::write(
        temp.path().join("Music/Reprise/Road/finished.opus"),
        b"finished",
    )
    .unwrap();

    assert!(run(storage.delete_managed(None, "/Music/Reprise", "Road/finished.opus")).unwrap());
    assert!(!run(storage.delete_managed(None, "/Music/Reprise", "Road/finished.opus")).unwrap());
    assert!(matches!(
        run(storage.delete_managed(None, "/Music/Reprise", "../outside.opus")),
        Err(DeviceIoError::InvalidRelativePath)
    ));
}

#[test]
fn delete_track_adopts_the_resident_spelling_without_creating_directories() {
    let (temp, storage) = fixture();
    let resident = temp.path().join("Music/REPRISE/Road");
    fs::create_dir_all(&resident).unwrap();
    let track = resident.join("finished.opus");
    fs::write(&track, b"finished").unwrap();

    assert!(run(storage.delete_managed(None, "/Music/Reprise", "Road/finished.opus")).unwrap());
    assert!(!track.exists());
    assert!(!temp.path().join("Music/Reprise").exists());
}
