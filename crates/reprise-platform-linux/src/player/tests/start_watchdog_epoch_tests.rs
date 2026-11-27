//! A hung CUE start that was begun over is not disturbed by what the hung
//! pipeline reports late.
//!
//! The hung pipeline's jobs are stale once `restart_hung_start` has moved the
//! gate's epoch, and the new start's pending start-seek is the gate's now. A
//! late refusal of the hung pipeline's own seek would clear it, and the new
//! start would then never seek to the track's start, nor reach `Playing`.

use super::segment_support::{cue_item, record_heard};
use super::start_watchdog_restart_tests::{hold_the_restarted_start, on_the_injected_hang};
use super::start_watchdog_tests::{
    current_playbin, hang_starts, harness_with_deadline, PATIENCE, ROOMY_DEADLINE,
};
use super::*;

/// Silence, the track's tone, silence: the tone starts at the CUE track's
/// start, so the first buffer heard proves where the track began.
const CUE_START_MS: i64 = 2_000;
const CUE_END_MS: i64 = 4_000;
/// How far the first sample heard may lie from the CUE track's start.
const CUE_START_TOLERANCE_MS: i64 = 20;
/// The preroll at the file's start, then the seek to the track's start.
const SEEKS_AFTER_START: usize = 2;

fn playing(events: &[PlayerEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, PlayerEvent::StateChanged(PlaybackState::Playing)))
        .count()
}

fn current_queue(player: &Player) -> pipeline_queue::QueueHandle {
    player
        .playbin
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .queue()
}

/// Drives a hung CUE start through `restart_hung_start`, holds the new
/// pipeline's start back, and lets the hung pipeline's seek to the track's
/// start be refused late. The new start must still seek there and play.
#[test]
fn a_late_refusal_from_the_pipeline_a_start_over_replaced_leaves_the_new_start_alone() {
    on_the_injected_hang(|| {
        let harness = harness_with_deadline(ROOMY_DEADLINE);
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("album.wav");
        let flac = dir.path().join("album.flac");
        super::segment_support::write_regions_wav(
            &wav,
            &[(2_000, false), (2_000, true), (2_000, false)],
        );
        super::segment_support::encode_flac(&wav, &flac);
        let reached = hang_starts(&harness.player, 1);
        let heard = record_heard(&harness.player);
        let restart = hold_the_restarted_start();

        harness
            .player
            .play(cue_item(&flac, (CUE_START_MS, CUE_END_MS), 0.0))
            .unwrap();
        let hung = current_playbin(&harness.player);
        let hung_queue = current_queue(&harness.player);
        let hung_epoch = harness.player.segments.epoch();
        harness.pump_until(PATIENCE, |_| harness.player.start_watchdog.restarts() >= 1);
        if reached.load(Ordering::SeqCst) == 0
            || harness.player.start_watchdog.restarts() != 1
            || restart.held.recv_timeout(PATIENCE).is_err()
        {
            let _ = restart.release.send(());
            return false;
        }
        assert_ne!(current_playbin(&harness.player), hung, "no start over");
        let restarted_epoch = current_queue(&harness.player).start_epoch();
        assert_eq!(
            restarted_epoch,
            Some(harness.player.segments.epoch()),
            "the new pipeline does not carry the gate's epoch"
        );
        assert_ne!(restarted_epoch, Some(hung_epoch));
        // The control arm: the hung pipeline is out of service, and a seek on
        // it is refused, which is what its late job would see.
        harness.pump_until(PATIENCE, |_| hung_queue.is_finished());
        assert!(
            hung_queue.is_finished(),
            "the hung pipeline never went down"
        );
        assert!(hung
            .seek_simple(gst::SeekFlags::FLUSH, gst::ClockTime::ZERO)
            .is_err());

        harness.player.segments.run_late_start_job(
            &hung,
            &hung_queue,
            hung_epoch,
            Some(CUE_START_MS),
        );
        let _ = restart.release.send(());
        let events = harness.pump_until(PATIENCE, |collected| {
            playing(collected) > 0 && heard.landings() >= SEEKS_AFTER_START
        });

        assert!(
            playing(&events) > 0,
            "the late refusal cleared the new start's pending seek: {events:?}"
        );
        let first = heard.latest_landing();
        assert!(
            (first.start_ms - CUE_START_MS).abs() <= CUE_START_TOLERANCE_MS,
            "the new start must begin at the track's start, got {first:?}"
        );
        true
    });
}
