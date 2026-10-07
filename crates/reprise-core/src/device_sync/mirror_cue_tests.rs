//! A CUE album syncs once, with a sheet derived for the device (CUE-15).

use std::path::PathBuf;

use super::cue_files::{CueSyncFile, CueSyncTrack};
use super::{
    plan_mirror, DeviceFileRecord, DeviceSyncMachine, Effect, Event, ManagedDeviceFile,
    ManagedRemoval, MirrorInput, MirrorPlan, MirrorPlaylistSnapshot, MirrorTrack, Mp3Quality,
    SelectionSource, SyncTrack, TransferProfile,
};

const SOURCE: &str = "/music/Band/Album/album.flac";
const DEVICE_PATH: &str = "Band/Album/album.mp3";
const PROFILE: TransferProfile = TransferProfile::Mp3(Mp3Quality::Kbps256);
const FINGERPRINT: &str = "mp3-cbr-256-v1";

fn album() -> CueSyncFile {
    let track = |id: i64, start_ms: i64, end_ms: i64| CueSyncTrack {
        track_id: id,
        start_ms,
        end_ms,
        title: format!("Song {id}"),
        performer: "Band".into(),
        track_no: Some(u32::try_from(id).unwrap()),
    };
    CueSyncFile {
        source_path: PathBuf::from(SOURCE),
        album: "Album".into(),
        album_artist: "Band".into(),
        year: None,
        genre: String::new(),
        duration_ms: 30_000,
        tracks: vec![
            track(1, 0, 10_000),
            track(2, 10_000, 20_000),
            track(3, 20_000, 30_000),
        ],
    }
}

fn segment(id: i64) -> SyncTrack {
    SyncTrack {
        id,
        source_path: PathBuf::from(SOURCE),
        original_name: "album.flac".into(),
        title: format!("Song {id}"),
        artist: "Band".into(),
        album: "Album".into(),
        album_artist: "Band".into(),
        track_number: Some(u32::try_from(id).unwrap()),
        duration_ms: 10_000,
        bitrate_kbps: Some(900),
        size_bytes: 3_000_000,
        source_mtime: 10,
    }
}

fn input(ids: &[i64]) -> MirrorInput {
    let source = SelectionSource::Playlist(1);
    MirrorInput {
        selected: vec![source.clone()],
        playlists: vec![MirrorPlaylistSnapshot {
            source,
            name: "Album".into(),
            entries: ids
                .iter()
                .map(|id| MirrorTrack::Available(segment(*id)))
                .collect(),
            stability_margin_track_ids: Vec::new(),
            cue_files: vec![album()],
        }],
        profile: PROFILE,
        inventory: Vec::new(),
        playlist_inventory: Vec::new(),
        managed_files: Vec::new(),
        partial_paths: Vec::new(),
        lyrics_files: Vec::new(),
        managed_files_scanned: true,
        desktop_analyses: Vec::new(),
    }
}

fn row(id: i64) -> DeviceFileRecord {
    DeviceFileRecord {
        device_serial: "phone".into(),
        track_id: id,
        source_path: SOURCE.into(),
        source_size: 3_000_000,
        source_mtime: 10,
        device_path: DEVICE_PATH.into(),
        device_size: 960_000,
        profile_fingerprint: FINGERPRINT.into(),
        pinned: false,
    }
}

/// `ids` selected, with rows for `rows` and the file and its sheet on the device.
fn synced(ids: &[i64], rows: &[i64]) -> MirrorInput {
    let first = plan_mirror(input(ids));
    let sheet_bytes = first.cue_writes[0].size_bytes;
    let sheet_path = first.cue_writes[0].device_path.clone();
    MirrorInput {
        inventory: rows.iter().map(|id| row(*id)).collect(),
        managed_files: vec![
            ManagedDeviceFile {
                relative_path: DEVICE_PATH.into(),
                size_bytes: 960_000,
            },
            ManagedDeviceFile {
                relative_path: sheet_path,
                size_bytes: sheet_bytes,
            },
        ],
        ..input(ids)
    }
}

