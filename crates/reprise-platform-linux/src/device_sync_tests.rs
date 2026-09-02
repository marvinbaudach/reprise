use std::cell::RefCell;
use std::fs;
use std::future::Future;
use std::io::Write;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use reprise_core::device_sync::{
    DeviceStorageAccess, ManagedDeviceFile, StorageId, SyncTarget, DEFAULT_TARGET_PATH,
};
use reprise_core::library::m3u::M3uEntry;
use tempfile::TempDir;

use super::inspection::storage_access_from_attributes;
use super::target_browser::derive_storage_id;
use super::*;

#[derive(Clone, Default)]
struct CapturedWarnings(Arc<Mutex<Vec<u8>>>);

struct WarningWriter(Arc<Mutex<Vec<u8>>>);

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CapturedWarnings {
    type Writer = WarningWriter;

    fn make_writer(&'a self) -> Self::Writer {
        WarningWriter(Arc::clone(&self.0))
    }
}

impl Write for WarningWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn capture_warnings<T>(operation: impl FnOnce() -> T) -> (T, String) {
    let output = CapturedWarnings::default();
    let subscriber = tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_target(false)
        .with_max_level(tracing::Level::WARN)
        .with_writer(output.clone())
        .finish();
    let result = tracing::subscriber::with_default(subscriber, operation);
    let bytes = output.0.lock().unwrap().clone();
    (result, String::from_utf8(bytes).unwrap())
}

pub(super) fn run<T>(future: impl Future<Output = T>) -> T {
    let context = gio::glib::MainContext::new();
    context
        .with_thread_default(|| context.block_on(future))
        .unwrap()
}

pub(super) fn fixture() -> (TempDir, DeviceStorage) {
    let temp = tempfile::tempdir().unwrap();
    let root = gio::File::for_path(temp.path());
    (temp, DeviceStorage::from_root(&root))
}

fn default_target() -> SyncTarget {
    SyncTarget::default()
}

fn target_at(storage_id: StorageId, path: &str) -> SyncTarget {
    SyncTarget {
        storage_id: Some(storage_id),
        path: path.to_string(),
        ..SyncTarget::default()
    }
}

#[test]
fn missing_music_directory_inspects_as_empty() {
    let (_temp, storage) = fixture();
    let inspection = run(storage.inspect(&default_target())).unwrap();
    assert!(inspection.managed_files.is_empty());
    assert_eq!(inspection.snapshot.reprise_music_bytes, 0);
    assert_eq!(inspection.snapshot.other_music_bytes, 0);
    assert!(inspection.snapshot.free_bytes.is_some());
    assert!(inspection.snapshot.total_bytes.is_some());
}

#[test]
fn storage_access_prefers_read_only_evidence_and_preserves_unknowns() {
    assert_eq!(
        storage_access_from_attributes(Some(true), Some(true)),
        DeviceStorageAccess::ReadOnly
    );
    assert_eq!(
        storage_access_from_attributes(Some(false), Some(false)),
        DeviceStorageAccess::ReadOnly
    );
    assert_eq!(
        storage_access_from_attributes(None, Some(true)),
        DeviceStorageAccess::Writable
    );
    assert_eq!(
        storage_access_from_attributes(Some(false), None),
        DeviceStorageAccess::Unknown
    );
    assert_eq!(
        storage_access_from_attributes(None, None),
        DeviceStorageAccess::Unknown
    );
}

