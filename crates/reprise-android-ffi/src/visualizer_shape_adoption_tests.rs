use super::*;

#[test]
fn current_bands_reports_the_engines_displayed_bars() {
    let engine = AndroidVisualEngine::new();
    engine.set_playing(true);
    engine.ingest_bands(vec![0.4; 24]);

    let bands = engine.current_bands();

    assert_eq!(bands.len(), 64);
    assert!(
        bands.iter().all(|band| (band - 0.4).abs() < 1e-4),
        "current_bands should mirror the ingested, interpolated bands: {bands:?}"
    );
}

#[test]
fn adopt_shape_draws_immediately_on_a_fresh_engine() {
    // Production order, not the more convenient adopt-then-everything-else:
    // `rememberVisualSceneEngine` runs `noteTrackChanged()` in a
    // `DisposableEffect` keyed on the freshly created engine before the
    // `adoptShape` effect that follows it, and only then does the
    // `SideEffect` that reports `set_playing` run. `note_track_changed`
    // clears `has_ingested`, so adopting before it would be wiped out; kept
    // in the real order here so this test cannot pass for a reason
    // production never provides.
    let engine = AndroidVisualEngine::new();

    engine.note_track_changed();
    engine.adopt_shape(vec![0.8; 64]);
    engine.set_playing(true);

    let scene = decode_scene(&engine.scene(272.0, 272.0));
    assert!(
        !main_bar_segments(&scene, 272.0).is_empty(),
        "adopt_shape should draw bars before any PCM has arrived"
    );
    let bands = engine.current_bands();
    assert_eq!(bands.len(), 64);
    assert!(
        bands.iter().all(|band| (band - 0.8).abs() < 1e-4),
        "current_bands should match the adopted seed: {bands:?}"
    );
    assert!(
        !engine.has_live_audio(),
        "adopt_shape must not mark the engine as having live audio"
    );
}

#[test]
fn adopt_shape_with_empty_bands_is_a_no_op() {
    let engine = AndroidVisualEngine::new();
    engine.set_playing(true);

    engine.adopt_shape(Vec::new());

    assert!(engine.scene(272.0, 272.0).is_empty());
}

#[test]
fn adopted_shape_holds_until_the_live_stream_speaks() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    engine.note_track_changed();
    engine.adopt_shape(vec![0.8; SPECTRUM_BAND_COUNT]);
    engine.set_playing(true);
    let before = main_bar_segments(&decode_scene(&engine.scene(272.0, 272.0)), 272.0).len();
    assert!(before > 0, "the adopted shape should already show bars");

    for _ in 0..15 {
        clock.advance(Duration::from_millis(16));
        engine.tick();
    }

    let after = main_bar_segments(&decode_scene(&engine.scene(272.0, 272.0)), 272.0).len();
    assert_eq!(
        after, before,
        "the adopted shape decayed before live PCM arrived: before={before}, after={after}"
    );
}

#[test]
fn a_track_change_discards_an_unconsumed_adopted_shape_seed() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    engine.adopt_shape(vec![0.9; SPECTRUM_BAND_COUNT]);
    engine.note_track_changed();
    engine.set_playing(true);

    let silent_pcm = vec![0; 8_192 * 2 * size_of::<i16>()];
    ingest_one_live_block(&engine, &clock, &silent_pcm, 48_000);

    let bands = engine.current_bands();
    assert!(
        bands.iter().all(|band| *band < 0.05),
        "the next track inherited the stale adopted shape seed: {bands:?}"
    );
}

#[test]
// Regression test for the second half of the swipe bug: a panel taking over
// the live slot used to start its brand-new engine from zero. `adopt_shape`
// seeds that engine's smoother memory ahead of the first PCM block (see
// `live_processor_for_stream`'s pending-seed handling), so the transition
// from the adopted shape to the first real live frame does not itself drop
// to zero either.
fn adopt_shape_keeps_bars_on_screen_through_the_first_live_pcm_block() {
    // Same production order as `adopt_shape_draws_immediately_on_a_fresh_engine`.
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    engine.note_track_changed();
    engine.adopt_shape(vec![0.8; 64]);
    engine.set_playing(true);
    let before = main_bar_segments(&decode_scene(&engine.scene(272.0, 272.0)), 272.0).len();
    assert!(before > 0, "the adopted shape should already show bars");

    let pcm = stereo_sine_pcm16(200.0, 48_000, 0, 8_192);
    ingest_one_live_block(&engine, &clock, &pcm, 48_000);

    let after = main_bar_segments(&decode_scene(&engine.scene(272.0, 272.0)), 272.0).len();
    assert!(
        after * 2 >= before,
        "the first live PCM block dropped the adopted shape to near zero: before={before}, after={after}"
    );
}

#[test]
fn ui_reads_do_not_wait_for_live_pcm_processing() {
    let engine = Arc::new(AndroidVisualEngine::new());

    let (read, worker) = engine.with_live_processor_locked_for_testing(|| {
        let (sender, receiver) = mpsc::channel();
        let engine = Arc::clone(&engine);
        let worker = thread::spawn(move || {
            sender
                .send(engine.has_live_audio())
                .expect("test receiver remains alive");
        });
        (receiver.recv_timeout(Duration::from_millis(250)), worker)
    });

    worker.join().expect("UI read worker should finish");
    assert!(!read.expect("UI read waited for the PCM processor"));
}

#[test]
fn pcm_ingest_does_not_contend_on_display_state() {
    let engine = AndroidVisualEngine::new();
    let pcm = stereo_sine_pcm16(80.0, 48_000, 0, 512);

    assert_eq!(engine.dropped_audio_frames(), 0);
    let accepted = engine.with_state_locked_for_testing(|| {
        engine.ingest_pcm_i16(pcm.clone(), pcm.len() as u32, 48_000, 2)
    });

    assert!(accepted);
    assert_eq!(engine.dropped_audio_frames(), 0);
    assert!(engine.ingest_pcm_i16(pcm.clone(), pcm.len() as u32, 48_000, 2));
    assert_eq!(engine.dropped_audio_frames(), 0);
}

#[test]
fn pcm_block_dropped_on_live_audio_contention_is_counted() {
    let engine = AndroidVisualEngine::new();
    let pcm = stereo_sine_pcm16(80.0, 48_000, 0, 512);

    assert_eq!(engine.dropped_audio_frames(), 0);
    let accepted = engine.with_live_processor_locked_for_testing(|| {
        engine.ingest_pcm_i16(pcm.clone(), pcm.len() as u32, 48_000, 2)
    });

    assert!(!accepted);
    assert_eq!(engine.dropped_audio_frames(), 1);
    assert!(engine.ingest_pcm_i16(pcm.clone(), pcm.len() as u32, 48_000, 2));
    assert_eq!(engine.dropped_audio_frames(), 1);
}
