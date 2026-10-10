use super::*;

const ADOPTED_LEVEL: f32 = 0.8;
const STORED_LEVEL: f32 = 0.01;
const SCENE_SIZE: f32 = 272.0;

#[test]
fn ac_29_stored_frames_wait_for_the_adopted_shape_until_the_stream_speaks() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = adopted_engine(clock.clone());
    let adopted = engine.current_bands();
    let adopted_scene = engine.scene(SCENE_SIZE, SCENE_SIZE);
    let flags = engine.ingest_flags_for_testing();

    engine.ingest_bands(stored_frame());

    assert_eq!(
        engine.scene(SCENE_SIZE, SCENE_SIZE),
        adopted_scene,
        "the encoded scene must not move for a blocked stored frame"
    );
    assert_ne!(
        adopted_scene,
        floor_scene(),
        "the control arm: an ingested stored frame draws a different scene"
    );
    assert_eq!(engine.ingest_flags_for_testing(), flags);

    clock.advance(Duration::from_millis(16));
    engine.tick();

    assert_eq!(engine.current_bands(), adopted);
}

#[test]
fn ac_29_stored_frames_take_over_once_the_grace_expires() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = adopted_engine(clock.clone());
    let adopted = engine.current_bands();
    let adopted_scene = engine.scene(SCENE_SIZE, SCENE_SIZE);

    clock.advance(Duration::from_millis(501));
    engine.ingest_bands(stored_frame());

    assert_ne!(engine.current_bands(), adopted);
    assert_bands_near(&engine.current_bands(), STORED_LEVEL);
    assert_ne!(engine.scene(SCENE_SIZE, SCENE_SIZE), adopted_scene);
}

#[test]
fn the_stored_frame_grace_lasts_exactly_half_a_second() {
    let held_clock = Arc::new(FakeMonotonicClock::default());
    let held = adopted_engine(held_clock.clone());
    let adopted = held.current_bands();
    held_clock.advance(Duration::from_millis(499));
    held.ingest_bands(stored_frame());
    assert_eq!(held.current_bands(), adopted, "499 ms is still inside");

    let released_clock = Arc::new(FakeMonotonicClock::default());
    let released = adopted_engine(released_clock.clone());
    released_clock.advance(Duration::from_millis(500));
    released.ingest_bands(stored_frame());
    assert_bands_near(&released.current_bands(), STORED_LEVEL);
}

#[test]
fn a_second_adoption_rearms_the_stored_frame_grace() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = adopted_engine(clock.clone());

    clock.advance(Duration::from_millis(400));
    engine.adopt_shape(vec![ADOPTED_LEVEL; SPECTRUM_BAND_COUNT]);
    let readopted = engine.current_bands();

    clock.advance(Duration::from_millis(400));
    engine.ingest_bands(stored_frame());
    assert_eq!(
        engine.current_bands(),
        readopted,
        "800 ms after the first adoption, 400 ms after the second"
    );

    clock.advance(Duration::from_millis(100));
    engine.ingest_bands(stored_frame());
    assert_bands_near(&engine.current_bands(), STORED_LEVEL);
}

#[test]
fn stored_frames_without_an_adoption_ingest_once_a_fresh_start_gave_up_waiting() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    engine.set_playing(true);

    clock.advance(Duration::from_millis(500));
    engine.ingest_bands(stored_frame());

    assert_bands_near(&engine.current_bands(), STORED_LEVEL);
}

#[test]
fn a_pause_blip_does_not_end_the_stored_frame_grace() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = adopted_engine(clock);
    let adopted = engine.current_bands();

    // NowPlayingScene re-sends setPlaying on every recomposition, and a
    // transient false through the item change is documented.
    engine.set_playing(false);
    engine.set_playing(true);
    engine.ingest_bands(stored_frame());

    assert_eq!(engine.current_bands(), adopted);
}

