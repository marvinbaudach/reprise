//! The equalizer and ReplayGain, owned by whoever owns the audio path.
//!
//! The facet a surface reads is what the backend *accepted*, not what the
//! settings table holds. The two come apart whenever the pipeline has no
//! element for what was asked, and a runtime that published only the stored
//! value would have every surface showing an equalizer that does nothing.

use reprise_core::library::settings::{self, ReplayGainMode};
use reprise_core::playback::AudioEffects;
use reprise_runtime_protocol::effects::EffectsRequest;

use crate::event::RuntimeEvent;
use crate::runtime::{Command, Runtime};

use super::{full_client, harness, over, Harness};

/// The same curve as a wire request — what a surface actually sends.
fn a_request() -> EffectsRequest {
    EffectsRequest {
        equalizer_enabled: true,
        equalizer_bands: a_curve().equalizer_bands.to_vec(),
        replay_gain: "track".into(),
    }
}

fn a_curve() -> AudioEffects {
    AudioEffects {
        equalizer_enabled: true,
        equalizer_bands: [3.0, 2.0, 1.0, 0.0, -1.0, -2.0, 0.0, 1.0, 2.0, 3.0],
        replay_gain: ReplayGainMode::Track,
    }
}

/// A runtime whose backend has no equalizer element, with a curve already
/// stored — the situation `apply_stored`'s fallback exists for.
///
/// Built by hand rather than through `over`, because the refusal has to be
/// in place *before* `Runtime::new` applies the stored effects, and `over`
/// hands the backend to the runtime in the same expression that builds it.
fn refused_effects_harness() -> Harness {
    let db = reprise_core::db::Db::open_in_memory().expect("an in-memory database migrates");
    reprise_core::library::audio_effect_settings::store(&db, &a_curve())
        .expect("the settings are writable");
    let playback = crate::fakes::FakePlayback::new();
    let devices = crate::fakes::FakeDevices::new();
    let clock = crate::fakes::FakeClock::starting_at(1_753_600_000);
    let handles = (playback.handle(), devices.handle(), clock.handle());
    handles.0.refuse_effects(true);
    let ports = crate::ports::Ports {
        playback: Box::new(playback),
        library: Box::new(crate::fakes::FakeLibrary::with_tracks([1, 2, 3])),
        devices: Box::new(devices),
        clock: Box::new(clock),
    };
    Harness {
        runtime: Runtime::new(db, ports),
        playback: handles.0,
        devices: handles.1,
        clock: handles.2,
    }
}

#[test]
fn a_fresh_runtime_applies_what_was_stored() {
    let db = reprise_core::db::Db::open_in_memory().expect("an in-memory database migrates");
    reprise_core::library::audio_effect_settings::store(&db, &a_curve())
        .expect("the settings are writable");

    let harness = over(db);

    assert_eq!(
        harness.playback.accepted_effects().last(),
        Some(&a_curve()),
        "the backend starts on its own defaults; a runtime that does not \
         push the stored effects leaves the user's equalizer silently off"
    );
    let effects = harness.runtime.snapshot().unwrap().effects;
    assert!(effects.equalizer_enabled);
    assert_eq!(effects.replay_gain, "track");
    assert!(!effects.degraded, "nothing was refused");
}

#[test]
fn a_backend_without_an_equalizer_still_plays() {
    let harness = refused_effects_harness();

    let effects = harness.runtime.snapshot().unwrap().effects;
    assert!(
        !effects.equalizer_enabled,
        "an equalizer the pipeline cannot build must not be reported as on"
    );
    assert!(
        effects.degraded,
        "and the surface has to be able to say why, rather than showing a \
         flat disabled equalizer that looks exactly like an untouched install"
    );
}

#[test]
fn a_refused_equalizer_keeps_the_curve_the_user_dialled_in() {
    let harness = refused_effects_harness();

    let db = harness.runtime.database();
    assert!(
        !settings::get_equalizer_enabled(db),
        "the switch goes off, so the next start does not repeat a failure \
         the user has already been shown"
    );
    assert_eq!(
        settings::get_equalizer_bands(db),
        a_curve().equalizer_bands,
        "but the curve stays: it is work someone did, and flattening it over \
         a missing plugin that may be installed tomorrow destroys it silently"
    );
    assert_eq!(
        settings::get_replay_gain_mode(db),
        ReplayGainMode::Off,
        "ReplayGain is a switch too, not a curve"
    );
}

#[test]
fn setting_effects_applies_them_and_publishes_the_change() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    harness.runtime.drain(client).unwrap();

    harness
        .runtime
        .command(client, &Command::SetAudioEffects(a_request()))
        .expect("the backend accepts them");

    assert_eq!(
        harness.playback.accepted_effects().last(),
        Some(&a_curve()),
        "the sound has to actually change, not just the snapshot"
    );
    let events = harness.runtime.drain(client).unwrap().events;
    assert!(
        events
            .iter()
            .any(|event| matches!(&event.event, RuntimeEvent::EffectsChanged(_))),
        "a second surface showing the equalizer has no other way to learn it \
         moved"
    );
}

#[test]
fn effects_that_the_backend_refuses_are_not_stored() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    harness.playback.refuse_effects(true);

    let error = harness
        .runtime
        .command(client, &Command::SetAudioEffects(a_request()))
        .expect_err("the pipeline has no equalizer element");

    assert_eq!(error.category(), "failed");
    assert!(
        !settings::get_equalizer_enabled(harness.runtime.database()),
        "storing a setting the audio path refused would have the next start \
         read it, fail on it, and switch it off — the user's change undoing \
         itself one launch later, far from what caused it"
    );
}

#[test]
fn effects_survive_a_track_change() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    harness
        .runtime
        .command(client, &Command::SetAudioEffects(a_request()))
        .unwrap();

    harness
        .runtime
        .command(
            client,
            &Command::PlayTracks {
                track_ids: vec![1, 2],
                start_index: 0,
            },
        )
        .unwrap();

    let effects = harness.runtime.snapshot().unwrap().effects;
    assert!(
        effects.equalizer_enabled,
        "effects belong to the audio path, not to what is loaded — this is \
         why they are their own facet rather than part of playback, which is \
         empty when nothing plays"
    );
}

#[test]
fn an_equalizer_of_the_wrong_shape_is_rejected_rather_than_padded() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    // Not zero: startup already applied the stored effects, which is the
    // whole point of `apply_stored`.
    let before = harness.playback.accepted_effects().len();

    let error = harness
        .runtime
        .command(
            client,
            &Command::SetAudioEffects(EffectsRequest {
                equalizer_bands: vec![1.0, 2.0, 3.0],
                ..a_request()
            }),
        )
        .expect_err("three gains do not describe ten fixed centre frequencies");

    assert_eq!(error.kind(), "rejected:unknown_equalizer_shape");
    assert_eq!(
        harness.playback.accepted_effects().len(),
        before,
        "and the backend was never asked: padding this would silently apply \
         gains to frequencies the caller never named"
    );
}

#[test]
fn an_unknown_replay_gain_mode_is_rejected() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);

    let error = harness
        .runtime
        .command(
            client,
            &Command::SetAudioEffects(EffectsRequest {
                replay_gain: "loudest".into(),
                ..a_request()
            }),
        )
        .expect_err("there are three modes and that is not one");

    assert_eq!(error.kind(), "rejected:unknown_replay_gain_mode");
}
