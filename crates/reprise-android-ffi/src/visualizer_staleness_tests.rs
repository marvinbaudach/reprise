use super::*;

#[test]
fn paused_live_audio_reports_silence_without_forgetting_the_stream() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    engine.set_playing(true);
    let pcm = stereo_sine_pcm16(80.0, 48_000, 0, 8_192);
    ingest_one_live_block(&engine, &clock, &pcm, 48_000);

    engine.set_playing(false);

    assert!(engine.has_live_audio());
    assert_eq!(engine.bass_pressure().kick, 0.0);
    assert_eq!(engine.bass_pressure().pressure, 0.0);
}

#[test]
fn paused_scene_uses_elapsed_time_at_a_fifteen_hertz_redraw_rate() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    engine.set_playing(true);
    let pcm = stereo_sine_pcm16(80.0, 48_000, 0, 8_192);
    assert!(engine.ingest_pcm_i16(pcm.clone(), pcm.len() as u32, 48_000, 2));
    engine.set_playing(false);
    clock.advance(Duration::from_secs(2));
    engine.tick();
    let start = engine.scene(272.0, 272.0);

    // Three 15 Hz frames span exactly 200 ms despite millisecond rounding.
    // Ninety redraws therefore cover the portable wave's six-second period.
    for elapsed in [67, 67, 66].into_iter().cycle().take(90) {
        clock.advance(Duration::from_millis(elapsed));
        engine.tick();
    }
    let after_one_period = engine.scene(272.0, 272.0);

    assert_eq!(start.len(), after_one_period.len());
    let start = decode_float_bytes(&start);
    let after_one_period = decode_float_bytes(&after_one_period);
    let largest_error = start
        .iter()
        .zip(after_one_period)
        .map(|(before, after)| (before - after).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        largest_error < 0.000_1,
        "paused Android scene missed its six-second return by {largest_error}"
    );
}

#[test]
fn paused_stored_analysis_keeps_the_existing_generic_fallback() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let stored = AndroidVisualEngine::with_clock(clock.clone());
    stored.ingest_bands(
        (0..64)
            .map(|index| 0.2 + index as f32 * 0.7 / 63.0)
            .collect(),
    );
    let generic = AndroidVisualEngine::with_clock(clock.clone());
    generic.ingest_bands(Vec::new());

    for _ in 0..60 {
        clock.advance(Duration::from_nanos(16_666_667));
        stored.tick();
        generic.tick();
    }

    let stored_scene = stored.scene(272.0, 272.0);
    let generic_scene = generic.scene(272.0, 272.0);
    assert_eq!(stored_scene.len(), generic_scene.len());
    let stored_scene = decode_float_bytes(&stored_scene);
    let generic_scene = decode_float_bytes(&generic_scene);
    let largest_error = stored_scene
        .iter()
        .zip(&generic_scene)
        .map(|(stored, generic)| (stored - generic).abs())
        .fold(0.0, f32::max);
    assert!(
        largest_error < 0.000_02,
        "stored analysis replaced the generic paused fallback by {largest_error}"
    );
}

#[test]
fn live_pcm_staleness_pauses_while_playback_is_not_intended() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    engine.set_playback_intended(true);
    engine.set_playing(true);
    let pcm = stereo_sine_pcm16(80.0, 48_000, 0, 8_192);
    ingest_one_live_block(&engine, &clock, &pcm, 48_000);

    engine.set_playback_intended(false);
    engine.set_playing(false);
    clock.advance(LIVE_AUDIO_STALE_AFTER + LIVE_AUDIO_STALE_AFTER);
    assert!(engine.has_live_audio());
}

#[test]
fn live_pcm_staleness_expires_while_player_buffers_with_playback_intent() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    engine.set_playback_intended(true);
    engine.set_playing(true);
    let pcm = stereo_sine_pcm16(80.0, 48_000, 0, 8_192);
    ingest_one_live_block(&engine, &clock, &pcm, 48_000);

    // The engine has no Buffering state of its own. This isolates the generic
    // 500 ms expiry while playback intent and visual evolution remain active;
    // Kotlin separately owns the Buffering-to-active projection.
    assert!(engine.bass_pressure().pressure > 0.0);
    clock.advance(LIVE_AUDIO_STALE_AFTER);

    assert!(!engine.has_live_audio());
    assert_eq!(engine.bass_pressure().pressure, 0.0);
}

