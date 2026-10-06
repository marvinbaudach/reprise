//! AC-29 at the desktop pipeline seam: the stage the PCM tap drives, fed the
//! 60 Hz buffers (735 samples at 44.1 kHz) the real tap delivers, across the
//! three events that restart it.

use super::*;
use reprise_core::playback::boundary_fixture::{
    assert_no_dip, judge_boundary, pinning_complaints, Boundary, Frame, Measure, Opening,
    SyntheticMusic, BOUNDARY_OFFSETS_SECONDS, FRAMES_PER_SECOND, JUDGED_FRAMES, SETTLE_FRAMES,
};

const RATE_HZ: u32 = 44_100;
const HOP: usize = 735;
const LEVEL_STEP_DB: f32 = 14.0;
const WARM_SECONDS: usize = 16;
const BOUNDARY_SECONDS: usize = 30;
const RECORDED_FRAMES: usize = SETTLE_FRAMES + JUDGED_FRAMES + FRAMES_PER_SECOND;
const FIRST_STREAM: u64 = 1;

fn loud() -> SyntheticMusic {
    SyntheticMusic::new(1, RATE_HZ, 0.9)
}

fn quiet() -> SyntheticMusic {
    loud().louder_by(-LEVEL_STEP_DB)
}

/// The sample `offset_seconds` past `BOUNDARY_SECONDS` into a song.
fn boundary_sample(offset_seconds: f32) -> usize {
    BOUNDARY_SECONDS * RATE_HZ as usize + (offset_seconds * RATE_HZ as f32) as usize
}

