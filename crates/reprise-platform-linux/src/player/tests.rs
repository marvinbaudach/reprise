use super::*;
use crate::player_effects::{
    build_audio_filter, requested_state, same_filter_topology, set_spectrum_messages,
};
use reprise_core::library::settings::TrackTransition;

#[test]
fn path_to_uri_encodes_special_chars() {
    let uri = path_to_uri("/home/marvin/Music/Björk/Jóga (Live).flac").unwrap();
    assert!(uri.starts_with("file:///home/marvin/Music/"));
    assert!(uri.contains("J%C3%B3ga%20(Live).flac"));
    assert!(path_to_uri("relativ/pfad.mp3").is_err());
}

#[test]
fn play_uri_accepts_only_http_https_and_file_schemes() {
    assert_eq!(
        validated_playback_uri("https://radio.example/live").unwrap(),
        "https://radio.example/live"
    );
    assert_eq!(
        validated_playback_uri("file:///tmp/episode.mp3").unwrap(),
        "file:///tmp/episode.mp3"
    );
    assert!(validated_playback_uri("ftp://example.test/audio").is_err());
    assert!(validated_playback_uri("relative/audio.mp3").is_err());
}

#[test]
fn stream_tags_merge_partial_updates_and_suppress_duplicates() {
    let empty = (None, None);
    let title = merge_stream_tags(&empty, Some("Current song".into()), None)
        .expect("the first title changes stream metadata");
    assert_eq!(title, (Some("Current song".into()), None));

    let complete = merge_stream_tags(&title, None, Some("Example Radio".into()))
        .expect("organization augments the existing title");
    assert_eq!(
        complete,
        (Some("Current song".into()), Some("Example Radio".into()))
    );
    assert_eq!(merge_stream_tags(&complete, None, None), None);
    assert_eq!(
        merge_stream_tags(&complete, Some("Current song".into()), None),
        None
    );
}

#[test]
fn audio_filter_contains_configured_equalizer_and_replaygain() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    gst::init().unwrap();
    let effects = AudioEffects {
        equalizer_enabled: true,
        equalizer_bands: [3.0; 10],
        replay_gain: reprise_core::library::settings::ReplayGainMode::Album,
    };
    let filter = build_audio_filter(&effects).unwrap().unwrap();
    let bin = filter.downcast::<gst::Bin>().unwrap();
    assert!(bin.by_name("reprise-equalizer").is_some());
    let replaygain = bin.by_name("reprise-replaygain").unwrap();
    assert!(replaygain.property::<bool>("album-mode"));
}

#[test]
fn enabling_equalizer_keeps_filter_topology_stable() {
    let disabled = AudioEffects::default();
    let enabled = AudioEffects {
        equalizer_enabled: true,
        ..AudioEffects::default()
    };

    assert!(same_filter_topology(&disabled, &enabled));
}

#[test]
fn disabled_equalizer_is_neutral_in_the_stable_filter() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    gst::init().unwrap();
    let effects = AudioEffects {
        equalizer_bands: [8.0; 10],
        ..AudioEffects::default()
    };
    let filter = build_audio_filter(&effects).unwrap().unwrap();
    let equalizer = filter
        .downcast::<gst::Bin>()
        .unwrap()
        .by_name("reprise-equalizer")
        .unwrap();

    assert_eq!(equalizer.property::<f64>("band0"), 0.0);
}

#[test]
fn ac_20_audio_filter_contains_a_disabled_bounded_spectrum_analyzer() {
    gst::init().unwrap();
    let filter = build_audio_filter(&AudioEffects::default())
        .unwrap()
        .unwrap();
    let bin = filter.clone().downcast::<gst::Bin>().unwrap();
    let spectrum = bin
        .by_name("reprise-spectrum")
        .expect("the stable filter contains the visual analyzer");

    assert_eq!(
        spectrum.property::<u32>("bands"),
        reprise_core::playback::SPECTRUM_ANALYSIS_BAND_COUNT as u32
    );
    assert_eq!(spectrum.property::<i32>("threshold"), -80);
    assert!(!spectrum.property::<bool>("post-messages"));

    set_spectrum_messages(&filter, true).unwrap();
    assert!(spectrum.property::<bool>("post-messages"));
}

