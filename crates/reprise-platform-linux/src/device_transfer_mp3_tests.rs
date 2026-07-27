use std::sync::atomic::AtomicBool;

use lofty::picture::{MimeType, Picture, PictureType};
use lofty::prelude::*;
use lofty::tag::ItemKey;
use reprise_core::device_sync::{Mp3Quality, SyncTrack};

use super::{
    finish_output, probe_transcode_capability, transcode_audio, AudioMetadata, TranscodeError,
    TranscodeProfile, TranscodeRequest, REQUIRED_MP3_FACTORIES, REQUIRED_OPUS_FACTORIES,
};

const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x62, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

fn tagged_audio_fixture(directory: &std::path::Path) -> SyncTrack {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    let source = directory.join("tagged-source.flac");
    std::fs::copy(fixture, &source).unwrap();
    let mut tagged = lofty::read_from_path(&source).unwrap();
    let tag = tagged.primary_tag_mut().unwrap();
    tag.set_title("Fixture title".into());
    tag.set_artist("Fixture artist".into());
    tag.set_album("Fixture album".into());
    tag.insert_text(ItemKey::AlbumArtist, "Fixture album artist".into());
    tag.set_track(7);
    tag.push_picture(
        Picture::unchecked(TINY_PNG.to_vec())
            .pic_type(PictureType::CoverFront)
            .mime_type(MimeType::Png)
            .build(),
    );
    tag.save_to_path(&source, lofty::config::WriteOptions::default())
        .unwrap();
    SyncTrack {
        id: 7,
        source_path: source,
        original_name: "tagged-source.flac".into(),
        title: "Fixture title".into(),
        artist: "Fixture artist".into(),
        album: "Fixture album".into(),
        album_artist: "Fixture album artist".into(),
        track_number: Some(7),
        duration_ms: 1_161,
        bitrate_kbps: None,
        size_bytes: 12_066,
        source_mtime: 1,
    }
}

#[test]
fn capability_probe_covers_each_selected_lossy_profile() {
    assert!(REQUIRED_MP3_FACTORIES.contains(&"lamemp3enc"));
    assert!(REQUIRED_MP3_FACTORIES.contains(&"id3v2mux"));
    assert!(REQUIRED_OPUS_FACTORIES.contains(&"opusenc"));
    assert!(REQUIRED_OPUS_FACTORIES.contains(&"oggmux"));
    probe_transcode_capability(TranscodeProfile::Mp3(Mp3Quality::Kbps256)).unwrap();
    probe_transcode_capability(TranscodeProfile::Opus160).unwrap();
}

#[test]
fn real_audio_fixture_transcodes_to_mp3_with_required_tags_and_embedded_cover() {
    let directory = tempfile::tempdir().unwrap();
    let track = tagged_audio_fixture(directory.path());
    let output = directory.path().join("encoded.mp3");
    let metadata = AudioMetadata::for_track(&track);
    assert_eq!(metadata.cover.as_deref(), Some(TINY_PNG));
    let request = TranscodeRequest {
        source: track.source_path,
        output: output.clone(),
        profile: TranscodeProfile::Mp3(Mp3Quality::Kbps256),
        metadata,
    };

    let encoded = transcode_audio(&request, &AtomicBool::new(false)).unwrap();

    assert_eq!(encoded.path, output);
    assert!(encoded.size_bytes > 0);
    assert!(std::fs::read(&encoded.path).unwrap().starts_with(b"ID3"));
    let tags = reprise_core::library::tag_edit::read_editable_tags(&encoded.path).unwrap();
    assert_eq!(tags.title, "Fixture title");
    assert_eq!(tags.artist, "Fixture artist");
    assert_eq!(tags.album, "Fixture album");
    assert_eq!(tags.album_artist, "Fixture album artist");
    assert_eq!(tags.track_no, Some(7));
    assert_eq!(
        reprise_core::cover::read_cover_tag(&encoded.path)
            .picture
            .as_deref(),
        Some(TINY_PNG)
    );
}

#[test]
fn real_audio_fixture_transcodes_to_opus_160_with_required_tags_and_embedded_cover() {
    let directory = tempfile::tempdir().unwrap();
    let track = tagged_audio_fixture(directory.path());
    let output = directory.path().join("encoded.opus");
    let metadata = AudioMetadata::for_track(&track);
    let request = TranscodeRequest {
        source: track.source_path,
        output: output.clone(),
        profile: TranscodeProfile::Opus160,
        metadata,
    };

    let encoded = transcode_audio(&request, &AtomicBool::new(false)).unwrap();

    assert_eq!(encoded.path, output);
    assert!(encoded.size_bytes > 0);
    let tags = reprise_core::library::tag_edit::read_editable_tags(&encoded.path).unwrap();
    assert_eq!(tags.title, "Fixture title");
    assert_eq!(tags.artist, "Fixture artist");
    assert_eq!(tags.album, "Fixture album");
    assert_eq!(tags.album_artist, "Fixture album artist");
    assert_eq!(tags.track_no, Some(7));
    assert_eq!(
        reprise_core::cover::read_cover_tag(&encoded.path)
            .picture
            .as_deref(),
        Some(TINY_PNG)
    );
}

#[test]
fn pre_cancelled_transcode_leaves_no_output() {
    let directory = tempfile::tempdir().unwrap();
    let track = tagged_audio_fixture(directory.path());
    let output = directory.path().join("cancelled.opus");
    let metadata = AudioMetadata::for_track(&track);
    let request = TranscodeRequest {
        metadata,
        source: track.source_path,
        output: output.clone(),
        profile: TranscodeProfile::Opus160,
    };

    assert!(matches!(
        transcode_audio(&request, &AtomicBool::new(true)),
        Err(TranscodeError::Cancelled)
    ));
    assert!(!output.exists());
}

#[test]
fn cancellation_after_output_creation_removes_the_incomplete_local_file() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("incomplete.mp3");
    std::fs::write(&output, b"partial ID3 output").unwrap();

    assert!(matches!(
        finish_output(&output, Err(TranscodeError::Cancelled)),
        Err(TranscodeError::Cancelled)
    ));
    assert!(!output.exists());
}