#[test]
fn live_pcm_staleness_expires_when_playback_intent_was_never_reported() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    engine.set_playing(true);
    let pcm = stereo_sine_pcm16(80.0, 48_000, 0, 8_192);
    assert!(engine.ingest_pcm_i16(pcm.clone(), pcm.len() as u32, 48_000, 2));

    clock.advance(LIVE_AUDIO_STALE_AFTER);

    assert!(!engine.has_live_audio());
}

#[test]
fn resume_history_reset_preserves_live_scene_until_pcm_restarts_or_expires() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    engine.set_playback_intended(true);
    engine.set_playing(true);
    let pcm = stereo_sine_pcm16(80.0, 48_000, 0, 8_192);
    ingest_one_live_block(&engine, &clock, &pcm, 48_000);

    engine.set_playback_intended(false);
    engine.set_playing(false);
    let paused_scene = engine.scene(272.0, 272.0);
    clock.advance(LIVE_AUDIO_STALE_AFTER + LIVE_AUDIO_STALE_AFTER);

    // Media3 publishes playWhenReady before isPlaying on a real resume.
    engine.set_playback_intended(true);
    engine.reset_audio_history();
    engine.set_playing(true);

    assert!(engine.has_live_audio());
    assert_eq!(engine.scene(272.0, 272.0), paused_scene);
    assert!(engine
        .live_bands_for_testing()
        .iter()
        .all(|band| *band == 0.0));

    clock.advance(LIVE_AUDIO_STALE_AFTER);
    assert!(!engine.has_live_audio());
}

#[test]
// Regression test for the swipe bug this fix addresses: `reset_audio_history`
// used to fully reset the CAVA processor (`CavaBarProcessor::reset`), so the
// next analyzed live-PCM block reported near-zero bars for one frame while
// the peak caps stayed at their old height — a visible flash of bare caps
// with no bars underneath. `LiveAudioState::reset` now calls
// `reset_stream()`, which keeps the smoother's shape, so the next block
// falls from the previous bars instead of climbing back up from zero.
fn reset_audio_history_keeps_bars_on_screen_through_the_next_live_block() {
    // Device-sized ticks (~800 samples @ 48 kHz, the `withFrameNanos` cadence
    // documented on `Smoother::apply`'s tests), not one giant block: the CAVA
    // main FFT window is 4_096 samples wide, so only a device tick's worth of
    // real signal lands in it right after a reset, while the rest is still
    // the zero fill the reset left behind. That partial refill is exactly
    // what makes this test depend on the smoother's retained bar shape
    // instead of on the window having already refilled with real audio.
    const DEVICE_TICK_FRAMES: usize = 800;
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    engine.set_playback_intended(true);
    engine.set_playing(true);
    for chunk in 0..200 {
        let pcm = stereo_sine_pcm16(200.0, 48_000, chunk, DEVICE_TICK_FRAMES);
        ingest_one_live_block(&engine, &clock, &pcm, 48_000);
    }
    let before = main_bar_segments(&decode_scene(&engine.scene(272.0, 272.0)), 272.0).len();
    assert!(before > 0, "warm-up should already show bars on screen");

    engine.reset_audio_history();
    let pcm = stereo_sine_pcm16(200.0, 48_000, 200, DEVICE_TICK_FRAMES);
    ingest_one_live_block(&engine, &clock, &pcm, 48_000);

    let after = main_bar_segments(&decode_scene(&engine.scene(272.0, 272.0)), 272.0).len();
    assert!(
        after * 2 >= before,
        "bars dropped to near zero right after reset_audio_history: before={before}, after={after}"
    );
}

#[test]
fn stale_live_pcm_reopens_the_stored_spectrogram_fallback() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    engine.set_playback_intended(true);
    engine.set_playing(true);
    let pcm = stereo_sine_pcm16(80.0, 48_000, 0, 8_192);
    ingest_one_live_block(&engine, &clock, &pcm, 48_000);
    assert!(engine.has_live_audio());

    clock.advance(LIVE_AUDIO_STALE_AFTER);
    assert!(!engine.has_live_audio());

    let stored_bands = vec![0.35; 24];
    engine.ingest_bands(stored_bands.clone());
    let fallback = AndroidVisualEngine::with_clock(clock);
    fallback.set_playing(true);
    fallback.ingest_bands(stored_bands);

    let stale_scene = decode_scene(&engine.scene(272.0, 272.0));
    let fallback_scene = decode_scene(&fallback.scene(272.0, 272.0));
    assert_eq!(
        main_bar_segments(&stale_scene, 272.0),
        main_bar_segments(&fallback_scene, 272.0),
    );
}