#[test]
fn ac_20_spectrum_messages_project_exactly_one_bounded_frame() {
    gst::init().unwrap();
    let magnitudes = gst::List::new(
        (0..reprise_core::playback::SPECTRUM_ANALYSIS_BAND_COUNT)
            .map(|index| -80.0_f32 + (index % 80) as f32),
    );
    let structure = gst::Structure::builder("spectrum")
        .field("magnitude", magnitudes)
        .build();
    let decibels = spectrum_decibels_from_structure(&structure).expect("valid spectrum frame");
    let frame = reprise_core::playback::SpectrumAnalyzer::new().ingest(decibels);
    assert_eq!(
        frame.bands().len(),
        reprise_core::playback::SPECTRUM_BAND_COUNT
    );
    assert!(frame
        .bands()
        .iter()
        .all(|b| b.is_finite() && (0.0..=1.0).contains(b)));
    assert!(spectrum_decibels_from_structure(&gst::Structure::new_empty("other")).is_none());
}

/// Guards every test in this module that sets `AUDIO_SINK_ENV_VAR`:
/// `std::env::set_var`/`remove_var` affect the whole process, and
/// `cargo test` runs tests in this module concurrently by default. Two
/// such tests running at once can interleave — one test's `remove_var`
/// landing between the other's `set_var` and `build_playbin`'s env
/// read — so that `build_playbin` sees no override, builds a *real*
/// audio sink, and plays `sine.flac` audibly on the developer's desktop
/// (or simply fails to find `fakesink`'s paced-sync behavior headless,
/// flaking the test). Each test that touches this env var must acquire
/// this lock for its *entire* duration, from the `set_var` through the
/// matching `remove_var`, so no two such tests ever overlap.
///
/// Poisoned-recovery, not `.unwrap()`: if an earlier test in this lock
/// panicked while holding it, the lock is poisoned but the environment
/// variable was still cleaned up correctly enough for the next test to
/// proceed — refusing to run every subsequent audio-sink test over one
/// unrelated panic would be worse than the poisoning itself.
static AUDIO_SINK_TEST_LOCK: Mutex<()> = Mutex::new(());

/// End-to-end proof that the callback plumbing actually reaches the UI
/// layer: `play()` must emit `StateChanged(Playing)` and `stop()` must
/// emit `StateChanged(Stopped)`. Runs headless via `REPRISE_AUDIO_SINK`
/// (fakesink), which GStreamer supports without a real audio device.
/// This and `play_recovers_after_a_failed_attempt` are the only tests in
/// the crate that touch process environment; both hold
/// `AUDIO_SINK_TEST_LOCK` for their full duration to prevent the
/// cross-test race documented on that lock.
#[test]
fn play_and_stop_emit_state_changed_events() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::env::set_var(AUDIO_SINK_ENV_VAR, "fakesink");

    let (tx, rx) = std::sync::mpsc::channel::<PlayerEvent>();
    let player = Player::new(Box::new(move |event| {
        let _ = tx.send(event);
    }))
    .unwrap();

    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sine.flac");
    player.play(path).unwrap();

    let playing_timeout = Duration::from_secs(5);
    let event = rx
        .recv_timeout(playing_timeout)
        .expect("expected a StateChanged(Playing) event within timeout");
    assert!(matches!(
        event,
        PlayerEvent::StateChanged(PlaybackState::Playing)
    ));

    player.stop().unwrap();
    let event = rx
        .recv_timeout(playing_timeout)
        .expect("expected a StateChanged(Stopped) event within timeout");
    assert!(matches!(
        event,
        PlayerEvent::StateChanged(PlaybackState::Stopped)
    ));

    std::env::remove_var(AUDIO_SINK_ENV_VAR);
}

#[test]
fn ac_20_enabled_player_emits_live_spectrum_frames() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::env::set_var(AUDIO_SINK_ENV_VAR, "fakesink");

    let (tx, rx) = std::sync::mpsc::channel::<PlayerEvent>();
    let player = Player::new(Box::new(move |event| {
        let _ = tx.send(event);
    }))
    .unwrap();
    player.set_spectrum_enabled(true).unwrap();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sine.flac");
    player.play(path).unwrap();

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let frame = 'wait: loop {
        while gst::glib::MainContext::default().pending() {
            gst::glib::MainContext::default().iteration(false);
        }
        while let Ok(event) = rx.try_recv() {
            if let PlayerEvent::Spectrum(frame) = event {
                break 'wait frame;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "expected a spectrum frame within timeout"
        );
        std::thread::sleep(Duration::from_millis(5));
    };

    assert!(frame
        .bands()
        .iter()
        .all(|value| (0.0..=1.0).contains(value)));
    assert!(frame.bands().iter().any(|value| *value > 0.0));
    player.stop().unwrap();
    std::env::remove_var(AUDIO_SINK_ENV_VAR);
}