/// What the tap does with consecutive buffers of `music` starting at sample
/// `first`. `discontinuous` is the DISCONT flag on the first one.
fn play(
    stage: &mut CavaStage,
    music: &SyntheticMusic,
    generation: u64,
    first: usize,
    frames: usize,
    discontinuous: bool,
) -> Vec<Frame> {
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
fn warmed(music: &SyntheticMusic, offset_seconds: f32) -> CavaStage {
    let mut stage = CavaStage::new(RATE_HZ, FIRST_STREAM).unwrap();
    play(
        &mut stage,
        music,
        FIRST_STREAM,
        boundary_sample(offset_seconds) - WARM_SECONDS * RATE_HZ as usize,
        WARM_SECONDS * FRAMES_PER_SECOND,
        false,
    );
    stage
}

/// The frames `music` draws on a stage that has been playing it all along.
fn settled_reference(music: &SyntheticMusic, offset_seconds: f32) -> Vec<Frame> {
    play(
        &mut warmed(music, offset_seconds),
        music,
        FIRST_STREAM,
        boundary_sample(offset_seconds),
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
    for offset in BOUNDARY_OFFSETS_SECONDS {
        let mut stage = warmed(&loud(), offset);

        // A seek lands in the same song: the buffer after it carries DISCONT.
        let run = play(
            &mut stage,
            &loud(),
            FIRST_STREAM,
            boundary_sample(offset),
            RECORDED_FRAMES,
            true,
        );

        let reference = settled_reference(&loud(), offset);
        let label = format!("seek at +{offset}");
        judge_boundary(&label, &run, &reference, Boundary::Continuing);
        assert_no_dip(&label, &run, &reference);
    }
}

#[test]
fn ac_29_a_new_stream_keeps_the_shape_and_measures_the_new_songs_level() {
    for offset in BOUNDARY_OFFSETS_SECONDS {
        let mut stage = warmed(&loud(), offset);

        let run = play(
            &mut stage,
            &quiet(),
            FIRST_STREAM + 1,
            boundary_sample(offset),
            RECORDED_FRAMES,
            true,
        );

        judge_boundary(
            &format!("new stream at +{offset}"),
            &run,
            &settled_reference(&quiet(), offset),
            Boundary::Different,
        );
    }
}

#[test]
fn ac_29_enabling_the_visualizer_measures_the_level_and_starts_from_nothing() {
    for offset in BOUNDARY_OFFSETS_SECONDS {
        let mut stage = warmed(&quiet(), offset);
        stage.disable();

        let run = play(
            &mut stage,
            &loud(),
            FIRST_STREAM,
            boundary_sample(offset),
            RECORDED_FRAMES,
            false,
        );

        // Nothing was on screen to continue, so the first frames start low.
        judge_boundary(
            &format!("enable at +{offset}"),
            &run,
            &settled_reference(&loud(), offset),
            Boundary::Fresh,
        );
    }
}

/// Frames more than the settled stage that may touch full height.
const PINNED_SLACK: usize = 6;

/// The loud song opening as `opening` says, at the boundary.
fn opened(opening: Opening) -> SyntheticMusic {
    loud().opened_by(opening, boundary_sample(0.0))
}

// A long, quiet intro: the gain was measured on it, and the body has to be
// caught however late it comes, on a first start and on a new stream.
#[test]
fn ac_29_a_loud_body_after_a_long_quiet_intro_does_not_pin_the_new_streams_bars() {
    const BODY_FRAMES: usize = 2 * FRAMES_PER_SECOND;
    let mut complaints = Vec::new();
    for (seconds, db) in [(8, 30.0), (10, 14.0)] {
        let song = opened(Opening::Intro { seconds, db });
        let first = seconds * FRAMES_PER_SECOND;
        let reference = play(
            &mut warmed(&loud(), 0.0),
            &loud(),
            FIRST_STREAM,
            boundary_sample(0.0) + seconds * RATE_HZ as usize,
            BODY_FRAMES,
            false,
        );
        let allowed = Measure::of(&reference).pinned_frames + PINNED_SLACK;
        for new_stream in [false, true] {
            let (mut stage, generation) = if new_stream {
                (warmed(&loud(), 0.0), FIRST_STREAM + 1)
            } else {
                (CavaStage::new(RATE_HZ, FIRST_STREAM).unwrap(), FIRST_STREAM)
            };
            let frames = play(
                &mut stage,
                &song,
                generation,
                boundary_sample(0.0),
                first + BODY_FRAMES,
                new_stream,
            );
            complaints.extend(pinning_complaints(
                &format!("a {seconds} s intro {db} dB down, new stream {new_stream}"),
                Measure::of(&frames[first..]),
                allowed,
            ));
        }
    }
    assert!(complaints.is_empty(), "{complaints:#?}");
}

// A song that rises out of silence, judged against the stage that has been
// playing the loud song and goes on into the fade.
#[test]
fn ac_29_a_fade_in_does_not_pin_the_new_streams_bars() {
    const FADE_FRAMES: usize = 10 * FRAMES_PER_SECOND;
    let mut complaints = Vec::new();
    for opening in [
        Opening::LinearFade { seconds: 3 },
        Opening::DbFade { seconds: 5 },
    ] {
        let song = opened(opening);
        let reference = play(
            &mut warmed(&loud(), 0.0),
            &song,
            FIRST_STREAM,
            boundary_sample(0.0),
            FADE_FRAMES,
            false,
        );
        let allowed = Measure::of(&reference).pinned_frames + PINNED_SLACK;
        for new_stream in [false, true] {
            let (mut stage, generation) = if new_stream {
                (warmed(&loud(), 0.0), FIRST_STREAM + 1)
            } else {
                (CavaStage::new(RATE_HZ, FIRST_STREAM).unwrap(), FIRST_STREAM)
            };
            let frames = play(
                &mut stage,
                &song,
                generation,
                boundary_sample(0.0),
                FADE_FRAMES,
                new_stream,
            );
            complaints.extend(pinning_complaints(
                &format!("a {opening:?}, new stream {new_stream}"),
                Measure::of(&frames),
                allowed,
            ));
        }
    }
    assert!(complaints.is_empty(), "{complaints:#?}");
}
