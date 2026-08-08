//! Proves the duration the position ticker *reports* stays with the stream
//! the position is measured from, across a real gapless hand-off — the
//! wiring, not just `reported_duration_ms`'s arithmetic.
//!
//! The unit tests next to that helper pin its three branches with literals,
//! which is exactly what a wiring mistake slips past: swapped arguments, a
//! `last_stable_duration_ms` overwritten while the hand-off is pending, or a
//! ticker that never reads the flag at all would leave every one of them
//! green. This test therefore runs the real ticker against a real `playbin3`
//! (headless, `fakesink` paced by the pipeline clock) and judges only what a
//! consumer of `PlayerEvent::Position` can see.
//!
//! The first track is generated rather than taken from `tests/fixtures`,
//! because the hand-off must be reached *after* the 500 ms ticker has
//! established a duration for the running stream. The shipped fixtures are
//! about a second long, so `playbin3` asks for the successor within ~90 ms —
//! before the first tick ever runs, leaving nothing stable to hold.

use std::path::Path;

use super::*;
use crate::player_pipeline::AUDIO_SINK_ENV_VAR;

/// Long enough that several ticks land before `about-to-finish` and several
/// after it: measured headless, `playbin3` asks for the successor roughly
/// 1.7 s before the end of this file.
const FIRST_TRACK_SECONDS: u32 = 6;
const FIXTURE_SAMPLE_RATE: u32 = 44_100;

/// Writes a mono 16-bit WAV of `seconds` of a 440 Hz sine. Generated rather
/// than committed for the reason in the module comment; `write_wav` in
/// `waveform.rs`'s tests does the same for its own fixtures.
fn write_sine_wav(path: &Path, seconds: u32) {
    let total_samples = (FIXTURE_SAMPLE_RATE * seconds) as usize;
    let data_size = u32::try_from(total_samples * 2).unwrap();
    let mut wav = Vec::with_capacity(44 + data_size as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_size).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&FIXTURE_SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(FIXTURE_SAMPLE_RATE * 2).to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    for index in 0..total_samples {
        let seconds_in = index as f64 / f64::from(FIXTURE_SAMPLE_RATE);
        let sample = (seconds_in * 440.0 * std::f64::consts::TAU).sin() * 8_000.0;
        wav.extend_from_slice(&(sample as i16).to_le_bytes());
    }
    std::fs::write(path, wav).unwrap();
}

/// The whole hand-off window — from the moment `about-to-finish` pre-feeds
/// the successor until `AdvancedToNext` confirms the swap — must not contain
/// a `Position` tick that moves the playhead backwards.
///
/// Two independent readings of that, both of which the pre-fix ticker
/// violates: the reported duration never shrinks from one tick to the next,
/// and no tick ever places the playhead past the end of the track it claims
/// to describe. Measured headless, the raw `query_duration` answers the
/// successor's length ~330 ms after `about-to-finish` while `query_position`
/// keeps measuring the outgoing stream for another 1.4 s, so the pre-fix
/// pairing reports position 4994 against duration 2000 — a fraction of 2.5
/// that collapses the playhead the instant the UI divides one by the other.
///
/// Holds `AUDIO_SINK_TEST_LOCK` for its full duration — see that lock.
#[test]
fn gapless_handoff_never_reports_a_duration_that_moves_the_playhead_backwards() {
    let _guard = AUDIO_SINK_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::env::set_var(AUDIO_SINK_ENV_VAR, "fakesink");

    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("first.wav");
    let second = directory.path().join("second.wav");
    write_sine_wav(&first, FIRST_TRACK_SECONDS);
    write_sine_wav(&second, 2);

    let (tx, rx) = std::sync::mpsc::channel::<PlayerEvent>();
    let player = Player::new(Box::new(move |event| {
        let _ = tx.send(event);
    }))
    .unwrap();

    player.play(first.to_str().unwrap()).unwrap();
    player.set_next(Some(second.to_str().unwrap()));

    // Same pump-until-resolved pattern as `gapless_handoff_advances_without_
    // pipeline_restart`: the bus watch that turns `StreamStart` into
    // `AdvancedToNext` is dispatched by the GLib main context, which nothing
    // iterates in a headless test.
    let main_context = gst::glib::MainContext::default();
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    let mut ticks: Vec<(i64, i64)> = Vec::new();
    let mut ticks_during_handoff = 0usize;
    let mut shortest_queried_during_handoff = i64::MAX;
    'wait: loop {
        main_context.iteration(false);
        let handoff_pending = player.handoff_pending.load(Ordering::SeqCst);
        while let Ok(event) = rx.try_recv() {
            match event {
                PlayerEvent::AdvancedToNext => break 'wait,
                PlayerEvent::Position {
                    position_ms,
                    duration_ms,
                } => {
                    ticks.push((position_ms, duration_ms));
                    if handoff_pending {
                        ticks_during_handoff += 1;
                    }
                }
                _ => {}
            }
        }
        if handoff_pending {
            let element = {
                let guard = player
                    .playbin
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                guard.clone()
            };
            if let Some(queried) = element.query_duration::<gst::ClockTime>() {
                shortest_queried_during_handoff =
                    shortest_queried_during_handoff.min(queried.mseconds() as i64);
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "expected an AdvancedToNext from the gapless handoff within timeout"
        );
        std::thread::sleep(Duration::from_millis(10));
    }

    // Everything below is only evidence if the run actually reached the
    // situation the fix exists for, so prove that first rather than passing
    // vacuously. Two ticks inside the window are 500 ms apart, so at least
    // one of them lands well after the ~330 ms it takes `playbin3` to start
    // answering `query_duration` for the successor — and the shortest answer
    // seen while the hand-off was pending must indeed be shorter than the
    // duration the ticker had settled on for the running stream.
    let settled_duration_ms = ticks
        .first()
        .expect("expected position ticks before the handoff")
        .1;
    assert!(
        ticks_during_handoff >= 2,
        "only {ticks_during_handoff} position tick(s) landed inside the handoff \
         window, which is too few to prove anything about it"
    );
    assert!(
        shortest_queried_during_handoff < settled_duration_ms,
        "playbin3 never answered the successor's (shorter) duration during the \
         handoff, so this run never reproduced the swap the fix guards against: \
         shortest queried {shortest_queried_during_handoff} against a settled \
         {settled_duration_ms}"
    );

    for pair in ticks.windows(2) {
        let ((_, previous_duration_ms), (_, duration_ms)) = (pair[0], pair[1]);
        assert!(
            duration_ms >= previous_duration_ms,
            "a reported duration shrank from {previous_duration_ms} to {duration_ms} \
             while the outgoing stream was still playing, which drops the playhead \
             backwards; ticks were {ticks:?}"
        );
    }
    for &(position_ms, duration_ms) in &ticks {
        assert!(
            position_ms <= duration_ms,
            "a tick placed the playhead at {position_ms} of a {duration_ms} track — \
             the position is measured on the outgoing stream and the duration on the \
             incoming one; ticks were {ticks:?}"
        );
    }

    player.stop().unwrap();
    std::env::remove_var(AUDIO_SINK_ENV_VAR);
}