#[test]
fn inspection_aggregates_music_and_returns_every_authoritative_reprise_file() {
    let (temp, storage) = fixture();
    fs::create_dir_all(temp.path().join("Music/Reprise/Road")).unwrap();
    fs::write(temp.path().join("Music/Reprise/Road/1-song.flac"), b"audio").unwrap();
    fs::write(temp.path().join("Music/loose.mp3"), b"audio2").unwrap();
    fs::write(temp.path().join("Music/notes.txt"), b"ignore").unwrap();
    fs::write(
        temp.path().join("Music/Reprise/Road.m3u8"),
        b"#EXTM3U\nRoad/1-song.flac\n",
    )
    .unwrap();

    let inspection = run(storage.inspect(&default_target())).unwrap();
    let paths = inspection
        .managed_files
        .iter()
        .map(|file| file.relative_path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(paths, ["Road.m3u8", "Road/1-song.flac"]);
    assert_eq!(inspection.snapshot.reprise_music_bytes, 30);
    assert_eq!(inspection.snapshot.other_music_bytes, 6);
    assert_eq!(
        inspection.snapshot.target_name.as_deref(),
        temp.path().file_name().and_then(|name| name.to_str())
    );
}

#[test]
fn managed_readback_keeps_byte_preserved_originals_with_unlisted_extensions() {
    let (temp, storage) = fixture();
    fs::create_dir_all(temp.path().join("Music/Reprise/Artist/Album")).unwrap();
    fs::write(
        temp.path()
            .join("Music/Reprise/Artist/Album/01 Original.aiff"),
        b"original",
    )
    .unwrap();
    fs::write(
        temp.path().join("Music/Reprise/Originals.m3u8"),
        b"#EXTM3U\nArtist/Album/01 Original.aiff\n",
    )
    .unwrap();
    fs::write(
        temp.path()
            .join("Music/Reprise/Artist/Album/unfinished.aiff.part"),
        b"partial",
    )
    .unwrap();
    fs::write(
        temp.path()
            .join("Music/Reprise/Artist/Album/01 Original.lrc"),
        b"lyrics",
    )
    .unwrap();

    let inspection = run(storage.inspect(&default_target())).unwrap();
    assert_eq!(
        inspection
            .managed_files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect::<Vec<_>>(),
        ["Artist/Album/01 Original.aiff", "Originals.m3u8"]
    );
    assert_eq!(inspection.snapshot.reprise_music_bytes, 46);
    assert_eq!(
        inspection.partial_paths,
        ["Artist/Album/unfinished.aiff.part"]
    );
    assert_eq!(
        inspection.lyrics_files,
        [ManagedDeviceFile {
            relative_path: "Artist/Album/01 Original.lrc".into(),
            size_bytes: 6,
        }]
    );
}

#[test]
fn managed_report_read_distinguishes_present_bytes_from_an_absent_file() {
    let (temp, storage) = fixture();
    fs::create_dir_all(temp.path().join("Music/Reprise")).unwrap();
    fs::write(
        temp.path().join("Music/Reprise/reprise-listens-back.rpl"),
        b"report-bytes",
    )
    .unwrap();

    assert_eq!(
        run(storage.read_managed(None, "/Music/Reprise", "reprise-listens-back.rpl",)).unwrap(),
        Some(b"report-bytes".to_vec())
    );
    assert_eq!(
        run(storage.read_managed(None, "/Music/Reprise", "missing.rpl")).unwrap(),
        None
    );
}

#[test]
fn storage_volume_choice_prefers_internal_storage_and_otherwise_stays_deterministic() {
    let volumes = vec!["SD Card".to_string(), "Internal shared storage".to_string()];
    assert_eq!(
        choose_storage_volume(&volumes),
        Some("Internal shared storage".into())
    );
    assert_eq!(
        choose_storage_volume(&["SD Card".into(), "Phone storage".into()]),
        Some("Phone storage".into())
    );
    assert_eq!(choose_storage_volume(&[]), None);
}

#[test]
fn copy_creates_managed_directories_and_reports_progress() {
    let (temp, storage) = fixture();
    let source_path = temp.path().join("source.flac");
    fs::write(&source_path, vec![7_u8; 32 * 1024]).unwrap();
    let progress = Rc::new(RefCell::new(Vec::new()));
    let observed = progress.clone();
    let outcome = run(storage.replace_managed(
        None,
        "/Music/Reprise",
        &gio::File::for_path(&source_path),
        "Road/7-source.flac",
        32 * 1024,
        &gio::Cancellable::new(),
        move |copied, total| observed.borrow_mut().push((copied, total)),
    ))
    .unwrap();

    assert_eq!(outcome, CopyOutcome::Copied);
    assert_eq!(
        fs::read(temp.path().join("Music/Reprise/Road/7-source.flac")).unwrap(),
        vec![7_u8; 32 * 1024]
    );
    assert!(progress
        .borrow()
        .windows(2)
        .all(|pair| pair[0].0 <= pair[1].0));
    assert_eq!(
        progress.borrow().last().copied(),
        Some((32 * 1024, 32 * 1024))
    );
}

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
fn managed_write_names_target_storage_resolution_failures() {
    let (temp, storage) = fixture();
    let source = temp.path().join("source.flac");
    fs::write(&source, b"audio").unwrap();

    let result = run(storage.replace_managed(
        Some(StorageId(u32::MAX)),
        "/Music/Reprise",
        &gio::File::for_path(source),
        "Road/song.flac",
        5,
        &gio::Cancellable::new(),
        |_, _| {},
    ));

    assert!(matches!(
        result,
        Err(DeviceIoError::DuringWrite {
            step: WriteStep::ResolveStorage,
            ..
        })
    ));
}

#[test]
fn mtp_17_same_size_untracked_destination_is_overwritten() {
    let (temp, storage) = fixture();
    fs::create_dir_all(temp.path().join("Music/Reprise/Road")).unwrap();
    fs::write(temp.path().join("source.flac"), b"new!").unwrap();
    fs::write(
        temp.path().join("Music/Reprise/Road/7-source.flac"),
        b"old!",
    )
    .unwrap();
    let outcome = run(storage.replace_managed(
        None,
        "/Music/Reprise",
        &gio::File::for_path(temp.path().join("source.flac")),
        "Road/7-source.flac",
        4,
        &gio::Cancellable::new(),
        |_copied, _total| {},
    ))
    .unwrap();
    assert_eq!(outcome, CopyOutcome::Copied);
    assert_eq!(
        fs::read(temp.path().join("Music/Reprise/Road/7-source.flac")).unwrap(),
        b"new!"
    );
}

#[test]
fn replace_track_overwrites_a_changed_file_even_when_its_size_is_unchanged() {
    let (temp, storage) = fixture();
    fs::create_dir_all(temp.path().join("Music/Reprise/Road")).unwrap();
    fs::write(temp.path().join("source.flac"), b"new!").unwrap();
    fs::write(
        temp.path().join("Music/Reprise/Road/7-source.flac"),
        b"old!",
    )
    .unwrap();

    let outcome = run(storage.replace_managed(
        None,
        "/Music/Reprise",
        &gio::File::for_path(temp.path().join("source.flac")),
        "Road/7-source.flac",
        4,
        &gio::Cancellable::new(),
        |_copied, _total| {},
    ))
    .unwrap();

    assert_eq!(outcome, CopyOutcome::Copied);
    assert_eq!(
        fs::read(temp.path().join("Music/Reprise/Road/7-source.flac")).unwrap(),
        b"new!"
    );
}

#[test]
fn replacement_verifies_the_partial_size_before_overwriting_the_final_file() {
    let (temp, storage) = fixture();
    fs::create_dir_all(temp.path().join("Music/Reprise/Road")).unwrap();
    fs::write(temp.path().join("source.flac"), b"short").unwrap();
    let final_path = temp.path().join("Music/Reprise/Road/7-source.flac");
    fs::write(&final_path, b"known-good").unwrap();

    let result = run(storage.replace_managed(
        None,
        "/Music/Reprise",
        &gio::File::for_path(temp.path().join("source.flac")),
        "Road/7-source.flac",
        6,
        &gio::Cancellable::new(),
        |_copied, _total| {},
    ));

    assert!(matches!(
        &result,
        Err(DeviceIoError::DuringWrite {
            step: WriteStep::VerifyPartial,
            source,
        }) if matches!(source.as_ref(), DeviceIoError::SizeMismatch {
            expected: 6, actual: 5
        })
    ));
    assert_eq!(
        result.unwrap_err().to_string(),
        "verifying the partial file failed: partial device file has 5 bytes, expected 6"
    );
    assert_eq!(fs::read(final_path).unwrap(), b"known-good");
    assert!(!temp
        .path()
        .join("Music/Reprise/Road/7-source.flac.part")
        .exists());
}

#[test]
fn copy_rejects_paths_outside_the_managed_root() {
    let (temp, storage) = fixture();
    fs::write(temp.path().join("source.flac"), b"x").unwrap();
    let result = run(storage.replace_managed(
        None,
        "/Music/Reprise",
        &gio::File::for_path(temp.path().join("source.flac")),
        "../outside.flac",
        1,
        &gio::Cancellable::new(),
        |_copied, _total| {},
    ));
    assert!(matches!(result, Err(DeviceIoError::InvalidRelativePath)));
    assert!(!temp.path().join("Music/outside.flac").exists());
}

#[test]
fn playlist_replace_and_read_round_trip() {
    let (_temp, storage) = fixture();
    run(storage.replace_playlist(
        None,
        "/Music/Reprise",
        "Road",
        b"#EXTM3U\nRoad/7-song.flac\n".to_vec(),
    ))
    .unwrap();
    assert_eq!(
        run(storage.read_playlist("/Music/Reprise", "Road")).unwrap(),
        vec![M3uEntry {
            path: "Road/7-song.flac".into()
        }]
    );
}

/// The MTP backend can answer a rename with success without performing it,
/// which is why publishing is proven rather than believed. A local fixture
/// cannot fake that answer, so the proof itself is exercised directly.
#[test]
fn mtp_21_a_published_file_is_proven_by_its_expected_byte_count() {
    let (temp, _storage) = fixture();
    let path = temp.path().join("published.opus");
    fs::write(&path, b"abcdef").unwrap();
    let published = gio::File::for_path(&path);

    assert!(run(verify_published(&published, 6)).is_ok());
    assert!(matches!(
        run(verify_published(&published, 9)),
        Err(DeviceIoError::DuringWrite {
            step: WriteStep::Publish,
            source,
        }) if matches!(source.as_ref(), DeviceIoError::SizeMismatch {
            expected: 9, actual: 6
        })
    ));
}

#[test]
fn mtp_21_a_rename_that_left_nothing_behind_is_reported_not_believed() {
    let (temp, _storage) = fixture();
    let missing = gio::File::for_path(temp.path().join("never-arrived.opus"));

    let result = run(verify_published(&missing, 6));
    assert!(matches!(
        &result,
        Err(DeviceIoError::DuringWrite {
            step: WriteStep::Publish,
            source,
        }) if matches!(source.as_ref(), DeviceIoError::PublishNotApplied { .. })
    ));
    assert_eq!(
        result.unwrap_err().to_string(),
        "publishing the destination file failed: the device acknowledged publishing never-arrived.opus but the file never appeared"
    );
}

#[test]
fn mtp_21_replacing_an_existing_track_publishes_it_without_leaving_a_partial() {
    let (temp, storage) = fixture();
    fs::create_dir_all(temp.path().join("Music/Reprise/Road")).unwrap();
    fs::write(temp.path().join("source.flac"), b"new!").unwrap();
    let final_path = temp.path().join("Music/Reprise/Road/7-source.flac");
    fs::write(&final_path, b"old!").unwrap();

    run(storage.replace_managed(
        None,
        DEFAULT_TARGET_PATH,
        &gio::File::for_path(temp.path().join("source.flac")),
        "Road/7-source.flac",
        4,
        &gio::Cancellable::new(),
        |_copied, _total| {},
    ))
    .unwrap();

    assert_eq!(fs::read(&final_path).unwrap(), b"new!");
    assert!(!temp
        .path()
        .join("Music/Reprise/Road/7-source.flac.part")
        .exists());
}

#[test]
fn mtp_21_rewriting_a_playlist_replaces_it_without_leaving_a_partial() {
    let (temp, storage) = fixture();
    run(storage.replace_playlist(
        None,
        DEFAULT_TARGET_PATH,
        "Road",
        b"#EXTM3U\nRoad/1-old.flac\n".to_vec(),
    ))
    .unwrap();

    run(storage.replace_playlist(
        None,
        DEFAULT_TARGET_PATH,
        "Road",
        b"#EXTM3U\nRoad/2-new.flac\n".to_vec(),
    ))
    .unwrap();

    assert_eq!(
        fs::read(temp.path().join("Music/Reprise/Road.m3u8")).unwrap(),
        b"#EXTM3U\nRoad/2-new.flac\n"
    );
    assert!(!temp.path().join("Music/Reprise/Road.m3u8.part").exists());
}

#[test]
fn pre_cancelled_copy_leaves_no_partial_file() {
    let (temp, storage) = fixture();
    fs::write(temp.path().join("source.flac"), vec![1_u8; 1024]).unwrap();
    let cancellable = gio::Cancellable::new();
    cancellable.cancel();
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
            step: WriteStep::CopyPartial,
            ..
        })
    ));
    assert!(result
        .unwrap_err()
        .to_string()
        .starts_with("copying the partial file failed: device I/O failed:"));
    assert!(!temp
        .path()
        .join("Music/Reprise/Road/1-source.flac.part")
        .exists());
}