fn is_quiet(plan: &MirrorPlan) -> bool {
    plan.copy.is_empty()
        && plan.replace.is_empty()
        && plan.shared_records.is_empty()
        && plan.cue_writes.is_empty()
        && plan.remove.is_empty()
}

#[test]
fn cue_15_two_tracks_of_one_file_sync_it_once_with_its_bytes_counted_once() {
    let plan = plan_mirror(input(&[1, 2]));

    assert_eq!(plan.copy.len(), 1, "one transfer for the file");
    assert_eq!(plan.copy[0].device_path, DEVICE_PATH);
    assert_eq!(plan.copy[0].track.duration_ms, 30_000, "the whole file");
    let file_bytes = PROFILE.estimated_target_bytes(&SyncTrack {
        duration_ms: 30_000,
        ..segment(1)
    });
    assert_eq!(
        plan.transfer_bytes - plan.cue_writes[0].size_bytes,
        file_bytes
    );
    assert_eq!(
        plan.target_bytes - plan.cue_writes[0].size_bytes,
        file_bytes
    );
    let paths: Vec<&str> = plan
        .desired_files
        .iter()
        .map(|file| file.device_path.as_str())
        .collect();
    assert_eq!(paths, [DEVICE_PATH, DEVICE_PATH]);
    assert_eq!(plan.shared_records.len(), 1);
    assert_eq!(plan.shared_records[0].desired.track.id, 2);
    assert_eq!(plan.shared_records[0].carrier_track_id, 1);
    assert_eq!(plan.shared_records[0].resident_size, None);
}

#[test]
fn cue_15_the_derived_sheet_names_the_device_file_and_every_track() {
    let plan = plan_mirror(input(&[2]));

    assert_eq!(plan.cue_writes.len(), 1);
    let write = &plan.cue_writes[0];
    assert!(super::derived_cue::describes(
        &write.device_path,
        DEVICE_PATH
    ));
    assert_eq!(write.audio_device_path, DEVICE_PATH);
    let sheet = crate::cue::parse(write.contents.as_bytes()).unwrap();
    assert_eq!(sheet.files[0].name, "album.mp3");
    assert_eq!(
        sheet.files[0].tracks.len(),
        3,
        "the phone lists every track"
    );
}

#[test]
fn cue_15_a_synced_cue_album_plans_nothing_on_the_next_run() {
    let plan = plan_mirror(synced(&[1, 2], &[1, 2]));

    assert!(is_quiet(&plan), "{plan:?}");
}

#[test]
fn cue_15_dropping_one_track_forgets_its_row_and_keeps_the_file() {
    let plan = plan_mirror(synced(&[1], &[1, 2]));

    assert_eq!(plan.remove, [ManagedRemoval::Unshared(row(2))]);
    assert_eq!(plan.bytes_freed, 0);
    assert!(plan.copy.is_empty() && plan.replace.is_empty());
}

#[test]
fn cue_15_dropping_the_last_track_removes_the_file_and_its_sheet() {
    let mut mirror_input = synced(&[1], &[1]);
    let sheet_path = mirror_input.managed_files[1].relative_path.clone();
    mirror_input.playlists[0].entries.clear();

    let plan = plan_mirror(mirror_input);

    assert!(plan.remove.contains(&ManagedRemoval::Inventory(row(1))));
    assert!(plan.remove.iter().any(|removal| matches!(
        removal,
        ManagedRemoval::Orphan(file) if file.relative_path == sheet_path
    )));
}

#[test]
fn cue_15_adding_a_track_of_a_synced_file_records_it_without_a_copy() {
    let plan = plan_mirror(synced(&[1, 2, 3], &[1, 2]));

    assert!(plan.copy.is_empty() && plan.replace.is_empty());
    assert_eq!(plan.shared_records.len(), 1);
    assert_eq!(plan.shared_records[0].desired.track.id, 3);
    assert_eq!(plan.shared_records[0].resident_size, Some(960_000));
}

