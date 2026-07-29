//! The audio effects across the real bus.
//!
//! Split out of `runtime_service.rs` when that file reached the repository's
//! 800-line ceiling. Same suite, same harness — the equalizer just happens to
//! be the subject that pushed it over.

use std::time::Duration;

use reprise_runtime_client::{ClientEvent, RuntimeCommand};

use super::{await_event, start_with_bus_name, Served};

/// The equalizer across the bus, including the part that only exists because
/// a backend can say no.
#[test]
#[ignore = "requires a session bus; run via dbus-run-session"]
fn audio_effects_apply_and_come_back_in_the_snapshot() {
    use reprise_runtime_protocol::effects::EffectsRequest;

    let served = Served::start("effects", Duration::from_secs(60));
    let (client, events) =
        start_with_bus_name(vec!["playback:control".to_owned()], served.bus_name.clone());
    await_event(
        &events,
        |event| matches!(event, ClientEvent::Connected(_)),
        "connection",
    );

    client
        .call(RuntimeCommand::SetAudioEffects(EffectsRequest {
            equalizer_enabled: true,
            equalizer_bands: vec![3.0, 2.0, 1.0, 0.0, -1.0, -2.0, 0.0, 1.0, 2.0, 3.0],
            replay_gain: "album".into(),
        }))
        .expect("the backend accepts them");

    // Read through a second peer: a surface that was not the one that changed
    // this has to see it too, which is the whole reason it is a facet.
    let observer = served.client();
    let seen = observer.connect().expect("a second peer connects");
    assert!(seen.effects.equalizer_enabled);
    assert_eq!(seen.effects.replay_gain, "album");
    assert_eq!(
        seen.effects.equalizer_bands.len(),
        10,
        "ten gains cross as ten gains; a list that arrives short would have \
         the surface draw a different curve than the one in force"
    );
    assert!(!seen.effects.degraded);

    client.shutdown();
}

#[test]
#[ignore = "requires a session bus; run via dbus-run-session"]
fn an_equalizer_of_the_wrong_shape_is_refused_across_the_bus() {
    use reprise_runtime_protocol::effects::EffectsRequest;

    let served = Served::start("effects-shape", Duration::from_secs(60));
    let (client, events) =
        start_with_bus_name(vec!["playback:control".to_owned()], served.bus_name.clone());
    await_event(
        &events,
        |event| matches!(event, ClientEvent::Connected(_)),
        "connection",
    );

    let error = client
        .call(RuntimeCommand::SetAudioEffects(EffectsRequest {
            equalizer_enabled: true,
            equalizer_bands: vec![1.0, 2.0],
            replay_gain: "off".into(),
        }))
        .expect_err("two gains do not describe ten fixed centre frequencies");

    assert!(
        format!("{error}").contains("unknown_equalizer_shape"),
        "the category and its short kind have to survive the wire, or a \
         client cannot tell this from a backend failure it should retry: {error}"
    );

    client.shutdown();
}