#[test]
fn cleanup_partials_removes_only_orphaned_part_files_under_the_managed_root() {
    let (temp, storage) = fixture();
    let managed = temp.path().join("Music/Reprise/Road");
    fs::create_dir_all(&managed).unwrap();
    fs::write(managed.join("unfinished.opus.part"), b"partial").unwrap();
    fs::write(managed.join("finished.opus"), b"finished").unwrap();
    fs::write(temp.path().join("Music/outside.part"), b"outside").unwrap();

    let listed = [
        "Road/unfinished.opus.part".into(),
        "Road/finished.opus".into(),
    ];
    assert_eq!(
        run(storage.cleanup_partials_in(None, "/Music/Reprise", &listed)).unwrap(),
        1
    );
    assert!(!managed.join("unfinished.opus.part").exists());
    assert!(managed.join("finished.opus").exists());
    assert!(temp.path().join("Music/outside.part").exists());
}

#[test]
fn cleanup_partials_silently_accepts_a_missing_managed_root() {
    let (_temp, storage) = fixture();
    let (result, warnings) = capture_warnings(|| {
        run(storage.cleanup_partials_in(None, "/Music/Reprise", &["missing.opus.part".into()]))
    });
    assert_eq!(result.unwrap(), 0);
    assert_eq!(warnings, "");
}

