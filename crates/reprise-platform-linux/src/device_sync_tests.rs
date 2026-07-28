use std::cell::RefCell;
use std::fs;
use std::future::Future;
use std::rc::Rc;

use gio::prelude::*;
use reprise_core::device_sync::DeviceStorageAccess;
use reprise_core::library::m3u::M3uEntry;
use tempfile::TempDir;

use super::inspection::storage_access_from_attributes;
use super::*;

fn run<T>(future: impl Future<Output = T>) -> T {
    let context = gio::glib::MainContext::new();
    context
        .with_thread_default(|| context.block_on(future))
        .unwrap()
}

fn fixture() -> (TempDir, DeviceStorage) {
    let temp = tempfile::tempdir().unwrap();
    let root = gio::File::for_path(temp.path());
    (temp, DeviceStorage::from_root(&root))
}

#[test]
fn descriptor_projection_accepts_only_mtp_roots() {
    assert!(project_descriptor("file:///tmp", Some("uuid"), "Disk").is_none());
    assert!(project_descriptor("gphoto2://phone", Some("uuid"), "Camera").is_none());
    assert!(project_descriptor("mtp://phone", Some("uuid"), "Phone").is_some());
}

#[test]
fn descriptor_prefers_uuid_for_stable_reconnects() {
    let descriptor = project_descriptor("mtp://phone", Some("serial-1"), "Pixel").unwrap();
    assert_eq!(descriptor.id, "serial-1");
    assert!(descriptor.reconnectable);
}

#[test]
fn descriptor_falls_back_to_uri_without_claiming_reconnect_support() {
    let descriptor = project_descriptor("mtp://phone", None, "Pixel").unwrap();
    assert_eq!(descriptor.id, "mtp://phone");
    assert!(!descriptor.reconnectable);
}

#[test]
fn missing_music_directory_inspects_as_empty() {
    let (_temp, storage) = fixture();
    let inspection = run(storage.inspect()).unwrap();
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

    let inspection = run(storage.inspect()).unwrap();
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

    let inspection = run(storage.inspect()).unwrap();
    assert_eq!(
        inspection
            .managed_files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect::<Vec<_>>(),
        ["Artist/Album/01 Original.aiff", "Originals.m3u8"]
    );
    assert_eq!(inspection.snapshot.reprise_music_bytes, 46);
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
        "/Music/Reprise",
        &gio::File::for_path(temp.path().join("source.flac")),
        "Road/7-source.flac",
        6,
        &gio::Cancellable::new(),
        |_copied, _total| {},
    ));

    assert!(matches!(
        result,
        Err(DeviceIoError::SizeMismatch {
            expected: 6,
            actual: 5,
        })
    ));
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

#[test]
fn pre_cancelled_copy_leaves_no_partial_file() {
    let (temp, storage) = fixture();
    fs::write(temp.path().join("source.flac"), vec![1_u8; 1024]).unwrap();
    let cancellable = gio::Cancellable::new();
    cancellable.cancel();
    let result = run(storage.replace_managed(
        "/Music/Reprise",
        &gio::File::for_path(temp.path().join("source.flac")),
        "Road/1-source.flac",
        1024,
        &cancellable,
        |_copied, _total| {},
    ));
    assert!(result.is_err());
    assert!(!temp
        .path()
        .join("Music/Reprise/Road/1-source.flac.part")
        .exists());
}

