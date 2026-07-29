//! Spectrum frames, which are the one thing here that is not state.
//!
//! Everything else the runtime publishes is a facet: it says what something
//! looks like now, a client that missed a delta can ask again, and a mailbox
//! that overflows means "resynchronize". A spectrum frame fits none of that.
//! It arrives ~60 times a second, it describes an instant that has already
//! passed, and there is nothing to catch up on — the 256-entry mailbox would
//! hold roughly four seconds of them before telling a client to start over
//! about data that was worthless long before.
//!
//! So frames take their own path: one slot per client, latest wins, no
//! sequence, no overflow, and no analysis at all while nobody is watching.

use reprise_core::playback::{PlayerEvent, SpectrumFrame, StreamEvent, StreamGeneration};

use crate::runtime::Command;

use super::{full_client, harness};

fn frame(first_band: f32) -> SpectrumFrame {
    let mut bands = [0.0_f32; reprise_core::playback::SPECTRUM_BAND_COUNT];
    bands[0] = first_band;
    SpectrumFrame::from_cava_bars(bands)
}

fn emit(harness: &mut super::Harness, frame: SpectrumFrame) {
    harness.runtime.on_player_event(&StreamEvent {
        generation: StreamGeneration::from(0),
        event: PlayerEvent::Spectrum(frame),
    });
}

#[test]
fn nobody_is_offered_frames_until_they_ask() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);

    emit(&mut harness, frame(0.5));

    assert!(
        harness.runtime.take_spectrum(client).is_none(),
        "a client that draws no visualizer must not be handed 60 frames a \
         second it will only throw away"
    );
}

#[test]
fn a_watcher_is_offered_the_newest_frame() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    harness
        .runtime
        .command(client, &Command::WatchSpectrum(true))
        .unwrap();

    emit(&mut harness, frame(0.25));

    let taken = harness.runtime.take_spectrum(client).expect("a frame");
    assert_eq!(taken.bands()[0], 0.25);
}

#[test]
fn an_untaken_frame_is_replaced_rather_than_queued() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    harness
        .runtime
        .command(client, &Command::WatchSpectrum(true))
        .unwrap();

    emit(&mut harness, frame(0.1));
    emit(&mut harness, frame(0.2));
    emit(&mut harness, frame(0.3));

    assert_eq!(
        harness
            .runtime
            .take_spectrum(client)
            .expect("a frame")
            .bands()[0],
        0.3,
        "under render load the frames are strictly latest-wins (AC-23); \
         drawing a backlog would show the user the past, slowly"
    );
    assert!(
        harness.runtime.take_spectrum(client).is_none(),
        "and there is no backlog behind it"
    );
}

#[test]
fn frames_never_reach_the_mailbox() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    harness
        .runtime
        .command(client, &Command::WatchSpectrum(true))
        .unwrap();
    harness.runtime.drain(client).unwrap();

    // Well past the mailbox's capacity. If frames were published as events,
    // this would overflow it and demand a resynchronize.
    for tick in 0..600 {
        emit(&mut harness, frame(tick as f32 / 600.0));
    }

    let delivery = harness.runtime.drain(client).unwrap();
    assert!(
        delivery.events.is_empty(),
        "a frame is not an event: it carries no sequence and nothing orders \
         itself against it"
    );
    assert!(
        !delivery.resynchronize,
        "ten seconds of visualizer must not cost a client its whole state — \
         that is the flood this separate path exists to prevent"
    );
}

#[test]
fn the_backend_only_analyses_while_someone_watches() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);

    harness
        .runtime
        .command(client, &Command::WatchSpectrum(true))
        .unwrap();
    assert_eq!(
        harness.playback.spectrum_switches(),
        vec![true],
        "analysing audio for nobody is CPU spent on the hot path for no reason"
    );

    harness
        .runtime
        .command(client, &Command::WatchSpectrum(false))
        .unwrap();
    assert_eq!(harness.playback.spectrum_switches(), vec![true, false]);
}

#[test]
fn a_second_watcher_does_not_re_enable_what_is_already_running() {
    let mut harness = harness();
    let first = full_client(&mut harness.runtime);
    let second = full_client(&mut harness.runtime);

    harness
        .runtime
        .command(first, &Command::WatchSpectrum(true))
        .unwrap();
    harness
        .runtime
        .command(second, &Command::WatchSpectrum(true))
        .unwrap();

    assert_eq!(
        harness.playback.spectrum_switches(),
        vec![true],
        "the backend is told when the total flips, not on every request"
    );
}

#[test]
fn the_last_watcher_leaving_stops_the_analysis_and_the_first_one_does_not() {
    let mut harness = harness();
    let first = full_client(&mut harness.runtime);
    let second = full_client(&mut harness.runtime);
    harness
        .runtime
        .command(first, &Command::WatchSpectrum(true))
        .unwrap();
    harness
        .runtime
        .command(second, &Command::WatchSpectrum(true))
        .unwrap();

    harness
        .runtime
        .command(first, &Command::WatchSpectrum(false))
        .unwrap();
    assert_eq!(
        harness.playback.spectrum_switches(),
        vec![true],
        "one surface closing its visualizer must not blank the other's"
    );

    harness
        .runtime
        .command(second, &Command::WatchSpectrum(false))
        .unwrap();
    assert_eq!(harness.playback.spectrum_switches(), vec![true, false]);
}

#[test]
fn stopping_watching_drops_the_frame_that_was_waiting() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    harness
        .runtime
        .command(client, &Command::WatchSpectrum(true))
        .unwrap();
    emit(&mut harness, frame(0.4));

    harness
        .runtime
        .command(client, &Command::WatchSpectrum(false))
        .unwrap();

    assert!(
        harness.runtime.take_spectrum(client).is_none(),
        "handing over a frame from before the visualizer was closed would \
         draw one last picture into a surface that has stopped listening"
    );
}

#[test]
fn one_clients_frames_are_not_another_clients() {
    let mut harness = harness();
    let watcher = full_client(&mut harness.runtime);
    let bystander = full_client(&mut harness.runtime);
    harness
        .runtime
        .command(watcher, &Command::WatchSpectrum(true))
        .unwrap();

    emit(&mut harness, frame(0.6));

    assert!(harness.runtime.take_spectrum(watcher).is_some());
    assert!(
        harness.runtime.take_spectrum(bystander).is_none(),
        "taking a frame is per client; a shared slot would have whichever \
         surface polled first starve the other"
    );
}
