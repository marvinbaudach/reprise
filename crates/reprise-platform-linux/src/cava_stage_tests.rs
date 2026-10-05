//! AC-29 at the desktop pipeline seam: the stage the PCM tap drives, fed the
//! 60 Hz buffers (735 samples at 44.1 kHz) the real tap delivers, across the
//! three events that restart it.

use super::*;
use reprise_core::playback::boundary_fixture::{
    assert_no_dip, judge_boundary, Frame, SyntheticMusic, FRAMES_PER_SECOND, JUDGED_FRAMES,
    SETTLE_FRAMES,
};

const RATE_HZ: u32 = 44_100;
const HOP: usize = 735;
const LEVEL_STEP_DB: f32 = 14.0;
const WARM_SECONDS: usize = 12;
const BOUNDARY_SECONDS: usize = 30;
const RECORDED_FRAMES: usize = SETTLE_FRAMES + JUDGED_FRAMES + FRAMES_PER_SECOND;
const FIRST_STREAM: u64 = 1;

fn loud() -> SyntheticMusic {
    SyntheticMusic::new(1, RATE_HZ, 0.9)
}

fn quiet() -> SyntheticMusic {
    loud().louder_by(-LEVEL_STEP_DB)
}

/// What the tap does with consecutive buffers of `music` starting `from`
/// seconds into it. `discontinuous` is the DISCONT flag on the first one.
fn play(
    stage: &mut CavaStage,
    music: &SyntheticMusic,
    generation: u64,
    from_seconds: usize,
    frames: usize,
    discontinuous: bool,
) -> Vec<Frame> {
    let first = from_seconds * RATE_HZ as usize;
    (0..frames)
        .map(|frame| {
            let pcm = music.mono(first + frame * HOP, HOP);
            *stage
                .analyze(generation, discontinuous && frame == 0, &pcm)
                .bands()
        })
        .collect()
}

/// A stage that has been playing `music` up to the boundary.
fn warmed(music: &SyntheticMusic) -> CavaStage {
    let mut stage = CavaStage::new(RATE_HZ, FIRST_STREAM).unwrap();
    play(
        &mut stage,
        music,
        FIRST_STREAM,
        BOUNDARY_SECONDS - WARM_SECONDS,
        WARM_SECONDS * FRAMES_PER_SECOND,
        false,
    );
    stage
}

/// The frames `music` draws on a stage that has been playing it all along.
fn settled_reference(music: &SyntheticMusic) -> Vec<Frame> {
    play(
        &mut warmed(music),
        music,
        FIRST_STREAM,
        BOUNDARY_SECONDS,
        RECORDED_FRAMES,
        false,
    )
}

#[test]
fn restart_for_names_the_restart_each_event_needs() {
    assert_eq!(restart_for(false, false, false), Restart::Hard);
    assert_eq!(restart_for(false, true, true), Restart::Hard);
    assert_eq!(restart_for(true, true, false), Restart::Stream);
    assert_eq!(restart_for(true, false, true), Restart::Stream);
    assert_eq!(restart_for(true, false, false), Restart::None);
}

#[test]
fn ac_29_a_seek_keeps_the_shape_and_the_height_the_song_was_drawn_at() {
    let mut stage = warmed(&loud());

    // A seek lands in the same song: the buffer after it carries DISCONT.
    let run = play(
        &mut stage,
        &loud(),
        FIRST_STREAM,
        BOUNDARY_SECONDS,
        RECORDED_FRAMES,
        true,
    );

    judge_boundary("seek", &run, &settled_reference(&loud()), true);
    assert_no_dip("seek", &run, &settled_reference(&loud()));
}

#[test]
fn ac_29_a_new_stream_keeps_the_shape_and_measures_the_new_songs_level() {
    let mut stage = warmed(&loud());

    let run = play(
        &mut stage,
        &quiet(),
        FIRST_STREAM + 1,
        BOUNDARY_SECONDS,
        RECORDED_FRAMES,
        true,
    );

    judge_boundary("new stream", &run, &settled_reference(&quiet()), true);
}

#[test]
fn ac_29_enabling_the_visualizer_measures_the_level_and_starts_from_nothing() {
    let mut stage = warmed(&quiet());
    stage.disable();

    let run = play(
        &mut stage,
        &loud(),
        FIRST_STREAM,
        BOUNDARY_SECONDS,
        RECORDED_FRAMES,
        false,
    );

    // Nothing was on screen to continue, so the first frames start low.
    judge_boundary("enable", &run, &settled_reference(&loud()), false);
}