#[test]
fn enabling_equalizer_does_not_replace_or_rewind_pipeline() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::env::set_var(AUDIO_SINK_ENV_VAR, "fakesink");
    let player = Player::new(Box::new(|_| {})).unwrap();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sine.flac");
    player.play(path).unwrap();
    {
        let playbin = player
            .playbin
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = playbin.state(gst::ClockTime::from_seconds(5));
    }
    player.seek_to(500).unwrap();
    std::thread::sleep(Duration::from_millis(50));

    let (filter_before, position_before) = {
        let playbin = player
            .playbin
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (
            playbin.property::<Option<gst::Element>>("audio-filter"),
            playbin
                .query_position::<gst::ClockTime>()
                .unwrap()
                .mseconds(),
        )
    };
    let effects = AudioEffects {
        equalizer_enabled: true,
        equalizer_bands: [4.0; 10],
        ..AudioEffects::default()
    };
    player.set_audio_effects(effects).unwrap();

    let playbin = player
        .playbin
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let filter_after = playbin.property::<Option<gst::Element>>("audio-filter");
    let position_after = playbin
        .query_position::<gst::ClockTime>()
        .unwrap()
        .mseconds();
    assert!(position_before >= 400);
    assert_eq!(filter_after, filter_before);
    assert_eq!(requested_state(&playbin), gst::State::Playing);
    assert!(position_after.saturating_add(50) >= position_before);
    drop(playbin);
    player.stop().unwrap();
    std::env::remove_var(AUDIO_SINK_ENV_VAR);
}

#[test]
fn live_audio_effect_change_preserves_a_playable_pipeline() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::env::set_var(AUDIO_SINK_ENV_VAR, "fakesink");
    let player = Player::new(Box::new(|_| {})).unwrap();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sine.flac");
    player.play(path).unwrap();
    let effects = AudioEffects {
        equalizer_enabled: true,
        equalizer_bands: [2.0; 10],
        replay_gain: reprise_core::library::settings::ReplayGainMode::Track,
    };
    player.set_audio_effects(effects.clone()).unwrap();
    assert_eq!(
        *player
            .effects
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
        effects
    );

    let filter_before = player
        .playbin
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .property::<Option<gst::Element>>("audio-filter")
        .unwrap();
    let adjusted = AudioEffects {
        equalizer_enabled: true,
        equalizer_bands: [5.0; 10],
        replay_gain: reprise_core::library::settings::ReplayGainMode::Album,
    };
    player.set_audio_effects(adjusted).unwrap();
    let playbin = player
        .playbin
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let filter_after = playbin
        .property::<Option<gst::Element>>("audio-filter")
        .unwrap();
    assert_eq!(filter_after, filter_before);
    assert_eq!(requested_state(&playbin), gst::State::Playing);
    let bin = filter_after.downcast::<gst::Bin>().unwrap();
    assert_eq!(
        bin.by_name("reprise-equalizer")
            .unwrap()
            .property::<f64>("band0"),
        5.0
    );
    assert!(bin
        .by_name("reprise-replaygain")
        .unwrap()
        .property::<bool>("album-mode"));
    drop(playbin);
    player.stop().unwrap();
    std::env::remove_var(AUDIO_SINK_ENV_VAR);
}

#[test]
fn failed_filter_replacement_restores_requested_playback_state() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::env::set_var(AUDIO_SINK_ENV_VAR, "fakesink");
    let player = Player::new(Box::new(|_| {})).unwrap();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sine.flac");
    player.play(path).unwrap();
    let playbin = player
        .playbin
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let result = replace_audio_filter(&playbin, &AudioEffects::default(), |_, _| {
        Err(PlaybackError::Backend("injected filter failure".into()))
    });

    assert!(result.is_err());
    assert_eq!(requested_state(&playbin), gst::State::Playing);
    drop(playbin);
    player.stop().unwrap();
    std::env::remove_var(AUDIO_SINK_ENV_VAR);
}

/// Stage 2 Task 5 regression test for the wedged-pipeline recovery (see
/// `Player::play`'s doc comment): a failed `play()` against a
/// nonexistent file must not take down subsequent, valid `play()` calls
/// on the same `Player` instance. Holds `AUDIO_SINK_TEST_LOCK` for its
/// full duration — see that lock's doc comment for why.
#[test]
fn play_recovers_after_a_failed_attempt() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::env::set_var(AUDIO_SINK_ENV_VAR, "fakesink");

    let (tx, rx) = std::sync::mpsc::channel::<PlayerEvent>();
    let player = Player::new(Box::new(move |event| {
        let _ = tx.send(event);
    }))
    .unwrap();

    let missing_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/does-not-exist.flac"
    );
    assert!(
        player.play(missing_path).is_err(),
        "playing a nonexistent file must fail, not panic"
    );

    let valid_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sine.flac");
    assert!(
        player.play(valid_path).is_ok(),
        "a valid file must still play successfully after a prior failure \
             on the same Player — this is the wedged-pipeline recovery this \
             test guards against regressing"
    );

    let playing_timeout = Duration::from_secs(5);
    let event = rx
        .recv_timeout(playing_timeout)
        .expect("expected a StateChanged(Playing) event within timeout");
    assert!(matches!(
        event,
        PlayerEvent::StateChanged(PlaybackState::Playing)
    ));

    std::env::remove_var(AUDIO_SINK_ENV_VAR);
}

