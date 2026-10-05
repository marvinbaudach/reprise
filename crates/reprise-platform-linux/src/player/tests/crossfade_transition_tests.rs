use super::*;
use reprise_core::library::settings::TrackTransition;

/// Crossfade Phase B backend proof (headless, fakesink): with `Crossfade`
/// selected, the position ticker must spin up a second playbin for the
/// pre-fed successor and promote it without stopping the primary pipeline.
#[test]
fn play_20b_crossfade_promotion_carries_the_next_gain_from_the_first_sample() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::env::set_var(AUDIO_SINK_ENV_VAR, "fakesink");

    let (tx, rx) = std::sync::mpsc::channel::<PlayerEvent>();
    let player = Player::new(Box::new(move |event| {
        let _ = tx.send(event);
    }))
    .unwrap();

    player.set_transition(TrackTransition::Crossfade, 1);

    let first = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sine.flac");
    let second = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/blip.flac");
    player.play(item(first)).unwrap();
    player.set_next(Some(PlaybackItem {
        path: second,
        gain_db: 6.0,
    }));

    let main_context = gst::glib::MainContext::default();
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    let mut advanced = 0usize;
    let mut finished = 0usize;
    let mut stopped = 0usize;
    let mut saw_crossfading = false;
    while std::time::Instant::now() < deadline {
        main_context.iteration(false);
        if player.crossfading.load(Ordering::SeqCst) {
            saw_crossfading = true;
        }
        while let Ok(event) = rx.try_recv() {
            match event {
                PlayerEvent::AdvancedToNext => advanced += 1,
                PlayerEvent::TrackFinished => finished += 1,
                PlayerEvent::StateChanged(PlaybackState::Stopped) => stopped += 1,
                _ => {}
            }
        }
        if advanced > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    main_context.iteration(false);
    while let Ok(event) = rx.try_recv() {
        match event {
            PlayerEvent::AdvancedToNext => advanced += 1,
            PlayerEvent::TrackFinished => finished += 1,
            PlayerEvent::StateChanged(PlaybackState::Stopped) => stopped += 1,
            _ => {}
        }
    }

    assert!(saw_crossfading, "expected the second pipeline to start");
    assert_eq!(
        advanced, 1,
        "expected exactly one AdvancedToNext from the crossfade promotion"
    );
    assert_eq!(stopped, 0, "the primary pipeline must not stop mid-fade");
    assert_eq!(
        finished, 0,
        "the outgoing EOS must not surface as TrackFinished"
    );

    let playbin = player
        .playbin
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let current_uri = playbin.property::<Option<String>>("current-uri");
    assert!(
        current_uri
            .as_deref()
            .is_some_and(|uri| uri.ends_with("blip.flac")),
        "the promoted pipeline should contain the second track"
    );
    let track_gain = playbin
        .property::<Option<gst::Element>>("audio-filter")
        .unwrap()
        .downcast::<gst::Bin>()
        .unwrap()
        .by_name("reprise-track-gain")
        .unwrap()
        .property::<f64>("volume");
    assert!((track_gain - 10_f64.powf(6.0 / 20.0)).abs() < 1e-6);
    drop(playbin);

    assert!(
        !player.crossfading.load(Ordering::SeqCst),
        "the crossfade guard must be cleared after promotion"
    );
    assert!(
        player
            .incoming
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_none(),
        "the incoming-pipeline slot must be empty after promotion"
    );

    player.stop().unwrap();
    std::env::remove_var(AUDIO_SINK_ENV_VAR);
}
