//! Proves the `PlaybackBackend` "Stream generations" contract (see
//! `reprise_core::playback`) against the real GStreamer backend: the counter
//! strictly increases across every kind of "start something new", and a
//! tagged event carries the generation that was current when its stream
//! started — not whatever generation happens to be current by the time a
//! consumer gets around to reading it.
//!
//! What this file does NOT attempt: reproducing the literal cross-stream
//! race the feature exists to let a consumer defend against (a stale
//! `TrackFinished`/`Error`/`Position` for an abandoned track delivered to a
//! consumer *after* it has already started a new one). That race is a
//! function of exactly when GStreamer posts a bus message relative to
//! exactly when a test thread calls `play()` again — not something this
//! headless `fakesink` harness can force to happen on demand without either
//! reaching into GStreamer internals or accepting a flaky, timing-dependent
//! test. What IS deterministically provable, and what these tests prove
//! instead, is the mechanism a consumer would use to defend against it: that
//! generations strictly increase and that each event is stamped with the
//! generation live at its own production instant.

use super::*;
use crate::player_pipeline::AUDIO_SINK_ENV_VAR;

/// Direct proof of the first required property: calling `play`/`play_uri`
/// again always produces a strictly greater generation than the call before
/// it, whether or not anything about the pipeline itself changed.
#[test]
fn consecutive_starts_produce_strictly_increasing_generations() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::env::set_var(AUDIO_SINK_ENV_VAR, "fakesink");

    let player = Player::new(Box::new(|_| {})).unwrap();
    assert_eq!(
        player.current_generation(),
        StreamGeneration::INITIAL,
        "before any stream has started, the generation must be the documented INITIAL"
    );

    let first = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sine.flac");
    player.play(first).unwrap();
    let after_first = player.current_generation();
    assert!(
        after_first > StreamGeneration::INITIAL,
        "play() must bump past INITIAL"
    );

    let second = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/blip.flac");
    player.play(second).unwrap();
    let after_second = player.current_generation();
    assert!(
        after_second > after_first,
        "a second play() must strictly increase the generation again"
    );

    // play_uri is the same "start something new" contract as play — a
    // file:// URI onto the same fixture exercises it without a real network.
    let third_uri = path_to_uri(first).unwrap();
    player.play_uri(&third_uri).unwrap();
    let after_third = player.current_generation();
    assert!(
        after_third > after_second,
        "play_uri() must strictly increase the generation too"
    );

    player.stop().unwrap();
    std::env::remove_var(AUDIO_SINK_ENV_VAR);
}

/// Direct proof of the second required property: an event emitted for a
/// stream carries the generation that was current when *that* stream
/// started — not, say, whatever generation is current by the time the test
/// gets around to draining the channel.
#[test]
fn tagged_event_carries_the_generation_current_when_its_stream_started() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::env::set_var(AUDIO_SINK_ENV_VAR, "fakesink");

    let (tx, rx) = std::sync::mpsc::channel::<StreamEvent>();
    let player = Player::new_with_generation(Box::new(move |tagged| {
        let _ = tx.send(tagged);
    }))
    .unwrap();

    let first = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sine.flac");
    player.play(first).unwrap();
    let playing_timeout = Duration::from_secs(5);
    let first_tagged = rx
        .recv_timeout(playing_timeout)
        .expect("expected a tagged StateChanged(Playing) within timeout");
    assert!(matches!(
        first_tagged.event,
        PlayerEvent::StateChanged(PlaybackState::Playing)
    ));
    let first_generation = first_tagged.generation;
    assert_eq!(
        first_generation,
        player.current_generation(),
        "the event produced by the first play() must carry exactly the \
         generation that play() just became current"
    );

    let second = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/blip.flac");
    player.play(second).unwrap();
    let second_tagged = loop {
        let tagged = rx
            .recv_timeout(playing_timeout)
            .expect("expected a tagged StateChanged(Playing) for the second stream");
        if matches!(
            tagged.event,
            PlayerEvent::StateChanged(PlaybackState::Playing)
        ) {
            break tagged;
        }
    };
    assert!(
        second_tagged.generation > first_generation,
        "the second stream's event must carry a strictly newer generation \
         than the first stream's, so a consumer that remembers {first_generation:?} \
         can tell the two apart"
    );
    assert_eq!(second_tagged.generation, player.current_generation());

    player.stop().unwrap();
    std::env::remove_var(AUDIO_SINK_ENV_VAR);
}

