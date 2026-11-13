//! Repeated FLAC starts for CUE tracks (PLAY-22).
//!
//! These exercise the actual `playbin3`/`flacparse` path. An error here is
//! the backend event that makes the frontend skip an existing track.

use super::segment_support::{
    count, cue_item, encode_flac, gain_of, on_every_pipeline, write_regions_wav, Harness,
};
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
    // On the pipeline of every start: each start builds a fresh one.
    on_every_pipeline(&harness.player, move |playbin| {
        let counted = counted.clone();
        gain_of(playbin).static_pad("src").unwrap().add_probe(
            gst::PadProbeType::EVENT_DOWNSTREAM,
            move |_, info| {
                if matches!(
                    &info.data,
                    Some(gst::PadProbeData::Event(event))
                        if matches!(event.view(), gst::EventView::Segment(_))
                ) {
                    counted.fetch_add(1, Ordering::SeqCst);
                }
                gst::PadProbeReturn::Ok
            },
        );
    });

    let album = fixture("sine.flac");
    // A start that hangs in READY is begun again; the SEGMENT events the
    // abandoned attempt sent must not count towards the one that succeeds, and
    // its events (an `Error` included) must not reach the assertions below.
    harness.start_resetting(
        || segment_events.store(0, Ordering::SeqCst),
        || {
            harness
                .player
                .play(cue_item(&album, ZERO_START_CUE, 0.0))
                .unwrap();
        },
    );
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
    // On the pipeline of every start: each start builds a fresh one.
    on_every_pipeline(&harness.player, move |playbin| {
        let counted = counted.clone();
        gain_of(playbin).static_pad("src").unwrap().add_probe(
            gst::PadProbeType::EVENT_DOWNSTREAM,
            move |_, info| {
                if matches!(
                    &info.data,
                    Some(gst::PadProbeData::Event(event))
                        if matches!(event.view(), gst::EventView::Segment(_))
                ) {
                    counted.fetch_add(1, Ordering::SeqCst);
                }
                gst::PadProbeReturn::Ok
            },
        );
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
    // It plays directly and counts on the watchdog to recover a real hang
    // inside `HANG_GUARD`, which the harness's later deadline would not leave
    // room for.
    let harness = Harness::recovering_hung_starts_itself();
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

/// Where the tone of the file starts, and where the CUE track of this test
/// does: a nonzero start, so the start completes through a seek.
const PAUSED_CUE: (i64, i64) = (2_000, 4_000);

/// A CUE start the user paused before its preroll finished stays paused after
/// `complete_start`: it enters the state last asked for, not `Playing` out of
/// habit. A pause toggle that follows resumes it.
#[test]
fn play_22_a_cue_start_paused_before_its_preroll_stays_paused() {
    use super::start_watchdog_tests::state_of;

    let harness = Harness::new();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("album.wav");
    write_regions_wav(&path, &[(2_000, false), (2_000, true)]);

    harness.start(|| {
        harness
            .player
            .play(cue_item(&path, PAUSED_CUE, 0.0))
            .unwrap();
        assert_eq!(
            harness.player.toggle_pause().unwrap(),
            PlaybackState::Paused
        );
    });
    // The preroll, the start seek and its own landing all pass in this time.
    let events = harness.pump_for(Duration::from_millis(1_500));
    let settled = state_of(&harness.player);
    let playing_events = count(&events, |event| {
        matches!(event, PlayerEvent::StateChanged(PlaybackState::Playing))
    });

    assert_eq!(
        settled,
        (gst::State::Paused, gst::State::VoidPending),
        "the start must finish in the state the user asked for"
    );
    assert_eq!(playing_events, 0, "events: {events:?}");

    assert_eq!(
        harness.player.toggle_pause().unwrap(),
        PlaybackState::Playing
    );
    harness.pump_until(HANG_GUARD, |_| {
        state_of(&harness.player).0 == gst::State::Playing
    });
    assert_eq!(state_of(&harness.player).0, gst::State::Playing);
}