#[test]
fn a_track_change_ends_the_stored_frame_grace() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = adopted_engine(clock);

    engine.note_track_changed();
    engine.ingest_bands(stored_frame());

    assert_bands_near(&engine.current_bands(), STORED_LEVEL);
}

#[test]
fn ac_29_a_stream_reset_after_the_adoption_keeps_stored_frames_waiting() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = adopted_engine(clock);
    let adopted = engine.current_bands();

    engine.reset_audio_stream();
    let flags = engine.ingest_flags_for_testing();
    assert!(
        flags.awaiting_stream_after_reset,
        "the reset must leave the engine awaiting its stream"
    );
    engine.ingest_bands(stored_frame());

    assert_eq!(engine.current_bands(), adopted);
    assert_eq!(engine.ingest_flags_for_testing(), flags);
}

#[test]
fn a_frame_blocked_by_the_grace_is_drawn_by_the_first_tick_after_it_expires() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = adopted_engine(clock.clone());
    let adopted = engine.current_bands();

    clock.advance(Duration::from_millis(100));
    engine.ingest_bands(stored_frame());
    clock.advance(Duration::from_millis(100));
    engine.tick();
    assert_eq!(engine.current_bands(), adopted, "still inside the grace");

    // The playhead is stalled, so no further stored frame is sent.
    clock.advance(Duration::from_millis(400));
    assert!(engine.tick());

    let drawn = engine.current_bands();
    assert!(
        drawn.iter().all(|band| *band < ADOPTED_LEVEL / 4.0),
        "the remembered frame should replace the adopted shape, got {drawn:?}"
    );
}

#[test]
fn only_the_latest_blocked_frame_is_remembered() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = adopted_engine(clock.clone());

    engine.ingest_bands(vec![0.5; SPECTRUM_BAND_COUNT]);
    engine.ingest_bands(stored_frame());
    clock.advance(Duration::from_millis(500));
    engine.tick();

    assert!(engine
        .current_bands()
        .iter()
        .all(|band| *band < ADOPTED_LEVEL / 4.0));
}

#[test]
fn a_new_adoption_forgets_the_remembered_frame() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = adopted_engine(clock.clone());

    engine.ingest_bands(stored_frame());
    engine.adopt_shape(vec![ADOPTED_LEVEL; SPECTRUM_BAND_COUNT]);
    let readopted = engine.current_bands();
    clock.advance(Duration::from_millis(500));
    engine.tick();

    assert_eq!(engine.current_bands(), readopted);
}

#[test]
fn a_track_change_forgets_the_remembered_frame() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = adopted_engine(clock.clone());

    engine.ingest_bands(stored_frame());
    engine.note_track_changed();
    clock.advance(Duration::from_millis(600));
    engine.tick();

    assert!(
        engine.scene(SCENE_SIZE, SCENE_SIZE).is_empty(),
        "nothing was ingested for the new track"
    );
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

#[test]
fn live_pcm_forgets_the_remembered_frame() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = adopted_engine(clock.clone());
    let pcm = stereo_sine_pcm16(200.0, 48_000, 0, 800);

    engine.ingest_bands(stored_frame());
    assert!(engine.remembers_stored_frame_for_testing());
    ingest_one_live_block(&engine, &clock, &pcm, 48_000);

    assert!(!engine.remembers_stored_frame_for_testing());
}

fn adopted_engine(clock: Arc<FakeMonotonicClock>) -> AndroidVisualEngine {
    let engine = AndroidVisualEngine::with_clock(clock);
    engine.set_playing(true);
    engine.note_track_changed();
    engine.adopt_shape(vec![ADOPTED_LEVEL; SPECTRUM_BAND_COUNT]);
    engine
}

fn floor_scene() -> Vec<u8> {
    let engine = AndroidVisualEngine::new();
    engine.set_playing(true);
    engine.ingest_bands(stored_frame());
    engine.scene(SCENE_SIZE, SCENE_SIZE)
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