#[test]
fn cleanup_partials_uses_only_listed_paths_and_continues_after_a_delete_failure() {
    let (temp, storage) = fixture();
    if fs::metadata(temp.path()).unwrap().uid() == 0 {
        eprintln!("skipped permission-based test under a root test runner");
        return;
    }
    let root = temp.path().join("Music/Reprise");
    let protected = root.join("Protected");
    let writable = root.join("Deep/Writable");
    let unreadable = root.join("Unreadable");
    fs::create_dir_all(&protected).unwrap();
    fs::create_dir_all(&writable).unwrap();
    fs::create_dir_all(&unreadable).unwrap();
    fs::write(protected.join("left-behind.opus.part"), b"partial").unwrap();
    fs::write(writable.join("removed.opus.part"), b"partial").unwrap();
    fs::set_permissions(&protected, fs::Permissions::from_mode(0o500)).unwrap();
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).unwrap();

    let listed = [
        "Protected/left-behind.opus.part".into(),
        "Deep/Writable/removed.opus.part".into(),
    ];
    let (result, warnings) =
        capture_warnings(|| run(storage.cleanup_partials_in(None, "/Music/Reprise", &listed)));

    fs::set_permissions(&protected, fs::Permissions::from_mode(0o700)).unwrap();
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(result.as_ref().ok(), Some(&1));
    assert!(protected.join("left-behind.opus.part").exists());
    assert!(!writable.join("removed.opus.part").exists());
    assert!(warnings.contains("Protected/left-behind.opus.part"));
    assert!(warnings.contains("action=\"delete partial file\""));
}