/// Portability seam (refactor Task 5): drives play/stop through
/// `Box<dyn PlaybackBackend>` — the exact shape the controller holds — to
/// pin that the trait surface alone is enough to operate the backend.
#[test]
fn playback_backend_trait_object_drives_play_and_stop() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::env::set_var(AUDIO_SINK_ENV_VAR, "fakesink");

    let (tx, rx) = std::sync::mpsc::channel::<PlayerEvent>();
    let player = Player::new(Box::new(move |event| {
        let _ = tx.send(event);
    }))
    .unwrap();

    let backend: Box<dyn PlaybackBackend> = Box::new(player);

    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sine.flac");
    backend.play(path).unwrap();

    let playing_timeout = Duration::from_secs(5);
    let event = rx
        .recv_timeout(playing_timeout)
        .expect("expected a StateChanged(Playing) event within timeout");
    assert!(matches!(
        event,
        PlayerEvent::StateChanged(PlaybackState::Playing)
    ));

    backend.stop().unwrap();
    let event = rx
        .recv_timeout(playing_timeout)
        .expect("expected a StateChanged(Stopped) event within timeout");
    assert!(matches!(
        event,
        PlayerEvent::StateChanged(PlaybackState::Stopped)
    ));

    std::env::remove_var(AUDIO_SINK_ENV_VAR);
}

/// Gapless Phase A backend proof (headless, fakesink): a pre-fed next
/// track must take over via `about-to-finish` WITHOUT a pipeline restart.
/// Plays a short first track, `set_next`s a distinct second track, then
/// pumps the GLib main context (which dispatches the bus watch) until the
/// first track's end resolves one way or the other. Asserts:
///   (a) the `about-to-finish` handler consumed the URI (slot left empty),
///   (b) the handoff was seamless — exactly one `AdvancedToNext` and zero
///       `TrackFinished` at the transition (a non-gapless advance would EOS
///       the first track into `TrackFinished` instead), and the playbin
///       never left `Playing`,
///   (c) `AdvancedToNext` was emitted and the second track is loaded.
///
/// Holds `AUDIO_SINK_TEST_LOCK` for its full duration — see that lock.
#[test]
fn gapless_handoff_advances_without_pipeline_restart() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::env::set_var(AUDIO_SINK_ENV_VAR, "fakesink");

    let (tx, rx) = std::sync::mpsc::channel::<PlayerEvent>();
    let player = Player::new(Box::new(move |event| {
        let _ = tx.send(event);
    }))
    .unwrap();

    let first = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sine.flac");
    let second = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/blip.flac");
    player.play(first).unwrap();
    player.set_next(Some(second));

    // The bus watch (source of AdvancedToNext / TrackFinished) is dispatched
    // by the GLib main context; nothing iterates it in a headless test, so
    // pump it here until the first track's end resolves one way or another.
    let main_context = gst::glib::MainContext::default();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut advanced = 0usize;
    let mut finished = 0usize;
    while std::time::Instant::now() < deadline {
        main_context.iteration(false);
        while let Ok(event) = rx.try_recv() {
            match event {
                PlayerEvent::AdvancedToNext => advanced += 1,
                PlayerEvent::TrackFinished => finished += 1,
                _ => {}
            }
        }
        if advanced > 0 || finished > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    // (c) + (b): the transition was the gapless handoff, not an EOS advance.
    assert_eq!(
        advanced, 1,
        "expected exactly one AdvancedToNext from the gapless handoff, got {advanced}"
    );
    assert_eq!(
        finished, 0,
        "a seamless handoff must not EOS the first track (no TrackFinished)"
    );

    // (a): the about-to-finish handler took() the pre-fed URI.
    assert!(
        player
            .next_uri
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_none(),
        "about-to-finish must consume the queued URI, leaving the slot empty"
    );

    // (b) + (c): the second track is loaded and the pipeline stayed live
    // (never dropped to Null/Stopped) across the transition.
    let playbin = player
        .playbin
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let current_uri = playbin.property::<Option<String>>("current-uri");
    assert!(
        current_uri
            .as_deref()
            .is_some_and(|uri| uri.ends_with("blip.flac")),
        "playbin should be playing the handed-off second track, got {current_uri:?}"
    );
    assert_eq!(requested_state(&playbin), gst::State::Playing);
    drop(playbin);

    player.stop().unwrap();
    std::env::remove_var(AUDIO_SINK_ENV_VAR);
}

