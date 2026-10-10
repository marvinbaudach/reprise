//! Repeated FLAC starts for CUE tracks (PLAY-22).
//!
//! These exercise the actual `playbin3`/`flacparse` path. An error here is
//! the backend event that makes the frontend skip an existing track.

use super::segment_support::{cue_item, encode_flac, gain_element, write_regions_wav, Harness};
use super::*;
use std::sync::atomic::AtomicUsize;

const HANG_GUARD: Duration = Duration::from_secs(10);
const ZERO_START_CUE: (i64, i64) = (0, 800);
const SEEKED_CUE: (i64, i64) = (200, 800);
const DEFAULT_ATTEMPTS: usize = 16;

#[derive(Default, Debug)]
struct Outcomes {
    attempts: usize,
    errors: usize,
    finishes: usize,
}

impl Outcomes {
    fn record(&mut self, events: &[PlayerEvent]) {
        self.attempts += 1;
        self.errors += usize::from(
            events
                .iter()
                .any(|event| matches!(event, PlayerEvent::Error(_))),
        );
        self.finishes += usize::from(
            events
                .iter()
                .any(|event| matches!(event, PlayerEvent::TrackFinished)),
        );
    }

    fn assert_clean(&self, arm: &str) {
        assert_eq!(
            self.errors, 0,
            "{arm}: {self:?}; a backend error makes the frontend skip the existing track"
        );
        assert_eq!(
            self.finishes, self.attempts,
            "{arm}: every start must reach its own finish: {self:?}"
        );
    }
}

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn attempts() -> usize {
    std::env::var("REPRISE_PLAY_22_ATTEMPTS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_ATTEMPTS)
}

/// Writes the shape that exposed the parser failure: a file long enough that
/// the first CUE track has a real boundary, with no FLAC seek table. Encoding
/// runs as fast as the pipeline can consume the generated WAV; it never uses
/// the audio sink or the wall clock.
fn write_cue_flac(directory: &tempfile::TempDir) -> std::path::PathBuf {
    let wav = directory.path().join("album.wav");
    let flac = directory.path().join("album.flac");
    write_regions_wav(&wav, &[(32_000, true)]);

    encode_flac(&wav, &flac);
    flac
}

fn run_to_finish_or_error(harness: &Harness) -> Vec<PlayerEvent> {
    harness.pump_until(HANG_GUARD, |events| {
        events
            .iter()
            .any(|event| matches!(event, PlayerEvent::TrackFinished | PlayerEvent::Error(_)))
    })
}

#[test]
fn play_22_a_zero_start_does_not_flush_the_prerolled_flac_parser() {
    let segment_events = Arc::new(AtomicUsize::new(0));
    let harness = Harness::new();
    let counted = segment_events.clone();
    gain_element(&harness.player)
        .static_pad("src")
        .unwrap()
        .add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_, info| {
            if matches!(
                &info.data,
                Some(gst::PadProbeData::Event(event))
                    if matches!(event.view(), gst::EventView::Segment(_))
            ) {
                counted.fetch_add(1, Ordering::SeqCst);
            }
            gst::PadProbeReturn::Ok
        });

    let album = fixture("sine.flac");
    harness
        .player
        .play(cue_item(&album, ZERO_START_CUE, 0.0))
        .unwrap();
    let events = run_to_finish_or_error(&harness);

    assert!(
        events
            .iter()
            .all(|event| !matches!(event, PlayerEvent::Error(_))),
        "the CUE start must not error: {events:?}"
    );
    assert_eq!(
        segment_events.load(Ordering::SeqCst),
        1,
        "starting at the parser's current zero position must not send a flushing seek"
    );
}

#[test]
fn play_22_a_cue_track_is_not_announced_playing_until_its_start_seek_lands() {
    let segment_events = Arc::new(AtomicUsize::new(0));
    let segments_at_playing = Arc::new(AtomicUsize::new(0));
    let harness = Harness::observing({
        let segment_events = segment_events.clone();
        let segments_at_playing = segments_at_playing.clone();
        move |event| {
            if matches!(event, PlayerEvent::StateChanged(PlaybackState::Playing)) {
                segments_at_playing.store(segment_events.load(Ordering::SeqCst), Ordering::SeqCst);
            }
        }
    });
    let counted = segment_events.clone();
    gain_element(&harness.player)
        .static_pad("src")
        .unwrap()
        .add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_, info| {
            if matches!(
                &info.data,
                Some(gst::PadProbeData::Event(event))
                    if matches!(event.view(), gst::EventView::Segment(_))
            ) {
                counted.fetch_add(1, Ordering::SeqCst);
            }
            gst::PadProbeReturn::Ok
        });

    let album = fixture("sine.flac");
    harness.start(|| {
        harness
            .player
            .play(cue_item(&album, SEEKED_CUE, 0.0))
            .unwrap();
    });
    let events = run_to_finish_or_error(&harness);

    assert!(
        events
            .iter()
            .all(|event| !matches!(event, PlayerEvent::Error(_))),
        "the CUE start must not error: {events:?}"
    );
    assert!(
        segments_at_playing.load(Ordering::SeqCst) >= 2,
        "Playing was announced after preroll but before the start seek landed"
    );
}

#[test]
fn play_22_flac_cue_tracks_start_reliably_cold_and_after_a_whole_file() {
    let harness = Harness::new();
    let directory = tempfile::tempdir().unwrap();
    let album = write_cue_flac(&directory);
    let whole = fixture("blip.flac");
    let attempts = attempts();

    let mut whole_file_control = Outcomes::default();
    for _ in 0..attempts {
        harness.player.play(item(whole.to_str().unwrap())).unwrap();
        whole_file_control.record(&run_to_finish_or_error(&harness));
    }

    let mut cold_cue = Outcomes::default();
    for _ in 0..attempts {
        harness.player.stop().unwrap();
        harness
            .player
            .play(cue_item(&album, ZERO_START_CUE, 0.0))
            .unwrap();
        cold_cue.record(&run_to_finish_or_error(&harness));
    }

    let mut hard_change_cue = Outcomes::default();
    for _ in 0..attempts {
        harness.player.play(item(whole.to_str().unwrap())).unwrap();
        let whole_events = run_to_finish_or_error(&harness);
        assert!(
            whole_events
                .iter()
                .any(|event| matches!(event, PlayerEvent::TrackFinished)),
            "the whole-file setup arm must reach EOS: {whole_events:?}"
        );
        harness
            .player
            .play(cue_item(&album, ZERO_START_CUE, 0.0))
            .unwrap();
        hard_change_cue.record(&run_to_finish_or_error(&harness));
    }

    eprintln!(
        "PLAY-22 repeated FLAC starts: control={whole_file_control:?}, \
         cold={cold_cue:?}, hard_change={hard_change_cue:?}"
    );
    whole_file_control.assert_clean("whole-file control");
    cold_cue.assert_clean("cold CUE start");
    hard_change_cue.assert_clean("CUE start after whole-file EOS");
}
