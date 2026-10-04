use super::*;
use crate::library::loudness::ReplayGainTags;
use lofty::prelude::*;
use lofty::tag::{ItemKey, Tag, TagType};

fn tagged_fixture(key: ItemKey, value: &str) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    let path = temp.path().join("tagged.flac");
    std::fs::copy(source, &path).unwrap();
    let mut tag = Tag::new(TagType::VorbisComments);
    tag.insert_text(key, value.to_string());
    tag.save_to_path(&path, lofty::config::WriteOptions::default())
        .unwrap();
    temp
}

#[test]
fn scanner_reads_each_replaygain_tag_form_and_locale_comma() {
    let cases = [
        (
            ItemKey::ReplayGainTrackGain,
            "-7,32 dB",
            ReplayGainTags {
                track_gain_db: Some(-7.32),
                ..ReplayGainTags::default()
            },
        ),
        (
            ItemKey::ReplayGainTrackPeak,
            "0.8125",
            ReplayGainTags {
                track_peak: Some(0.8125),
                ..ReplayGainTags::default()
            },
        ),
        (
            ItemKey::ReplayGainAlbumGain,
            "+1.25",
            ReplayGainTags {
                album_gain_db: Some(1.25),
                ..ReplayGainTags::default()
            },
        ),
        (
            ItemKey::ReplayGainAlbumPeak,
            "0,95",
            ReplayGainTags {
                album_peak: Some(0.95),
                ..ReplayGainTags::default()
            },
        ),
    ];

    for (key, value, expected) in cases {
        let temp = tagged_fixture(key, value);
        let meta = track_meta::read_meta(&temp.path().join("tagged.flac")).unwrap();
        assert_eq!(meta.replay_gain, expected);
    }
}

#[test]
fn scanner_leaves_replaygain_empty_when_tags_are_missing() {
    let temp = tagged_fixture(ItemKey::TrackTitle, "No gain");
    let meta = track_meta::read_meta(&temp.path().join("tagged.flac")).unwrap();
    assert_eq!(meta.replay_gain, ReplayGainTags::default());
}

#[test]
fn scanner_converts_opus_r128_q7_8_gains_to_replaygain_reference() {
    let temp = tempfile::tempdir().unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    let path = temp.path().join("r128.flac");
    std::fs::copy(source, &path).unwrap();
    let mut tag = Tag::new(TagType::VorbisComments);
    tag.insert_text(ItemKey::R128TrackGain, "-256".to_string());
    tag.insert_text(ItemKey::R128AlbumGain, "256".to_string());
    tag.save_to_path(&path, lofty::config::WriteOptions::default())
        .unwrap();

    let meta = track_meta::read_meta(&path).unwrap();
    assert_eq!(
        meta.replay_gain,
        ReplayGainTags {
            track_gain_db: Some(4.0),
            album_gain_db: Some(6.0),
            ..ReplayGainTags::default()
        }
    );
}

#[test]
fn scanner_persists_replaygain_tags_and_current_scan_version() {
    let temp = tagged_fixture(ItemKey::ReplayGainTrackGain, "-4.5 dB");
    let database = crate::db::Db::open_in_memory().unwrap();

    super::tests::completed(scan_folder(&database, temp.path()).unwrap());

    let stored: (Option<f64>, Option<f64>, Option<f64>, Option<f64>, i64) = database
        .conn()
        .query_row(
            "SELECT rg_track_gain, rg_track_peak, rg_album_gain, rg_album_peak, tag_scan_version
             FROM tracks",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(stored, (Some(-4.5), None, None, None, 1));
}

#[test]
fn scanner_rereads_a_version_zero_row_exactly_once() {
    let temp = tagged_fixture(ItemKey::ReplayGainTrackGain, "-4.5 dB");
    let database = crate::db::Db::open_in_memory().unwrap();
    super::tests::completed(scan_folder(&database, temp.path()).unwrap());
    database
        .conn()
        .execute("UPDATE tracks SET tag_scan_version = 0", [])
        .unwrap();

    track_meta::READ_META_CALLS.with(|calls| calls.set(0));
    super::tests::completed(scan_folder(&database, temp.path()).unwrap());
    assert_eq!(track_meta::READ_META_CALLS.with(std::cell::Cell::get), 1);
    let version: i64 = database
        .conn()
        .query_row("SELECT tag_scan_version FROM tracks", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 1);

    track_meta::READ_META_CALLS.with(|calls| calls.set(0));
    super::tests::completed(scan_folder(&database, temp.path()).unwrap());
    assert_eq!(track_meta::READ_META_CALLS.with(std::cell::Cell::get), 0);
}