/// Crossfade Phase B backend proof (headless, fakesink): with `Crossfade`
/// selected, the position ticker must, in the last `crossfade_seconds` of the
/// current track, spin up a *second* playbin for the pre-fed successor, ramp
/// the two inversely, and promote the successor to the primary pipeline —
/// emitting exactly one `AdvancedToNext`, just like the gapless handoff, and
/// WITHOUT ever dropping the primary to Null/Stopped mid-fade.
///
/// A 1-second fade over the ~1.16 s `sine.flac` keeps the run fast. Asserts:
///   (a) a second pipeline was started — the `crossfading` guard was observed
///       set (the crossfade trigger fired),
///   (b) exactly one `AdvancedToNext` and no `StateChanged(Stopped)` across the
///       transition,
///   (c) the outgoing pipeline's natural EOS mid-fade did not surface as a
///       `TrackFinished` (the promotion is the authoritative advance),
///   (d) after promotion the primary pipeline is playing the handed-off
///       `blip.flac` (`current-uri`).
///
/// What this does NOT prove: the *audible* equal-power blend itself — that the
/// two streams overlap and their gains actually cross — is not observable
/// headless with `fakesink` and is left to a manual listening test. The
/// deterministic gain math is covered by `crossfade::tests`.
///
/// Holds `AUDIO_SINK_TEST_LOCK` for its full duration — see that lock.
#[test]
fn crossfade_promotes_second_pipeline_and_advances_once() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::env::set_var(AUDIO_SINK_ENV_VAR, "fakesink");

    let (tx, rx) = std::sync::mpsc::channel::<PlayerEvent>();
    let player = Player::new(Box::new(move |event| {
        let _ = tx.send(event);
    }))
    .unwrap();

    // Short fade (1 s) so the test finishes quickly.
    player.set_transition(TrackTransition::Crossfade, 1);

    let first = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sine.flac");
    let second = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/blip.flac");
    player.play(first).unwrap();
    player.set_next(Some(second));

    // The bus watch (source of TrackFinished / spurious Stopped) is dispatched
    // by the GLib main context; pump it while we wait for the crossfade to
    // promote. `AdvancedToNext` is emitted directly from the ramp thread, so it
    // arrives on the channel without the pump — but we still pump so any
    // (suppressed) EOS is actually delivered and we would notice a leak.
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
    // Drain any events that landed right after the promotion (e.g. an EOS the
    // very short blip.flac posts once it too ends), so late arrivals are still
    // counted before we assert.
    main_context.iteration(false);
    while let Ok(event) = rx.try_recv() {
        match event {
            PlayerEvent::AdvancedToNext => advanced += 1,
            PlayerEvent::TrackFinished => finished += 1,
            PlayerEvent::StateChanged(PlaybackState::Stopped) => stopped += 1,
            _ => {}
        }
    }

    // (a): the crossfade trigger fired — a second pipeline was spun up.
    assert!(
        saw_crossfading,
        "expected the crossfading guard to be observed set (second pipeline started)"
    );
    // (b): exactly one advance, and the outgoing pipeline never reported Stopped.
    assert_eq!(
        advanced, 1,
        "expected exactly one AdvancedToNext from the crossfade promotion, got {advanced}"
    );
    assert_eq!(
        stopped, 0,
        "the primary pipeline must not drop to Stopped during a crossfade"
    );
    // (c): the outgoing track's mid-fade EOS must not surface as TrackFinished.
    assert_eq!(
        finished, 0,
        "a crossfade must not EOS the outgoing track into a spurious TrackFinished"
    );

    // (d): the promoted primary is playing the handed-off second track.
    let playbin = player
        .playbin
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let current_uri = playbin.property::<Option<String>>("current-uri");
    assert!(
        current_uri
            .as_deref()
            .is_some_and(|uri| uri.ends_with("blip.flac")),
        "after the crossfade the primary should be the promoted second track, got {current_uri:?}"
    );
    drop(playbin);

    // The crossfade slot is cleared after promotion.
    assert!(
        !player.crossfading.load(Ordering::SeqCst),
        "crossfading guard must be cleared once the fade completes"
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