#[test]
fn cue_15_hiding_the_track_a_copy_was_recorded_under_causes_no_recopy() {
    // Track 1's row recorded the copy; track 1 has since left the library.
    let plan = plan_mirror(synced(&[2, 3], &[1, 2, 3]));

    assert!(plan.copy.is_empty() && plan.replace.is_empty());
    assert_eq!(plan.remove, [ManagedRemoval::Unshared(row(1))]);
}

#[test]
fn cue_15_a_run_records_every_track_of_the_file_after_one_copy() {
    let plan = plan_mirror(input(&[1, 2]));
    let mut machine = DeviceSyncMachine::new("phone".into(), plan);
    let mut effects = machine.dispatch(Event::Start);
    let mut seen = Vec::new();
    while let Some(effect) = effects.pop() {
        let event = match &effect {
            Effect::CleanPartials(_) => Event::PartialsCleaned(Ok(())),
            Effect::Transcode { .. } => Event::Transcoded(Ok(960_000)),
            Effect::CopyTrack { index, .. } => Event::TrackCopied(Ok(super::CopiedTrack {
                device_size: 960_000,
                device_path: machine.transfers()[*index].desired.device_path.clone(),
            })),
            Effect::RecordFile { .. } => Event::FileRecorded(Ok(())),
            Effect::RecordSharedFile { .. } => Event::SharedFileRecorded(Ok(())),
            Effect::WriteDerivedCue { index } => {
                Event::DerivedCueWritten(Ok(machine.plan().cue_writes[*index].size_bytes))
            }
            Effect::WritePlaylist { .. } => Event::PlaylistWritten(Ok(())),
            Effect::RecordPlaylist { .. } => Event::PlaylistRecorded(Ok(())),
            Effect::Finished(outcome) => {
                seen.push(format!("{outcome:?}"));
                break;
            }
            other => panic!("unexpected effect {other:?}"),
        };
        seen.push(format!("{effect:?}"));
        effects = machine.dispatch(event);
    }

    let shared = seen
        .iter()
        .find(|effect| effect.starts_with("RecordSharedFile"))
        .expect("the second track is recorded");
    assert!(shared.contains("device_size: 960000"));
    assert!(shared.contains(DEVICE_PATH));
    assert_eq!(
        seen.iter()
            .filter(|effect| effect.starts_with("CopyTrack"))
            .count(),
        1
    );
    assert!(seen
        .iter()
        .any(|effect| effect.starts_with("WriteDerivedCue")));
    assert!(seen.last().unwrap().starts_with("Completed"));
}

#[test]
fn cue_15_the_sync_page_counts_a_cue_file_once_per_playlist() {
    let mirror_input = input(&[1, 2, 3]);
    let projection = super::project_sync_page(super::SyncPageInput {
        selected: mirror_input.selected,
        playlists: mirror_input.playlists,
        profile: TransferProfile::Original,
        ..super::SyncPageInput::default()
    });

    assert_eq!(projection.page.playlists[0].target_bytes, 3_000_000);
    assert_eq!(projection.page.playlists[0].unique_track_count, 3);
}

#[test]
fn cue_15_the_size_projection_counts_a_copied_cue_file_once() {
    let projection = super::project_playlist_sizes(
        &[super::PlaylistTracks {
            source: SelectionSource::Playlist(1),
            name: "Album".into(),
            tracks: vec![segment(1), segment(2), segment(3)],
        }],
        TransferProfile::Original,
    );

    assert_eq!(projection.playlists[0].target_bytes, 3_000_000);
}