/// The gapless hand-off (`about-to-finish` swapping in a pre-fed URI without
/// a pipeline restart) is a new stream even though no `play`/`play_uri` call
/// drove it — see `gapless.rs::connect_about_to_finish`'s doc comment for why
/// the bump sits at the URI swap. Mirrors the deterministic, bus-driven
/// `gapless_handoff_advances_without_pipeline_restart` test above, tagged.
#[test]
fn gapless_handoff_carries_a_newer_generation_than_the_track_it_replaced() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::env::set_var(AUDIO_SINK_ENV_VAR, "fakesink");

    let (tx, rx) = std::sync::mpsc::channel::<StreamEvent>();
    let player = Player::new_with_generation(Box::new(move |tagged| {
        let _ = tx.send(tagged);
    }))
    .unwrap();

    let first = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sine.flac");
    let second = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/blip.flac");
    player.play(first).unwrap();
    let first_generation = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("expected a tagged StateChanged(Playing) for the first stream")
        .generation;
    player.set_next(Some(second));

    // Same pump-until-resolved pattern as `gapless_handoff_advances_without_
    // pipeline_restart`: the bus watch driving `AdvancedToNext` is dispatched
    // by the GLib main context, which nothing iterates in a headless test.
    let main_context = gst::glib::MainContext::default();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let advanced_generation = 'wait: loop {
        main_context.iteration(false);
        while let Ok(tagged) = rx.try_recv() {
            if matches!(tagged.event, PlayerEvent::AdvancedToNext) {
                break 'wait tagged.generation;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "expected a tagged AdvancedToNext within timeout"
        );
        std::thread::sleep(Duration::from_millis(10));
    };

    assert!(
        advanced_generation > first_generation,
        "the gapless hand-off must carry a strictly newer generation than \
         the track it replaced, even though no play()/play_uri() call drove it"
    );

    player.stop().unwrap();
    std::env::remove_var(AUDIO_SINK_ENV_VAR);
}

/// The crossfade promotion is likewise a new stream — see
/// `crossfade.rs::CrossfadeEngine::promote`'s doc comment for why the bump
/// sits at promotion rather than when the silent secondary pipeline first
/// starts (position ticks read through `self.playbin` still describe the
/// *outgoing* track for the whole ramp; bumping earlier would mislabel
/// them). Mirrors `crossfade_promotes_second_pipeline_and_advances_once`.
#[test]
fn crossfade_promotion_carries_a_newer_generation_than_the_track_it_replaced() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::env::set_var(AUDIO_SINK_ENV_VAR, "fakesink");

    let (tx, rx) = std::sync::mpsc::channel::<StreamEvent>();
    let player = Player::new_with_generation(Box::new(move |tagged| {
        let _ = tx.send(tagged);
    }))
    .unwrap();
    // Short fade (1 s), matching the existing crossfade backend test, so this
    // finishes quickly.
    player.set_transition(TrackTransition::Crossfade, 1);

    let first = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sine.flac");
    let second = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/blip.flac");
    player.play(first).unwrap();
    let first_generation = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("expected a tagged StateChanged(Playing) for the first stream")
        .generation;
    player.set_next(Some(second));

    let main_context = gst::glib::MainContext::default();
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    let mut observed_position_generations = Vec::new();
    let promoted_generation = 'wait: loop {
        main_context.iteration(false);
        while let Ok(tagged) = rx.try_recv() {
            match tagged.event {
                PlayerEvent::AdvancedToNext => break 'wait tagged.generation,
                PlayerEvent::Position { .. } => {
                    observed_position_generations.push(tagged.generation);
                }
                _ => {}
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "expected a tagged AdvancedToNext from the crossfade promotion within timeout"
        );
        std::thread::sleep(Duration::from_millis(10));
    };

    assert!(
        promoted_generation > first_generation,
        "the crossfade promotion must carry a strictly newer generation \
         than the track it replaced"
    );
    // Every Position tick observed before promotion is read through
    // `self.playbin`, which still points at the outgoing pipeline for the
    // whole ramp — none of them may have been mislabelled with the new
    // (post-promotion) generation.
    assert!(
        observed_position_generations
            .iter()
            .all(|generation| *generation == first_generation),
        "a Position tick observed before AdvancedToNext must still carry \
         the outgoing track's generation, got {observed_position_generations:?} \
         against first_generation={first_generation:?}"
    );

    player.stop().unwrap();
    std::env::remove_var(AUDIO_SINK_ENV_VAR);
}
