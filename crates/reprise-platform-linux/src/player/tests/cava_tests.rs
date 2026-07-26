use super::*;
use crate::player_effects::{build_audio_filter, set_spectrum_messages};
use gstreamer_app as gst_app;

#[test]
fn ac_21_audio_filter_exposes_normalized_mono_pcm_to_cava() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    gst::init().unwrap();
    let filter = build_audio_filter(&AudioEffects::default())
        .unwrap()
        .unwrap();
    let bin = filter.clone().downcast::<gst::Bin>().unwrap();
    let sink = bin
        .by_name("reprise-cava-sink")
        .expect("the stable filter contains the CAVA PCM branch")
        .downcast::<gst_app::AppSink>()
        .expect("the CAVA branch ends in an AppSink");
    let caps = sink.caps().expect("the CAVA sink fixes its PCM contract");
    let structure = caps.structure(0).unwrap();

    assert_eq!(structure.get::<&str>("format").unwrap(), "F32LE");
    assert_eq!(structure.get::<i32>("channels").unwrap(), 1);
    assert_eq!(structure.get::<i32>("rate").unwrap(), 44_100);
    assert!(
        sink.property::<bool>("sync"),
        "visual PCM must follow the playback clock"
    );
    assert!(bin.by_name("reprise-spectrum").is_none());
    set_spectrum_messages(&filter, true).unwrap();
}

#[test]
fn ac_21_cava_pcm_branch_splits_before_replay_gain_normalization() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    gst::init().unwrap();
    let filter = build_audio_filter(&AudioEffects {
        replay_gain: reprise_core::library::settings::ReplayGainMode::Track,
        ..AudioEffects::default()
    })
    .unwrap()
    .unwrap();
    let bin = filter.downcast::<gst::Bin>().unwrap();
    let tee = bin
        .by_name("reprise-analysis-tee")
        .expect("the stable filter names its analysis split");
    let downstream = tee
        .src_pads()
        .into_iter()
        .filter_map(|pad| pad.peer())
        .filter_map(|pad| pad.parent_element())
        .map(|element| element.name().to_string())
        .collect::<std::collections::HashSet<_>>();
    let replay_gain = bin.by_name("reprise-replaygain").unwrap();
    let replay_gain_upstream = replay_gain
        .static_pad("sink")
        .and_then(|pad| pad.peer())
        .and_then(|pad| pad.parent_element())
        .expect("ReplayGain has an upstream playback queue");

    assert_eq!(
        downstream,
        ["reprise-playback-queue", "reprise-cava-queue"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        "the pre-ReplayGain tee must feed independent playback and CAVA branches"
    );
    assert_eq!(
        replay_gain_upstream.name(),
        "reprise-playback-queue",
        "ReplayGain must exist only after the audible playback branch splits"
    );
}

#[test]
fn ac_21_enabled_player_emits_live_cava_frames() {
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
fn ac_21_filter_replacement_reattaches_the_cava_processor() {
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
    player
        .set_audio_effects(AudioEffects {
            replay_gain: reprise_core::library::settings::ReplayGainMode::Track,
            ..AudioEffects::default()
        })
        .unwrap();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sine.flac");
    player.play(path).unwrap();

    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let frame = loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        let event = rx
            .recv_timeout(remaining)
            .expect("the replacement filter must keep emitting CAVA frames");
        if let PlayerEvent::Spectrum(frame) = event {
            break frame;
        }
    };

    assert!(frame.bands().iter().any(|value| *value > 0.0));
    player.stop().unwrap();
    std::env::remove_var(AUDIO_SINK_ENV_VAR);
}

#[test]
fn ac_21_stream_start_invalidates_the_previous_cava_history() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let player = Player::new(Box::new(|_| {})).unwrap();
    let before = player.cava_stream_generation.load(Ordering::SeqCst);
    let playbin = player
        .playbin
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();

    playbin
        .post_message(gst::message::StreamStart::builder().src(&playbin).build())
        .unwrap();
    while gst::glib::MainContext::default().pending() {
        gst::glib::MainContext::default().iteration(false);
    }

    assert!(player.cava_stream_generation.load(Ordering::SeqCst) > before);
}