#[test]
fn cue_15_a_sheet_edit_that_keeps_its_length_still_rewrites_the_derived_sheet() {
    let synced_input = synced(&[1, 2], &[1, 2]);
    let old_sheet = synced_input.managed_files[1].relative_path.clone();
    let mut edited = synced_input;
    // A start moved by a few frames keeps the sheet's length: MM:SS:FF is fixed-width.
    edited.playlists[0].cue_files[0].tracks[1].start_ms = 10_040;
    edited.playlists[0].cue_files[0].tracks[0].end_ms = 10_040;

    let plan = plan_mirror(edited);

    assert_eq!(plan.cue_writes.len(), 1, "the phone must get the new cut");
    assert_ne!(plan.cue_writes[0].device_path, old_sheet);
    assert!(plan.remove.iter().any(|removal| matches!(
        removal,
        ManagedRemoval::Orphan(file) if file.relative_path == old_sheet
    )));
}

#[test]
fn cue_15_a_frozen_cue_file_keeps_its_derived_sheet() {
    let mut mirror_input = synced(&[1], &[1]);
    let sheet_path = mirror_input.managed_files[1].relative_path.clone();
    mirror_input.playlists[0].entries.clear();
    let mut plan = plan_mirror(mirror_input);

    super::apply_frozen_smart_playlist_policy(
        &mut plan,
        &std::collections::HashSet::from([SelectionSource::Smart(9)]),
        &std::collections::HashSet::from([1]),
    );

    assert!(!plan.remove.iter().any(|removal| matches!(
        removal,
        ManagedRemoval::Orphan(file) if file.relative_path == sheet_path
    )));
    assert_eq!(plan.bytes_freed, 0);
}

/// `album()` as a compilation: per-track performers and no album artist.
fn compilation(ids: &[i64]) -> MirrorInput {
    let performer = |id: i64| ["A", "B", "C"][usize::try_from(id - 1).unwrap()].to_string();
    let mut file = album();
    file.album_artist = String::new();
    for track in &mut file.tracks {
        track.performer = performer(track.track_id);
    }
    let base = input(ids);
    let playlist = MirrorPlaylistSnapshot {
        entries: ids
            .iter()
            .map(|id| {
                MirrorTrack::Available(SyncTrack {
                    artist: performer(*id),
                    album_artist: String::new(),
                    ..segment(*id)
                })
            })
            .collect(),
        cue_files: vec![file],
        ..base.playlists[0].clone()
    };
    MirrorInput {
        playlists: vec![playlist],
        ..base
    }
}

#[test]
fn cue_15_a_compilation_file_has_one_device_path_whichever_tracks_are_selected() {
    let plan = plan_mirror(compilation(&[1, 3]));

    let paths: Vec<&str> = plan
        .desired_files
        .iter()
        .map(|file| file.device_path.as_str())
        .collect();
    assert_eq!(paths, ["A/Album/album.mp3", "A/Album/album.mp3"]);
    assert_eq!(plan.copy.len(), 1);
    assert_eq!(plan.cue_writes.len(), 1);
    for ids in [&[2][..], &[1, 2, 3][..]] {
        let plan = plan_mirror(compilation(ids));
        assert!(
            plan.desired_files
                .iter()
                .all(|file| file.device_path == "A/Album/album.mp3"),
            "{ids:?}: {:?}",
            plan.desired_files
        );
    }
}

#[test]
fn cue_15_a_synced_compilation_plans_nothing_on_the_next_run() {
    let first = plan_mirror(compilation(&[1, 3]));
    let device_path = "A/Album/album.mp3";
    let rows = [1, 3].map(|id| DeviceFileRecord {
        device_path: device_path.into(),
        ..row(id)
    });
    let plan = plan_mirror(MirrorInput {
        inventory: rows.to_vec(),
        managed_files: vec![
            ManagedDeviceFile {
                relative_path: device_path.into(),
                size_bytes: 960_000,
            },
            ManagedDeviceFile {
                relative_path: first.cue_writes[0].device_path.clone(),
                size_bytes: first.cue_writes[0].size_bytes,
            },
        ],
        ..compilation(&[1, 3])
    });

    assert!(is_quiet(&plan), "{plan:?}");
}