#[test]
fn cleanup_partials_removes_only_orphaned_part_files_under_the_managed_root() {
    let (temp, storage) = fixture();
    fs::create_dir_all(temp.path().join("Music/Reprise/Road")).unwrap();
    fs::write(
        temp.path().join("Music/Reprise/Road/unfinished.opus.part"),
        b"partial",
    )
    .unwrap();
    fs::write(
        temp.path().join("Music/Reprise/Road/finished.opus"),
        b"finished",
    )
    .unwrap();
    fs::write(temp.path().join("Music/outside.part"), b"outside").unwrap();

    assert_eq!(
        run(storage.cleanup_partials_in("/Music/Reprise")).unwrap(),
        1
    );
    assert!(!temp
        .path()
        .join("Music/Reprise/Road/unfinished.opus.part")
        .exists());
    assert!(temp
        .path()
        .join("Music/Reprise/Road/finished.opus")
        .exists());
    assert!(temp.path().join("Music/outside.part").exists());
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

    assert!(run(storage.delete_managed("/Music/Reprise", "Road/finished.opus")).unwrap());
    assert!(!run(storage.delete_managed("/Music/Reprise", "Road/finished.opus")).unwrap());
    assert!(matches!(
        run(storage.delete_managed("/Music/Reprise", "../outside.opus")),
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

#[test]
fn mtp_23_podcast_io_is_scoped_to_its_own_target_and_inspected_separately() {
    let (temp, storage) = fixture();
    let source_path = temp.path().join("episode.mp3");
    fs::write(&source_path, b"podcast").unwrap();
    fs::create_dir_all(temp.path().join("Music/Reprise/Album")).unwrap();
    fs::write(temp.path().join("Music/Reprise/Album/track.mp3"), b"music").unwrap();

    run(storage.replace_managed(
        "/Podcasts/Reprise",
        &gio::File::for_path(&source_path),
        "Show/1-Episode.mp3",
        7,
        &gio::Cancellable::new(),
        |_, _| {},
    ))
    .unwrap();
    assert_eq!(
        fs::read(temp.path().join("Podcasts/Reprise/Show/1-Episode.mp3")).unwrap(),
        b"podcast"
    );
    assert!(matches!(
        run(storage.delete_managed("/Podcasts/Reprise", "Music/Reprise/Album/track.mp3")),
        Ok(false)
    ));
    assert!(temp.path().join("Music/Reprise/Album/track.mp3").is_file());

    let contents = run(storage.inspect()).unwrap();
    assert_eq!(
        contents
            .podcast_files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect::<Vec<_>>(),
        ["Show/1-Episode.mp3"]
    );
    assert!(run(storage.delete_managed("/Podcasts/Reprise", "Show/1-Episode.mp3")).unwrap());
}

#[test]
fn mtp_23_youtube_audio_io_is_scoped_to_its_own_target_and_inspected_separately() {
    let (temp, storage) = fixture();
    let source_path = temp.path().join("video.opus");
    fs::write(&source_path, b"video-audio").unwrap();
    fs::create_dir_all(temp.path().join("Music/Reprise/Album")).unwrap();
    fs::write(temp.path().join("Music/Reprise/Album/track.mp3"), b"music").unwrap();
    fs::write(temp.path().join("Music/loose.mp3"), b"foreign").unwrap();

    run(storage.replace_managed(
        "/Music/Reprise-YouTube",
        &gio::File::for_path(&source_path),
        "Channel/1-Video.opus",
        11,
        &gio::Cancellable::new(),
        |_, _| {},
    ))
    .unwrap();
    assert_eq!(
        fs::read(
            temp.path()
                .join("Music/Reprise-YouTube/Channel/1-Video.opus")
        )
        .unwrap(),
        b"video-audio"
    );

    let contents = run(storage.inspect()).unwrap();
    assert_eq!(
        contents
            .youtube_files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect::<Vec<_>>(),
        ["Channel/1-Video.opus"]
    );
    // The YouTube-audio target sits inside `Music/`, alongside the
    // Playlists target and truly foreign files — it must count toward
    // neither `reprise_music_bytes` nor `other_music_bytes`.
    assert_eq!(contents.snapshot.reprise_music_bytes, 5);
    assert_eq!(contents.snapshot.other_music_bytes, 7);

    assert!(run(storage.delete_managed("/Music/Reprise-YouTube", "Channel/1-Video.opus")).unwrap());
}

#[test]
fn mtp_23_podcast_partial_cleanup_cannot_touch_music_or_other_podcast_apps() {
    let (temp, storage) = fixture();
    for path in [
        "Podcasts/Reprise/Show/episode.mp3.part",
        "Music/Reprise/Album/track.mp3.part",
        "Podcasts/Other App/episode.mp3.part",
    ] {
        let path = temp.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"partial").unwrap();
    }

    assert_eq!(
        run(storage.cleanup_partials_in("/Podcasts/Reprise")).unwrap(),
        1
    );
    assert!(!temp
        .path()
        .join("Podcasts/Reprise/Show/episode.mp3.part")
        .exists());
    assert!(temp
        .path()
        .join("Music/Reprise/Album/track.mp3.part")
        .exists());
    assert!(temp
        .path()
        .join("Podcasts/Other App/episode.mp3.part")
        .exists());
}

// Design 7d's device folder browser (`MTP-31`/`MTP-32`). Every operation
// below runs against a local temp directory standing in for the device
// root, exactly like the fixtures above — no real or simulated phone.

use super::target_browser::derive_storage_id;
use reprise_core::device_sync::browser::StorageKind;

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
