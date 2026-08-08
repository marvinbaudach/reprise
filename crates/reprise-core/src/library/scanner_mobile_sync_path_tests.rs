//! Phone-sync path identity regression tests, split for the 800-line rule.

use super::source_tests::{scripted_virtual_file, ScriptedSource};
use super::tests::{completed, fixture_copy};
use super::*;

#[test]
fn synced_metadata_uses_the_source_relative_name_not_an_opaque_handle() {
    use crate::device_sync::track_metadata_list::{
        TrackMetadataEntry, TrackMetadataList, FILE_NAME,
    };

    let tmp = tempfile::tempdir().unwrap();
    let staging = fixture_copy(tmp.path(), "staging.flac");
    let bytes = std::fs::read(&staging).unwrap();
    std::fs::remove_file(staging).unwrap();
    let track_handle = tmp.path().join("document-731.flac");
    let list_handle = tmp.path().join("document-941.rpl");
    let list = TrackMetadataList::new(vec![TrackMetadataEntry {
        device_path: "Phone Song.flac".into(),
        rating: 5,
        play_count: 41,
    }])
    .encode()
    .unwrap();
    let source = ScriptedSource::new(vec![
        scripted_virtual_file(&track_handle, bytes.len() as u64),
        scripted_virtual_file(&list_handle, list.len() as u64),
    ])
    .with_content(track_handle.clone(), bytes)
    .with_content(list_handle.clone(), list)
    .with_display_name(track_handle.clone(), "Phone Song.flac")
    .with_display_name(list_handle, FILE_NAME);
    let db = crate::db::Db::open_in_memory().unwrap();

    completed(scan_folder_with_source(&source, &db, tmp.path()).unwrap());

    let track = crate::queries::query_library_text_search(
        &db,
        "",
        crate::queries::WindowRange {
            offset: 0,
            limit: 10,
        },
    )
    .unwrap()
    .rows
    .remove(0);
    assert_eq!((track.rating, track.play_count), (5, 41));
    assert_eq!(
        crate::device_sync::mobile_import::device_path_for_track(&db, track.id).unwrap(),
        Some("Phone Song.flac".to_owned()),
        "the export identity is the source-relative path, not the opaque SAF handle"
    );
}