#[test]
fn cleanup_partials_rejects_an_invalid_storage_or_target_path() {
    let (_temp, storage) = fixture();
    assert!(matches!(
        run(storage.cleanup_partials_in(
            Some(StorageId(u32::MAX)),
            "/Music/Reprise",
            &["unfinished.opus.part".into()],
        )),
        Err(DeviceIoError::StorageNotFound)
    ));
    assert!(matches!(
        run(storage.cleanup_partials_in(None, "../outside", &["unfinished.opus.part".into()])),
        Err(DeviceIoError::InvalidRelativePath)
    ));
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
fn local_fixture_reports_available_space_when_supported() {
    let (_temp, storage) = fixture();
    assert!(run(storage.available_bytes()).unwrap().is_some());
}

#[test]
fn local_fixture_reports_total_capacity_when_supported() {
    let (_temp, storage) = fixture();
    let (available, total) = run(storage.capacity_bytes()).unwrap();

    assert!(available.is_some());
    assert!(total.is_some());
    assert!(total >= available);
}

/// Non-MTP roots (the local directories these tests use, and any future
/// backend that hands us a real filesystem) must not be re-rooted into a
/// storage volume: `storage_root` only descends for `mtp://`. Copying into a
/// fixture root therefore lands in `<root>/Music/Reprise`, not one level
/// deeper — which is what every other test in this file relies on.
#[test]
fn non_mtp_root_is_used_verbatim_without_storage_resolution() {
    let (temp, storage) = fixture();
    let source_path = temp.path().join("source.flac");
    fs::write(&source_path, b"audio").unwrap();

    run(storage.replace_managed(
        None,
        "/Music/Reprise",
        &gio::File::for_path(&source_path),
        "Road/song.flac",
        5,
        &gio::Cancellable::new(),
        |_, _| {},
    ))
    .unwrap();

    assert!(
        temp.path().join("Music/Reprise/Road/song.flac").is_file(),
        "copy lands directly under the fixture root's Music/Reprise"
    );
}

/// `MTP-23`: a target repointed at a non-default
/// storage (`MTP-31`'s folder browser) must have both its transfer AND its
/// next inspection actually use that `StorageId` + path — not silently
/// fall back to `DeviceStorage::storage_root`'s "prefer internal" guess.
/// Before this fix, `replace_managed`/`inspect` ignored `storage_id`
/// entirely, so a file written to an SD card landed on the default
/// storage instead, and a rescan never recognized the SD-card folder as
/// the playlists inventory.
#[test]
fn mtp_23_transfer_and_inspection_route_through_the_persisted_target_storage() {
    let (temp, storage) = fixture();
    fs::create_dir_all(temp.path().join("SD Card")).unwrap();
    let sd_card = derive_storage_id("SD Card");
    let source_path = temp.path().join("track.opus");
    fs::write(&source_path, b"track-audio").unwrap();

    run(storage.replace_managed(
        Some(sd_card),
        "/Music/Selected",
        &gio::File::for_path(&source_path),
        "Artist/1-Track.opus",
        11,
        &gio::Cancellable::new(),
        |_, _| {},
    ))
    .unwrap();

    // The file must land on the chosen SD card, not the default
    // "prefer internal" guess `storage_root` would have picked.
    assert!(temp
        .path()
        .join("SD Card/Music/Selected/Artist/1-Track.opus")
        .is_file());
    assert!(!temp.path().join("Music/Selected").exists());

    let target = target_at(sd_card, "/Music/Selected");
    let inspection = run(storage.inspect(&target)).unwrap();
    assert_eq!(
        inspection
            .managed_files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect::<Vec<_>>(),
        ["Artist/1-Track.opus"],
        "the next scan must recognize the selected SD-card playlists folder"
    );

    assert!(
        run(storage.delete_managed(Some(sd_card), "/Music/Selected", "Artist/1-Track.opus"))
            .unwrap()
    );
}
