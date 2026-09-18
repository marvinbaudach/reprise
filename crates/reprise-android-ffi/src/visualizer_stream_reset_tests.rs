use super::*;

#[test]
fn scene_before_the_first_ingest_is_empty() {
    let engine = AndroidVisualEngine::new();
    engine.set_playing(true);
    engine.note_track_changed();

    assert!(engine.scene(272.0, 272.0).is_empty());
}

#[test]
fn an_empty_analysis_uses_the_shared_resting_scene_while_playback_runs() {
    let engine = AndroidVisualEngine::new();
    engine.set_playing(true);
    engine.ingest_bands(Vec::new());
    for _ in 0..25 {
        engine.tick();
    }

    let scene = decode_scene(&engine.scene(272.0, 272.0));

    assert!(
        scene
            .shapes
            .iter()
            .any(|shape| matches!(shape.geom, Geom::Rect { .. })),
        "the no-analysis scene should contain the engine's resting bars"
    );
}

#[test]
fn live_pcm_produces_sixty_four_non_interpolated_cava_bands() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    engine.set_playing(true);

    for chunk in 0..20 {
        let pcm = stereo_sine_pcm16(2_000.0, 48_000, chunk, 512);
        assert!(engine.ingest_pcm_i16(pcm.clone(), pcm.len() as u32, 48_000, 2));
        clock.advance(Duration::from_nanos(10_666_667));
        engine.tick();
    }

    let bands = engine.live_bands_for_testing();
    let largest_neighbor_step = bands
        .windows(2)
        .map(|pair| (pair[0] - pair[1]).abs())
        .fold(0.0_f32, f32::max);
    assert_eq!(bands.len(), 64);
    assert!(
        largest_neighbor_step > 1.0 / 23.0,
        "direct CAVA bins should retain detail finer than one 24-band interpolation step: {bands:?}"
    );
    assert!(engine.has_live_audio());
    assert!(!engine.scene(272.0, 272.0).is_empty());
}

#[test]
fn stereo_pcm_is_averaged_to_mono_instead_of_summed() {
    let cancellation_clock = Arc::new(FakeMonotonicClock::default());
    let cancellation = AndroidVisualEngine::with_clock(cancellation_clock.clone());
    cancellation.set_playing(true);
    let opposite_phase = opposite_phase_stereo_pcm16(8_192);

    ingest_one_live_block(&cancellation, &cancellation_clock, &opposite_phase, 48_000);

    assert!(cancellation
        .live_bands_for_testing()
        .iter()
        .all(|band| *band == 0.0));
    assert_eq!(cancellation.bass_pressure().pressure, 0.0);

    let stereo_clock = Arc::new(FakeMonotonicClock::default());
    let stereo = AndroidVisualEngine::with_clock(stereo_clock.clone());
    stereo.set_playing(true);
    let stereo_pcm = stereo_sine_pcm16(2_000.0, 48_000, 0, 8_192);
    ingest_one_live_block(&stereo, &stereo_clock, &stereo_pcm, 48_000);

    let mono_clock = Arc::new(FakeMonotonicClock::default());
    let mono = AndroidVisualEngine::with_clock(mono_clock.clone());
    mono.set_playing(true);
    let mono_pcm = stereo_pcm
        .as_chunks::<{ 2 * size_of::<i16>() }>()
        .0
        .iter()
        .flat_map(|frame| frame[..size_of::<i16>()].iter().copied())
        .collect::<Vec<_>>();
    ingest_one_live_mono_block(&mono, &mono_clock, &mono_pcm, 48_000);

    assert_eq!(
        stereo.live_bands_for_testing(),
        mono.live_bands_for_testing()
    );
}

#[test]
fn stream_and_track_changes_reset_cava_and_bass_history() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    engine.set_playing(true);
    let pcm = stereo_sine_pcm16(80.0, 48_000, 0, 8_192);
    ingest_one_live_block(&engine, &clock, &pcm, 48_000);
    assert!(engine
        .live_bands_for_testing()
        .iter()
        .any(|band| *band > 0.0));

    engine.reset_audio_stream();
    assert!(!engine.has_live_audio());
    assert!(engine
        .live_bands_for_testing()
        .iter()
        .all(|band| *band == 0.0));
    assert_eq!(engine.bass_pressure().pressure, 0.0);

    assert!(engine.ingest_pcm_i16(pcm.clone(), pcm.len() as u32, 48_000, 2));
    engine.note_track_changed();
    assert!(!engine.has_live_audio());
    assert!(engine
        .live_bands_for_testing()
        .iter()
        .all(|band| *band == 0.0));
    assert_eq!(engine.bass_pressure().pressure, 0.0);
}

#[test]
fn stream_generation_reset_starts_idle_fade_and_phase_at_zero_after_a_time_gap() {
    let delayed_clock = Arc::new(FakeMonotonicClock::default());
    let delayed = AndroidVisualEngine::with_clock(delayed_clock.clone());
    delayed.set_playing(true);
    let pcm = stereo_sine_pcm16(80.0, 48_000, 0, 8_192);
    assert!(delayed.ingest_pcm_i16(pcm.clone(), pcm.len() as u32, 48_000, 2));

    delayed_clock.advance(Duration::from_secs(2));
    delayed.reset_audio_stream();
    delayed.tick();

    let immediate_clock = Arc::new(FakeMonotonicClock::default());
    let immediate = AndroidVisualEngine::with_clock(immediate_clock);
    immediate.set_playing(true);
    assert!(immediate.ingest_pcm_i16(pcm.clone(), pcm.len() as u32, 48_000, 2));
    immediate.reset_audio_stream();
    immediate.tick();

    assert_eq!(
        delayed.scene(272.0, 272.0),
        immediate.scene(272.0, 272.0),
        "pre-reset playing time advanced the idle fade or phase"
    );
}

