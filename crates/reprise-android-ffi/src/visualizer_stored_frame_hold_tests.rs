use super::*;

const ADOPTED_LEVEL: f32 = 0.8;
const STORED_LEVEL: f32 = 0.01;

#[test]
fn ac_29_stored_frames_wait_for_the_adopted_shape_until_the_stream_speaks() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = adopted_engine(clock.clone());
    let adopted = engine.current_bands();

    engine.ingest_bands(stored_frame());
    clock.advance(Duration::from_millis(16));
    engine.tick();

    assert_eq!(engine.current_bands(), adopted);
}

#[test]
fn ac_29_stored_frames_take_over_once_the_grace_expires() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = adopted_engine(clock.clone());
    let adopted = engine.current_bands();

    clock.advance(Duration::from_millis(501));
    engine.ingest_bands(stored_frame());

    assert_ne!(engine.current_bands(), adopted);
    assert_bands_near(&engine.current_bands(), STORED_LEVEL);
}

#[test]
fn stored_frames_without_an_adoption_ingest_at_once() {
    let engine = AndroidVisualEngine::new();
    engine.set_playing(true);

    engine.ingest_bands(stored_frame());

    assert_bands_near(&engine.current_bands(), STORED_LEVEL);
}

#[test]
fn a_pause_ends_the_stored_frame_grace() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = adopted_engine(clock);
    let adopted = engine.current_bands();

    engine.set_playing(false);
    engine.ingest_bands(stored_frame());

    assert_ne!(engine.current_bands(), adopted);
    assert_bands_near(&engine.current_bands(), STORED_LEVEL);
}

#[test]
fn ac_29_a_stream_reset_after_the_adoption_keeps_stored_frames_waiting() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = adopted_engine(clock);
    let adopted = engine.current_bands();

    engine.reset_audio_stream();
    engine.ingest_bands(stored_frame());

    assert_eq!(engine.current_bands(), adopted);
}

#[test]
fn live_pcm_after_the_adoption_keeps_the_pcm_hold_rules() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = adopted_engine(clock.clone());
    let adopted = engine.current_bands();
    let pcm = stereo_sine_pcm16(200.0, 48_000, 0, 800);

    ingest_one_live_block(&engine, &clock, &pcm, 48_000);
    assert_eq!(
        engine.current_bands(),
        adopted,
        "the existing PCM hold should keep the adopted shape"
    );

    let after_pcm = engine.current_bands();
    engine.ingest_bands(stored_frame());
    assert_eq!(
        engine.current_bands(),
        after_pcm,
        "live PCM should keep stored frames from replacing the display"
    );
}

fn adopted_engine(clock: Arc<FakeMonotonicClock>) -> AndroidVisualEngine {
    let engine = AndroidVisualEngine::with_clock(clock);
    engine.set_playing(true);
    engine.note_track_changed();
    engine.adopt_shape(vec![ADOPTED_LEVEL; SPECTRUM_BAND_COUNT]);
    engine
}

fn stored_frame() -> Vec<f32> {
    vec![STORED_LEVEL; SPECTRUM_BAND_COUNT]
}

fn assert_bands_near(bands: &[f32], expected: f32) {
    assert!(
        bands.iter().all(|band| (*band - expected).abs() < 1e-4),
        "expected every band near {expected}, got {bands:?}"
    );
}
