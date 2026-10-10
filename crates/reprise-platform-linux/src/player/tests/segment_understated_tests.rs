//! The last track of a file whose header understates its length (CUE-19),
//! against a real `playbin3` on `fakesink`.
//!
//! The file holds six seconds; its FLAC header claims four. The library has
//! learned the real length by decoding it, so the track is handed over as
//! `(2000, 6000)` while the pipeline still reads four seconds off the header.
//! The track must report and seek in the length the library knows.

use super::segment_support::{
    cue_item, encode_flac, record_heard, ticks, write_regions_wav, Harness,
};
use super::*;

const REAL_FILE_MS: u32 = 6_000;
const CLAIMED_FILE_MS: u32 = 4_000;
const TRACK_START_MS: i64 = 2_000;
const SETTLE: Duration = Duration::from_secs(20);
const SEEKS_AFTER_START: usize = 2;
/// Where the FLAC header's total sample count starts: the `fLaC` marker, the
/// metadata block header, and ten bytes of `STREAMINFO` before the 64 bits
/// holding sample rate, channels, bits per sample and the count.
const TOTAL_SAMPLES_FIELD_OFFSET: usize = 4 + 4 + 10;
const TOTAL_SAMPLES_MASK: u64 = (1 << 36) - 1;
const SAMPLE_RATE: u64 = 44_100;
/// How far a seek may land from where it was asked to.
const SEEK_TOLERANCE_MS: i64 = 50;

/// A silent two seconds, then a tone to the end of `REAL_FILE_MS`, as FLAC with
/// a header that says the file is `CLAIMED_FILE_MS` long.
fn flac_with_an_understated_header(directory: &tempfile::TempDir) -> std::path::PathBuf {
    let wav = directory.path().join("album.wav");
    let flac = directory.path().join("album.flac");
    write_regions_wav(
        &wav,
        &[
            (TRACK_START_MS as u32, false),
            (REAL_FILE_MS - TRACK_START_MS as u32, true),
        ],
    );
    encode_flac(&wav, &flac);

    let mut bytes = std::fs::read(&flac).unwrap();
    let field = TOTAL_SAMPLES_FIELD_OFFSET..TOTAL_SAMPLES_FIELD_OFFSET + 8;
    let packed = u64::from_be_bytes(bytes[field.clone()].try_into().unwrap());
    let real_samples = packed & TOTAL_SAMPLES_MASK;
    assert_eq!(
        real_samples,
        SAMPLE_RATE * u64::from(REAL_FILE_MS) / 1_000,
        "the encoder wrote the count where the fixture expects it"
    );
    let claimed_samples = SAMPLE_RATE * u64::from(CLAIMED_FILE_MS) / 1_000;
    let patched = (packed & !TOTAL_SAMPLES_MASK) | claimed_samples;
    bytes[field].copy_from_slice(&patched.to_be_bytes());
    std::fs::write(&flac, bytes).unwrap();
    flac
}

#[test]
fn cue_19_a_track_longer_than_its_header_reports_and_plays_its_whole_length() {
    let harness = Harness::new();
    let directory = tempfile::tempdir().unwrap();
    let album = flac_with_an_understated_header(&directory);
    let learned_end_ms = i64::from(REAL_FILE_MS);
    let own_length_ms = learned_end_ms - TRACK_START_MS;

    harness.start(|| {
        harness
            .player
            .play(cue_item(&album, (TRACK_START_MS, learned_end_ms), 0.0))
            .unwrap();
    });
    // Past the two seconds the header leaves the track.
    let events = harness.pump_until(SETTLE, |events| {
        ticks(events)
            .iter()
            .any(|&(position_ms, _)| position_ms > own_length_ms - 1_000)
    });

    let ticks = ticks(&events);
    assert!(
        ticks
            .iter()
            .any(|&(position_ms, _)| position_ms > own_length_ms - 1_000),
        "the clock never passed the header's end, ticks were {ticks:?}"
    );
    for &(_, duration_ms) in &ticks {
        assert!(
            (own_length_ms - 100..=own_length_ms + 100).contains(&duration_ms),
            "the track lasts {own_length_ms} ms, ticks were {ticks:?}"
        );
    }
}

#[test]
fn cue_19_the_second_half_of_a_track_longer_than_its_header_can_be_sought() {
    let harness = Harness::new();
    let directory = tempfile::tempdir().unwrap();
    let album = flac_with_an_understated_header(&directory);
    let heard = record_heard(&harness.player);
    let seek_ms = 3_000;

    harness.start(|| {
        harness
            .player
            .play(cue_item(
                &album,
                (TRACK_START_MS, i64::from(REAL_FILE_MS)),
                0.0,
            ))
            .unwrap();
    });
    harness.pump_until(SETTLE, |_| heard.landings() >= SEEKS_AFTER_START);
    harness.player.seek_to(seek_ms).unwrap();
    harness.pump_until(SETTLE, |_| heard.landings() > SEEKS_AFTER_START);

    let landed = heard.latest_landing();
    assert!(
        (landed.start_ms - (TRACK_START_MS + seek_ms)).abs() <= SEEK_TOLERANCE_MS,
        "a seek to {seek_ms} ms of the track must land at {} ms of the file, got {landed:?}",
        TRACK_START_MS + seek_ms
    );
}
