//! AC-29 at the core seam: a processor fed the desktop's 60 Hz cadence
//! (735 samples at 44.1 kHz) across a stream boundary.

use super::boundary_fixture::{
    assert_no_dip, judge_boundary, Frame, Measure, SyntheticMusic, FRAMES_PER_SECOND,
    JUDGED_FRAMES, SETTLE_FRAMES,
};
use super::{CavaBarProcessor, CavaConfig, SPECTRUM_BAND_COUNT};

const RATE_HZ: u32 = 44_100;
const HOP: usize = 735;
const LOUD_GAIN: f32 = 0.9;
const LEVEL_STEP_DB: f32 = 14.0;
/// Seconds the previous song plays before the boundary, and the reference
/// has been running before the moment it is compared at.
const WARM_SECONDS: usize = 12;
/// Where in the new song the boundary lands.
const BOUNDARY_SECONDS: usize = 30;
/// Frames recorded after the boundary.
const RECORDED_FRAMES: usize = SETTLE_FRAMES + JUDGED_FRAMES + FRAMES_PER_SECOND;

fn processor() -> CavaBarProcessor {
    CavaBarProcessor::new(CavaConfig::new(RATE_HZ, SPECTRUM_BAND_COUNT)).unwrap()
}

fn feed(
    processor: &mut CavaBarProcessor,
    music: &SyntheticMusic,
    from_seconds: usize,
    frames: usize,
) -> Vec<Frame> {
    let first = from_seconds * RATE_HZ as usize;
    (0..frames)
        .map(|frame| {
            let bars = processor.process(&music.mono(first + frame * HOP, HOP));
            bars.try_into().unwrap()
        })
        .collect()
}

fn warm(processor: &mut CavaBarProcessor, music: &SyntheticMusic, until_seconds: usize) {
    feed(
        processor,
        music,
        until_seconds - WARM_SECONDS,
        WARM_SECONDS * FRAMES_PER_SECOND,
    );
}

/// The frames the same song draws on a processor that has been running it.
fn settled_reference(music: &SyntheticMusic) -> Vec<Frame> {
    let mut reference = processor();
    warm(&mut reference, music, BOUNDARY_SECONDS);
    feed(&mut reference, music, BOUNDARY_SECONDS, RECORDED_FRAMES)
}

/// Plays `previous` for the warm-up, restarts the stream, and records `next`.
fn boundary_frames(previous: &SyntheticMusic, next: &SyntheticMusic) -> Vec<Frame> {
    let mut processor = processor();
    warm(&mut processor, previous, WARM_SECONDS);
    processor.reset_stream();
    feed(&mut processor, next, BOUNDARY_SECONDS, RECORDED_FRAMES)
}

fn loud() -> SyntheticMusic {
    SyntheticMusic::new(1, RATE_HZ, LOUD_GAIN)
}

fn quiet() -> SyntheticMusic {
    loud().louder_by(-LEVEL_STEP_DB)
}

#[test]
fn ac_29_a_fresh_processor_measures_the_first_song_instead_of_swelling() {
    for music in [loud(), quiet()] {
        let mut fresh = processor();
        let run = feed(&mut fresh, &music, BOUNDARY_SECONDS, RECORDED_FRAMES);
        judge_boundary("fresh start", &run, &settled_reference(&music), false);
    }
}

#[test]
fn ac_29_a_song_14_db_louder_does_not_hit_the_ceiling_on_a_carried_gain() {
    let run = boundary_frames(&quiet(), &loud());
    judge_boundary("quiet to loud", &run, &settled_reference(&loud()), true);
}

#[test]
fn ac_29_a_song_14_db_quieter_is_not_left_dim_on_a_carried_gain() {
    let run = boundary_frames(&loud(), &quiet());
    judge_boundary("loud to quiet", &run, &settled_reference(&quiet()), true);
}

#[test]
fn ac_29_a_song_of_the_same_loudness_keeps_its_height_across_the_boundary() {
    let other_song = SyntheticMusic::new(2, RATE_HZ, LOUD_GAIN);
    let run = boundary_frames(&other_song, &loud());
    judge_boundary("same loudness", &run, &settled_reference(&loud()), true);
}

#[test]
fn ac_29_a_seek_inside_a_song_keeps_its_height_across_the_boundary() {
    let mut processor = processor();
    warm(&mut processor, &loud(), WARM_SECONDS);
    processor.reset_stream();
    let run = feed(&mut processor, &loud(), BOUNDARY_SECONDS, RECORDED_FRAMES);
    judge_boundary("seek", &run, &settled_reference(&loud()), true);
    assert_no_dip("seek", &run, &settled_reference(&loud()));
}

// A song that opens quietly and then drops in at full level: the gain is
// measured on the intro, so the first loud bar has to pull it down at once
// instead of pinning the bars for as long as 2 % steps take.
#[test]
fn ac_29_a_loud_body_after_a_quiet_intro_does_not_pin_the_bars() {
    const INTRO_FRAMES: usize = 150;
    const JUDGED_AFTER_THE_STEP: usize = 2 * FRAMES_PER_SECOND;
    const INTRO_STEPS_DB: [f32; 3] = [14.0, 20.0, 30.0];
    const PINNED_SLACK: usize = 8;

    let reference = Measure::of(&settled_reference(&loud())[..JUDGED_AFTER_THE_STEP]);
    for step_db in INTRO_STEPS_DB {
        for previous in [Some(loud()), None] {
            let mut processor = processor();
            if let Some(previous) = previous {
                warm(&mut processor, &previous, WARM_SECONDS);
                processor.reset_stream();
            }
            let intro = loud().louder_by(-step_db);
            let frames: Vec<Frame> = (0..INTRO_FRAMES + JUDGED_AFTER_THE_STEP)
                .map(|frame| {
                    let music = if frame < INTRO_FRAMES {
                        &intro
                    } else {
                        &loud()
                    };
                    let bars = processor.process(&music.mono(frame * HOP, HOP));
                    bars.try_into().unwrap()
                })
                .collect();

            let after_the_step = Measure::of(&frames[INTRO_FRAMES..]);

            assert_eq!(
                after_the_step.wall_frames, 0,
                "an intro {step_db} dB down walled the drop: {after_the_step:?}"
            );
            assert!(
                after_the_step.pinned_frames <= reference.pinned_frames + PINNED_SLACK,
                "an intro {step_db} dB down pinned {} frames after the drop against the \
                 reference's {}",
                after_the_step.pinned_frames,
                reference.pinned_frames
            );
        }
    }
}
