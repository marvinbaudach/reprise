use super::*;

fn synced_audio_with_lyrics(contents: Option<&[u8]>) -> (tempfile::TempDir, MirrorInput) {
    let temp = tempfile::tempdir().unwrap();
    let source_path = temp.path().join("one.mp3");
    std::fs::write(&source_path, b"audio").unwrap();
    if let Some(contents) = contents {
        std::fs::write(source_path.with_extension("lrc"), contents).unwrap();
    }
    let mut mirror_input = synced_audio_input();
    let MirrorTrack::Available(wanted) = &mut mirror_input.playlists[0].entries[0] else {
        unreachable!("the fixture contains one available track")
    };
    wanted.source_path = source_path.clone();
    mirror_input.inventory[0].source_path = source_path.to_string_lossy().into_owned();
    (temp, mirror_input)
}

#[test]
fn a_resident_lyrics_sidecar_of_the_expected_size_plans_no_write() {
    let (_temp, mut input) = synced_audio_with_lyrics(Some(b"lyrics"));
    input.lyrics_files.push(ManagedDeviceFile {
        relative_path: "Album Artist/Album/01 Track 1.lrc".into(),
        size_bytes: 6,
    });
    let plan = plan_mirror(input);
    assert!(plan.lyrics_writes.is_empty());
    assert!(plan.remove.is_empty());
    assert_eq!(plan.transfer_bytes, 0);
}

#[test]
fn a_fold_equivalent_resident_lyrics_sidecar_plans_no_write() {
    let (_temp, mut input) = synced_audio_with_lyrics(Some(b"lyrics"));
    input.lyrics_files.push(ManagedDeviceFile {
        relative_path: "album artist/album/01 track 1.LRC".into(),
        size_bytes: 6,
    });

    assert!(plan_mirror(input).lyrics_writes.is_empty());
}

#[test]
fn a_resident_lyrics_sidecar_of_a_different_size_plans_only_its_rewrite() {
    let (_temp, mut input) = synced_audio_with_lyrics(Some(b"lyrics"));
    input.lyrics_files.push(ManagedDeviceFile {
        relative_path: "Album Artist/Album/01 Track 1.lrc".into(),
        size_bytes: 5,
    });
    let plan = plan_mirror(input);
    assert!(plan.copy.is_empty() && plan.replace.is_empty());
    assert_eq!(plan.lyrics_writes.len(), 1);
    assert_eq!(plan.lyrics_writes[0].existing_size_bytes, Some(5));
    assert_eq!(plan.lyrics_writes[0].size_bytes, 6);
    assert_eq!(plan.transfer_bytes, 6);
    assert!(plan.remove.is_empty());
}

#[test]
fn a_track_without_local_lyrics_plans_no_sidecar() {
    let (_temp, input) = synced_audio_with_lyrics(None);
    assert!(plan_mirror(input).lyrics_writes.is_empty());
}

#[test]
fn lyrics_arriving_with_their_audio_are_planned_once_and_never_removed() {
    let (_temp, mut input) = synced_audio_with_lyrics(Some(b"lyrics"));
    input.inventory.clear();
    input.managed_files.clear();
    let plan = plan_mirror(input);
    assert_eq!(plan.copy.len(), 1);
    assert_eq!(plan.lyrics_writes.len(), 1);
    assert_eq!(plan.lyrics_writes[0].existing_size_bytes, None);
    assert!(plan.remove.is_empty());
}
