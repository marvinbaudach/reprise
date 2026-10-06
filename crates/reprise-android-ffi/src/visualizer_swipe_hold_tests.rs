use super::*;

const DEVICE_TICK_FRAMES: usize = 800;

/// A device-paced warm-up: one 60 fps tick of a 200 Hz tone per chunk.
fn play_live_tone(engine: &AndroidVisualEngine, clock: &FakeMonotonicClock) {
    engine.set_playback_intended(true);
    engine.set_playing(true);
    for chunk in 0..200 {
        let pcm = stereo_sine_pcm16(200.0, 48_000, chunk, DEVICE_TICK_FRAMES);
        ingest_one_live_block(engine, clock, &pcm, 48_000);
    }
}

fn run_display_ticks(engine: &AndroidVisualEngine, clock: &FakeMonotonicClock, count: usize) {
    for _ in 0..count {
        clock.advance(Duration::from_millis(16));
        engine.tick();
    }
}

fn largest_difference(left: &[f32], right: &[f32]) -> f32 {
    left.iter()
        .zip(right)
        .map(|(left, right)| (left - right).abs())
        .fold(0.0, f32::max)
}

#[test]
fn ac_29_adoptable_bands_survive_a_stop_of_the_old_stream() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    play_live_tone(&engine, &clock);
    let live_shape = engine.current_bands();
    assert!(
        live_shape.iter().any(|band| *band > 0.3),
        "the warm-up should put real energy on screen: {live_shape:?}"
    );

    // The old audio stops before the new panel composes (the `slow-next` case).
    engine.set_playing(false);
    run_display_ticks(&engine, &clock, 120);

    assert!(
        largest_difference(&engine.current_bands(), &live_shape) > 0.05,
        "the displayed bars should have decayed away from the live shape"
    );
    assert_eq!(
        engine.adoptable_bands(),
        live_shape,
        "the shape handed to the next song must be the last live one, not the decayed display"
    );
}

#[test]
fn ac_29_a_long_stopped_song_does_not_hand_its_old_shape_to_a_later_swipe() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    play_live_tone(&engine, &clock);

    // The queue ended or the stream stalled with playback still intended, so
    // no pause cleared the shape; minutes later the user swipes.
    engine.set_playing(false);
    clock.advance(Duration::from_secs(120));
    engine.tick();

    assert_eq!(
        engine.adoptable_bands(),
        engine.current_bands(),
        "a shape the viewer saw fall away minutes ago must not pop back on screen"
    );
}

#[test]
fn ac_29_the_last_live_shape_stays_adoptable_until_the_staleness_and_answer_grace_pass() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    play_live_tone(&engine, &clock);
    let live_shape = engine.current_bands();
    engine.set_playing(false);

    clock.advance(ADOPTABLE_SHAPE_MAX_AGE);
    assert_eq!(engine.adoptable_bands(), live_shape);
    assert!(engine.adoptable_bands_are_live());

    clock.advance(Duration::from_millis(1));
    engine.tick();
    assert_eq!(engine.adoptable_bands(), engine.current_bands());
    assert_ne!(engine.adoptable_bands(), live_shape);
    assert!(!engine.adoptable_bands_are_live());
}

#[test]
fn ac_29_the_adoption_source_flag_names_the_fallback_without_live_audio() {
    let engine = AndroidVisualEngine::new();
    engine.set_playing(true);
    engine.ingest_bands(vec![0.4; 24]);

    assert!(!engine.adoptable_bands_are_live());
}

#[test]
fn ac_29_a_track_change_keeps_the_last_live_shape_for_adoption() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    play_live_tone(&engine, &clock);
    let live_shape = engine.current_bands();

    engine.note_track_changed();

    assert!(
        engine.current_bands().iter().all(|band| *band == 0.0),
        "the track change itself still clears the display"
    );
    assert_eq!(engine.adoptable_bands(), live_shape);

    engine.adopt_shape(engine.adoptable_bands());
    engine.set_playing(true);
    assert_eq!(
        engine.current_bands(),
        live_shape,
        "adopting the handed-over shape puts the old song's bars on screen"
    );
}

#[test]
fn ac_29_adoptable_bands_fall_back_to_the_display_without_live_audio() {
    let engine = AndroidVisualEngine::new();
    engine.set_playing(true);
    engine.ingest_bands(vec![0.4; 24]);

    assert_eq!(engine.adoptable_bands(), engine.current_bands());
}

#[test]
fn ac_29_a_stored_analysis_frame_replaces_the_last_live_shape_for_adoption() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    play_live_tone(&engine, &clock);
    engine.note_track_changed();

    // The next song has no live audio, only its stored analysis; a later swipe
    // must hand on what that song showed, not the song before it.
    engine.ingest_bands(vec![0.3; 24]);

    let adoptable = engine.adoptable_bands();
    assert_eq!(adoptable, engine.current_bands());
    assert!(
        adoptable.iter().all(|band| (band - 0.3).abs() < 1e-4),
        "the stored-analysis display is what the viewer saw: {adoptable:?}"
    );
}

#[test]
fn ac_29_an_empty_analysis_frame_ends_the_stream_hold() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    play_live_tone(&engine, &clock);
    engine.reset_audio_stream();

    // The new stream spoke, and what it said was "nothing to show".
    engine.ingest_bands(Vec::new());
    engine.set_playing(true);
    run_display_ticks(&engine, &clock, 60);

    assert!(
        engine.current_bands().iter().any(|band| *band > 0.01),
        "an empty frame left the post-reset hold set, so the engine kept \"playing\" a blank frame \
         instead of resting"
    );
}

#[test]
fn ac_29_a_reset_while_paused_arms_no_hold() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    play_live_tone(&engine, &clock);
    let live_shape = engine.current_bands();
    engine.set_playing(false);
    run_display_ticks(&engine, &clock, 120);
    assert!(
        largest_difference(&engine.current_bands(), &live_shape) > 0.05,
        "setup: the paused display should differ from the live shape"
    );

    // A boundary while nothing is playing, then playback resumes before any PCM.
    engine.reset_audio_stream();
    engine.set_playing(true);
    run_display_ticks(&engine, &clock, 5);

    assert!(
        largest_difference(&engine.current_bands(), &live_shape) > 0.05,
        "a reset that arrived while paused snapped the old live shape back on screen"
    );
}

#[test]
fn ac_29_a_swipe_after_a_user_pause_adopts_the_paused_display() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    play_live_tone(&engine, &clock);
    let live_shape = engine.current_bands();

    // The user pauses: Media3's playWhenReady drops with the snapshot state.
    engine.set_playback_intended(false);
    engine.set_playing(false);
    run_display_ticks(&engine, &clock, 120);

    assert!(
        largest_difference(&engine.current_bands(), &live_shape) > 0.05,
        "setup: the paused display should differ from the live shape"
    );
    assert_eq!(
        engine.adoptable_bands(),
        engine.current_bands(),
        "a swipe from a paused song must not pop the minutes-old live shape back on screen"
    );
}
