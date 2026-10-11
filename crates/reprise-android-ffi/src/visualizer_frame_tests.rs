use super::*;

const SAMPLE_RATE_HZ: u32 = 48_000;
const DISPLAY_FRAME: Duration = Duration::from_nanos(16_666_667);
const SCENE_SIDE: f32 = 272.0;

fn engine_with_clock() -> (AndroidVisualEngine, Arc<FakeMonotonicClock>) {
    let clock = Arc::new(FakeMonotonicClock::default());
    (AndroidVisualEngine::with_clock(clock.clone()), clock)
}

fn feed_pcm(engine: &AndroidVisualEngine, chunk: usize) {
    let pcm = stereo_sine_pcm16(80.0, SAMPLE_RATE_HZ, chunk, 2_048);
    assert!(engine.ingest_pcm_i16(pcm.clone(), pcm.len() as u32, SAMPLE_RATE_HZ, 2));
}

/// What Kotlin used to ask for with three calls: the tick, then the two queries.
fn tick_then_queries(engine: &AndroidVisualEngine) -> (bool, bool, String) {
    let advanced = engine.tick();
    (
        advanced,
        engine.has_live_audio(),
        format!("{:?}", engine.bass_pressure()),
    )
}

fn frame_fields(engine: &AndroidVisualEngine) -> (bool, bool, String) {
    let frame = engine.frame();
    (
        frame.advanced,
        frame.has_live_audio,
        format!("{:?}", frame.bass_pressure),
    )
}

#[test]
fn a_frame_answers_what_the_tick_and_both_queries_answered_separately() {
    let (separate, separate_clock) = engine_with_clock();
    let (single, single_clock) = engine_with_clock();
    separate.set_playing(true);
    single.set_playing(true);

    // Live PCM arriving, then a pause in the stream longer than the stale
    // window, so the frame has to report the expiry exactly as the queries did.
    for step in 0..120_usize {
        if step < 60 {
            feed_pcm(&separate, step);
            feed_pcm(&single, step);
        }
        separate_clock.advance(DISPLAY_FRAME);
        single_clock.advance(DISPLAY_FRAME);
        assert_eq!(
            tick_then_queries(&separate),
            frame_fields(&single),
            "step {step}"
        );
    }
}

#[test]
fn a_frame_reports_live_audio_and_a_pressure_once_pcm_has_been_analysed() {
    let (engine, clock) = engine_with_clock();
    engine.set_playing(true);
    feed_pcm(&engine, 0);
    clock.advance(Duration::from_millis(43));

    let frame = engine.frame();

    assert!(frame.advanced);
    assert!(frame.has_live_audio);
    assert_eq!(
        format!("{:?}", frame.bass_pressure),
        format!("{:?}", engine.bass_pressure())
    );
}

#[test]
fn a_frame_without_audio_reports_silence_and_no_live_audio() {
    let (engine, clock) = engine_with_clock();
    engine.set_playing(true);
    clock.advance(DISPLAY_FRAME);

    let frame = engine.frame();

    assert!(!frame.has_live_audio);
    assert_eq!(frame.bass_pressure.kick, 0.0);
    assert_eq!(frame.bass_pressure.pressure, 0.0);
}

#[test]
fn a_frame_after_a_stream_reset_forgets_the_old_live_audio() {
    let (engine, clock) = engine_with_clock();
    engine.set_playing(true);
    feed_pcm(&engine, 0);
    clock.advance(Duration::from_millis(43));
    assert!(engine.frame().has_live_audio);

    engine.reset_audio_stream();
    clock.advance(DISPLAY_FRAME);
    let frame = engine.frame();

    assert_eq!(frame.has_live_audio, engine.has_live_audio());
    assert!(!frame.has_live_audio);
}

/// Replays one script of FFI calls into an engine, with the setters either
/// repeated before every tick (as Kotlin did) or sent only on a change.
fn replay_script(repeat_setters: bool) -> Vec<(bool, Vec<u8>)> {
    let (engine, clock) = engine_with_clock();
    let mut playing = None;
    let mut accent = None;
    let mut observed = Vec::new();
    for step in 0..180_usize {
        let wanted_playing = !(70..100).contains(&step);
        let wanted_accent = if step < 120 {
            (0.9, 0.2, 0.1)
        } else {
            (0.1, 0.4, 0.9)
        };
        if repeat_setters || playing != Some(wanted_playing) {
            engine.set_playing(wanted_playing);
            playing = Some(wanted_playing);
        }
        if repeat_setters || accent != Some(wanted_accent) {
            engine.set_accent(wanted_accent.0, wanted_accent.1, wanted_accent.2);
            accent = Some(wanted_accent);
        }
        if step == 130 {
            engine.reset_audio_stream();
            playing = None;
        }
        if step == 150 {
            engine.note_track_changed();
            playing = None;
        }
        if step < 60 || (110..170).contains(&step) {
            feed_pcm(&engine, step);
        }
        clock.advance(DISPLAY_FRAME);
        let advanced = engine.tick();
        observed.push((advanced, engine.scene(SCENE_SIDE, SCENE_SIDE)));
    }
    observed
}

#[test]
fn sending_a_setter_only_when_its_value_changed_leaves_every_frame_identical() {
    let repeated = replay_script(true);
    let on_change = replay_script(false);

    assert_eq!(repeated.len(), on_change.len());
    assert!(
        repeated.iter().any(|(_, scene)| !scene.is_empty()),
        "the script never drew a scene, so it proves nothing"
    );
    for (step, (every, changed)) in repeated.iter().zip(&on_change).enumerate() {
        assert_eq!(every, changed, "step {step}");
    }
}
