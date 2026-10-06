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
            ItemKey::ReplayGainTrackGain,
            "-7.32 DB",
            ReplayGainTags {
                track_gain_db: Some(-7.32),
                ..ReplayGainTags::default()
            },
        ),
        (
            ItemKey::ReplayGainAlbumGain,
            "+2.5dB",
            ReplayGainTags {
                album_gain_db: Some(2.5),
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
    assert_eq!(
        stored,
        (Some(-4.5), None, None, None, super::TAG_SCAN_VERSION)
    );
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
    assert_eq!(version, super::TAG_SCAN_VERSION);

    track_meta::READ_META_CALLS.with(|calls| calls.set(0));
    super::tests::completed(scan_folder(&database, temp.path()).unwrap());
    assert_eq!(track_meta::READ_META_CALLS.with(std::cell::Cell::get), 0);
}

const OGG_CRC_POLYNOMIAL: u32 = 0x04c1_1db7;

fn ogg_crc(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0_u32, |mut crc, byte| {
        crc ^= u32::from(*byte) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ OGG_CRC_POLYNOMIAL
            } else {
                crc << 1
            };
        }
        crc
    })
}

/// One Ogg page holding a single packet shorter than 255 bytes.
fn ogg_page(header_type: u8, granule: u64, sequence: u32, packet: &[u8]) -> Vec<u8> {
    assert!(packet.len() < 255, "a one-segment packet");
    let mut page = b"OggS".to_vec();
    page.push(0);
    page.push(header_type);
    page.extend_from_slice(&granule.to_le_bytes());
    page.extend_from_slice(&1_u32.to_le_bytes());
    page.extend_from_slice(&sequence.to_le_bytes());
    page.extend_from_slice(&[0; 4]);
    page.push(1);
    page.push(packet.len() as u8);
    page.extend_from_slice(packet);
    let crc = ogg_crc(&page);
    page[22..26].copy_from_slice(&crc.to_le_bytes());
    page
}

/// A real Ogg Opus stream: OpusHead, an OpusTags packet carrying the given
/// comments, and one 20 ms silent frame. Written by hand so the test needs no
/// encoder and no binary fixture.
fn opus_with_comments(comments: &[&str]) -> Vec<u8> {
    let mut head = b"OpusHead".to_vec();
    head.extend_from_slice(&[1, 2]);
    head.extend_from_slice(&312_u16.to_le_bytes());
    head.extend_from_slice(&48_000_u32.to_le_bytes());
    head.extend_from_slice(&0_i16.to_le_bytes());
    head.push(0);
    let vendor = b"reprise-test";
    let mut tags = b"OpusTags".to_vec();
    tags.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    tags.extend_from_slice(vendor);
    tags.extend_from_slice(&(comments.len() as u32).to_le_bytes());
    for comment in comments {
        tags.extend_from_slice(&(comment.len() as u32).to_le_bytes());
        tags.extend_from_slice(comment.as_bytes());
    }
    let silent_frame = [0xf8, 0xff, 0xfe];
    [
        ogg_page(0x02, 0, 0, &head),
        ogg_page(0x00, 0, 1, &tags),
        ogg_page(0x04, 312 + 960, 2, &silent_frame),
    ]
    .concat()
}

#[test]
fn scanner_reads_r128_gains_from_a_real_opus_comment_header() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("r128.opus");
    std::fs::write(
        &path,
        opus_with_comments(&["TITLE=Opus", "R128_TRACK_GAIN=-256", "R128_ALBUM_GAIN=256"]),
    )
    .unwrap();

    let meta = track_meta::read_meta(&path).unwrap();

    assert_eq!(meta.title, "Opus");
    assert_eq!(
        meta.replay_gain,
        ReplayGainTags {
            track_gain_db: Some(4.0),
            album_gain_db: Some(6.0),
            ..ReplayGainTags::default()
        }
    );
}

/// Two MPEG-1 Layer III frames of silence (128 kbit/s, 44.1 kHz, stereo).
fn silent_mp3() -> Vec<u8> {
    let frame_len = 417;
    (0..4)
        .flat_map(|_| {
            let mut frame = vec![0_u8; frame_len];
            frame[..4].copy_from_slice(&[0xff, 0xfb, 0x90, 0x00]);
            frame
        })
        .collect()
}

#[test]
fn scanner_reads_replaygain_from_a_secondary_tag() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("two-tags.mp3");
    std::fs::write(&path, silent_mp3()).unwrap();
    let mut id3 = Tag::new(TagType::Id3v2);
    id3.insert_text(ItemKey::TrackTitle, "Primary tag".to_string());
    id3.save_to_path(&path, lofty::config::WriteOptions::default())
        .unwrap();
    let mut ape = Tag::new(TagType::Ape);
    ape.insert_text(ItemKey::ReplayGainTrackGain, "-3.5 dB".to_string());
    ape.save_to_path(&path, lofty::config::WriteOptions::default())
        .unwrap();

    let meta = track_meta::read_meta(&path).unwrap();

    assert_eq!(meta.title, "Primary tag");
    assert_eq!(meta.replay_gain.track_gain_db, Some(-3.5));
}