#[test]
// Regression test for the swipe bug this fix addresses: Media3's flush across
// a transport switch (`reset_audio_stream`) used to drop `has_audio` to false
// at once, so the engine decayed toward the idle/paused projection for the
// 200-300 ms gap before the new track's first PCM block arrived -- a visible
// decay-to-caps-only, then a "pop" once real data resumed. The engine now
// holds the displayed bar shape frozen across that gap instead.
fn reset_stream_holds_the_bar_shape_until_the_next_stream_speaks_or_playback_stops() {
    const DEVICE_TICK_FRAMES: usize = 800;
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    engine.set_playback_intended(true);
    engine.set_playing(true);
    for chunk in 0..200 {
        let pcm = stereo_sine_pcm16(200.0, 48_000, chunk, DEVICE_TICK_FRAMES);
        ingest_one_live_block(&engine, &clock, &pcm, 48_000);
    }
    let before = main_bar_segments(&decode_scene(&engine.scene(272.0, 272.0)), 272.0);
    assert!(
        !before.is_empty(),
        "warm-up should already show bars on screen"
    );

    engine.reset_audio_stream();

    // No PCM for the new stream arrives yet -- the decode-and-buffer gap
    // measured on device (200-300 ms) -- but the transport is still playing.
    for _ in 0..15 {
        clock.advance(Duration::from_millis(16));
        engine.tick();
    }
    let held = main_bar_segments(&decode_scene(&engine.scene(272.0, 272.0)), 272.0);
    assert_eq!(
        held, before,
        "the bar shape decayed during the reset gap instead of holding"
    );

    // The new stream speaks: the hold must release, and the bars must follow
    // the new audio -- a clearly different tone, several blocks, since
    // `CavaBarProcessor::reset_stream` deliberately keeps the smoother's
    // shape, so a single block from the new stream alone would fall from
    // (and look close to) the held picture rather than proving it moved.
    for chunk in 200..215 {
        let pcm = stereo_sine_pcm16(900.0, 48_000, chunk, DEVICE_TICK_FRAMES);
        ingest_one_live_block(&engine, &clock, &pcm, 48_000);
    }
    let after_pcm = main_bar_segments(&decode_scene(&engine.scene(272.0, 272.0)), 272.0);
    assert_ne!(
        after_pcm, before,
        "the first blocks of the new stream must move the held picture"
    );

    // Bound: the hold must not last forever. If playback actually stops
    // before the new stream ever speaks, the normal paused/idle projection
    // must take back over.
    engine.set_playing(false);
    for _ in 0..120 {
        clock.advance(Duration::from_millis(16));
        engine.tick();
    }
    let after_stop = main_bar_segments(&decode_scene(&engine.scene(272.0, 272.0)), 272.0);
    assert_ne!(
        after_stop, before,
        "a real stop must still release the held picture"
    );
}

#[test]
// Regression test for a bug found while building the test above:
// `reset_live_presentation` is shared by genuine stream-generation
// boundaries (`reset_audio_stream`, `note_track_changed`) and by ordinary
// live-audio staleness expiry (`expire_stale_live_audio`), which calls it
// with the same generation whenever PCM simply stops arriving mid-track.
// Holding the display unconditionally there would also freeze the picture
// forever the next time playback merely stalls, long after any reset. The
// `holds_display` parameter distinguishes the two: only a genuine
// generation change holds; ordinary staleness keeps its existing fallback.
fn ordinary_staleness_after_a_reset_still_falls_back_once_the_new_stream_has_spoken() {
    const DEVICE_TICK_FRAMES: usize = 800;
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    engine.set_playback_intended(true);
    engine.set_playing(true);

    engine.reset_audio_stream();
    for chunk in 0..40 {
        let pcm = stereo_sine_pcm16(300.0, 48_000, chunk, DEVICE_TICK_FRAMES);
        ingest_one_live_block(&engine, &clock, &pcm, 48_000);
    }
    assert!(
        engine.has_live_audio(),
        "warm-up should establish live audio"
    );
    let spoken = main_bar_segments(&decode_scene(&engine.scene(272.0, 272.0)), 272.0);

    // No more PCM arrives -- an ordinary mid-track buffering stall, not
    // another stream boundary. Wait out both the ring buffer's own
    // ~250 ms smoothing margin (so it stops trickling out already-buffered
    // samples) and the 500 ms staleness window, bounded so the test cannot
    // hang if staleness stops expiring.
    let mut still_live = true;
    for _ in 0..400 {
        clock.advance(Duration::from_millis(16));
        engine.tick();
        still_live = engine.has_live_audio();
        if !still_live {
            break;
        }
    }
    assert!(
        !still_live,
        "live audio never went stale within the wait bound"
    );

    // Ordinary staleness must still fall back to the paused/idle projection,
    // not stay frozen under a hold meant for the reset that happened long ago.
    for _ in 0..120 {
        clock.advance(Duration::from_millis(16));
        engine.tick();
    }
    let after_staleness = main_bar_segments(&decode_scene(&engine.scene(272.0, 272.0)), 272.0);
    assert_ne!(
        after_staleness, spoken,
        "ordinary staleness must still decay, not stay frozen because of an earlier reset"
    );
}
