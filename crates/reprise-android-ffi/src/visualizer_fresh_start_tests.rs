//! A fresh start must not flash the stored spectrogram's frame (#1181).
//!
//! The stored analysis is normalised on its own scale, roughly twice the live
//! engine's settled level. Drawn between the press on play and the first PCM
//! block it is a one-frame flash, and the peak caps it raises outlive it.

use super::*;

const STORED_LEVEL: f32 = 0.65;
const DEVICE_BLOCK_FRAMES: usize = 800;
const PAUSED_TICKS: usize = 120;
const FRAME: Duration = Duration::from_millis(16);
const SCENE_SIZE: f32 = 272.0;

fn stored_frame() -> Vec<f32> {
    vec![STORED_LEVEL; SPECTRUM_BAND_COUNT]
}

fn highest(bands: &[f32]) -> f32 {
    bands.iter().copied().fold(0.0, f32::max)
}

/// The issue's recipe: a seek while paused ingests a stored frame, play
/// follows, and the playhead keeps sending a stored frame about every third
/// display tick until the first PCM block arrives.
fn press_play_on_a_fresh_engine(
    engine: &AndroidVisualEngine,
    clock: &FakeMonotonicClock,
    ticks_before_pcm: usize,
) -> f32 {
    engine.set_playing(false);
    engine.ingest_bands(stored_frame());
    // The paused view ticks on, so its resting wave has taken over the display.
    for _ in 0..PAUSED_TICKS {
        clock.advance(FRAME);
        engine.tick();
    }
    engine.set_playing(true);
    let mut highest_before_pcm = highest(&engine.current_bands());
    for tick in 0..ticks_before_pcm {
        if tick % 3 == 0 {
            engine.ingest_bands(stored_frame());
        }
        clock.advance(FRAME);
        engine.tick();
        highest_before_pcm = highest_before_pcm.max(highest(&engine.current_bands()));
    }
    highest_before_pcm
}

fn feed_live_tone(engine: &AndroidVisualEngine, clock: &FakeMonotonicClock, blocks: usize) {
    for chunk in 0..blocks {
        let pcm = stereo_sine_pcm16(2_000.0, 48_000, chunk, DEVICE_BLOCK_FRAMES);
        ingest_one_live_block(engine, clock, &pcm, 48_000);
    }
}

#[test]
fn a_fresh_start_does_not_draw_the_stored_frame_before_the_first_pcm() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());

    let highest_before_pcm = press_play_on_a_fresh_engine(&engine, &clock, 6);

    assert!(
        highest_before_pcm < STORED_LEVEL / 2.0,
        "the stored frame flashed at {highest_before_pcm} before any PCM arrived"
    );
}

#[test]
fn a_fresh_start_leaves_no_trace_of_the_stored_frame_once_live_audio_speaks() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    press_play_on_a_fresh_engine(&engine, &clock, 6);

    // Control arm: the same engine start without any stored frame at all.
    let control_clock = Arc::new(FakeMonotonicClock::default());
    let control = AndroidVisualEngine::with_clock(control_clock.clone());
    control.set_playing(false);
    for _ in 0..PAUSED_TICKS {
        control_clock.advance(FRAME);
        control.tick();
    }
    control.set_playing(true);
    for _ in 0..6 {
        control_clock.advance(FRAME);
        control.tick();
    }

    for chunk in 0..30 {
        let pcm = stereo_sine_pcm16(2_000.0, 48_000, chunk, DEVICE_BLOCK_FRAMES);
        ingest_one_live_block(&engine, &clock, &pcm, 48_000);
        ingest_one_live_block(&control, &control_clock, &pcm, 48_000);
        assert_eq!(
            engine.scene(SCENE_SIZE, SCENE_SIZE),
            control.scene(SCENE_SIZE, SCENE_SIZE),
            "block {chunk}: the bars or the peak caps still carry the stored frame"
        );
    }
}

#[test]
fn stored_frames_draw_after_half_a_second_without_pcm() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    press_play_on_a_fresh_engine(&engine, &clock, 6);

    clock.advance(Duration::from_millis(500));
    engine.ingest_bands(stored_frame());
    clock.advance(FRAME);
    engine.tick();

    assert!(
        (highest(&engine.current_bands()) - STORED_LEVEL).abs() < 1e-3,
        "a device that never delivers PCM falls back to the stored frames"
    );
}

#[test]
fn a_stored_frame_ingested_before_play_draws_when_no_pcm_comes() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    engine.ingest_bands(stored_frame());
    engine.set_playing(true);

    // The playhead is stalled, so no further frame is sent: the release must
    // come from the display tick alone.
    clock.advance(Duration::from_millis(600));
    assert!(engine.tick());

    assert!((highest(&engine.current_bands()) - STORED_LEVEL).abs() < 1e-3);
}

#[test]
fn pausing_ends_the_fresh_start_hold() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    engine.set_playing(true);
    engine.set_playing(false);

    engine.ingest_bands(stored_frame());

    assert!(
        !engine.remembers_stored_frame_for_testing(),
        "a paused engine ingests a seek's stored frame at once"
    );
}

#[test]
fn a_warm_resume_is_not_held() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    engine.set_playback_intended(true);
    engine.set_playing(true);
    feed_live_tone(&engine, &clock, 100);
    let live_shape = engine.current_bands();
    assert!(highest(&live_shape) > 0.3, "the warm-up should draw bars");

    engine.set_playback_intended(false);
    engine.set_playing(false);
    engine.set_playback_intended(true);
    engine.set_playing(true);
    engine.ingest_bands(stored_frame());

    assert!(
        engine.has_live_audio(),
        "a pause keeps the live stream the resume continues"
    );
    assert!(!engine.remembers_stored_frame_for_testing());
}

#[test]
fn an_adoption_before_play_keeps_its_own_grace() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    engine.adopt_shape(vec![0.8; SPECTRUM_BAND_COUNT]);
    engine.set_playing(true);
    let adopted = engine.current_bands();

    engine.ingest_bands(stored_frame());

    assert_eq!(engine.current_bands(), adopted);
    assert!(
        highest(&adopted) > 0.5,
        "the swipe's adopted shape stays on screen, it is not hidden by the hold"
    );
}
