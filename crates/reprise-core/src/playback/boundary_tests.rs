//! AC-29 at the core seam: a processor fed the desktop's 60 Hz cadence
//! (735 samples at 44.1 kHz) across a stream boundary.

use super::boundary_fixture::{
    assert_no_dip, assert_settles_where_a_continuing_run_does, judge_boundary, Boundary, Frame,
    Measure, SyntheticMusic, BOUNDARY_OFFSETS_SECONDS, FRAMES_PER_SECOND, SETTLED_FRAMES,
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
/// Frames recorded when the stretch three to ten seconds after the boundary is judged.
const LONG_RECORDING_FRAMES: usize = SETTLED_FRAMES.end;

fn processor() -> CavaBarProcessor {
    CavaBarProcessor::new(CavaConfig::new(RATE_HZ, SPECTRUM_BAND_COUNT)).unwrap()
}

/// The sample `offset_seconds` past `BOUNDARY_SECONDS` into a song.
fn boundary_sample(offset_seconds: f32) -> usize {
    BOUNDARY_SECONDS * RATE_HZ as usize + (offset_seconds * RATE_HZ as f32) as usize
}

/// Feeds `frames` hops of `music` starting at sample `first`.
fn feed_from(
    processor: &mut CavaBarProcessor,
    music: &SyntheticMusic,
    first: usize,
    frames: usize,
) -> Vec<Frame> {
    (0..frames)
        .map(|frame| {
            let bars = processor.process(&music.mono(first + frame * HOP, HOP));
            bars.try_into().unwrap()
        })
        .collect()
}

/// Feeds `music` up to the boundary, for `WARM_SECONDS`.
fn warm(processor: &mut CavaBarProcessor, music: &SyntheticMusic, offset_seconds: f32) {
    feed_from(
        processor,
        music,
        boundary_sample(offset_seconds) - WARM_SECONDS * RATE_HZ as usize,
        WARM_SECONDS * FRAMES_PER_SECOND,
    );
}

/// The frames the same song draws on a processor that has been running it.
fn settled_reference(music: &SyntheticMusic, offset_seconds: f32) -> Vec<Frame> {
    let mut reference = processor();
    warm(&mut reference, music, offset_seconds);
    feed_from(
        &mut reference,
        music,
        boundary_sample(offset_seconds),
        LONG_RECORDING_FRAMES,
    )
}

/// Plays `previous` for the warm-up, restarts the stream, and records `next`.
fn boundary_frames(
    previous: &SyntheticMusic,
    next: &SyntheticMusic,
    offset_seconds: f32,
) -> Vec<Frame> {
    let mut processor = processor();
    warm(&mut processor, previous, offset_seconds);
    processor.reset_stream();
    feed_from(
        &mut processor,
        next,
        boundary_sample(offset_seconds),
        LONG_RECORDING_FRAMES,
    )
}

fn loud() -> SyntheticMusic {
    SyntheticMusic::new(1, RATE_HZ, LOUD_GAIN)
}

fn quiet() -> SyntheticMusic {
    loud().louder_by(-LEVEL_STEP_DB)
}

#[test]
fn ac_29_a_fresh_processor_measures_the_first_song_instead_of_swelling() {
    for offset in BOUNDARY_OFFSETS_SECONDS {
        for music in [loud(), quiet()] {
            let mut fresh = processor();
            let run = feed_from(
                &mut fresh,
                &music,
                boundary_sample(offset),
                LONG_RECORDING_FRAMES,
            );
            judge_boundary(
                &format!("fresh start at +{offset}"),
                &run,
                &settled_reference(&music, offset),
                Boundary::Fresh,
            );
        }
    }
}

#[test]
fn ac_29_a_song_14_db_louder_does_not_hit_the_ceiling_on_a_carried_gain() {
    for offset in BOUNDARY_OFFSETS_SECONDS {
        let run = boundary_frames(&quiet(), &loud(), offset);
        judge_boundary(
            &format!("quiet to loud at +{offset}"),
            &run,
            &settled_reference(&loud(), offset),
            Boundary::Different,
        );
    }
}

#[test]
fn ac_29_a_song_14_db_quieter_is_not_left_dim_on_a_carried_gain() {
    for offset in BOUNDARY_OFFSETS_SECONDS {
        let run = boundary_frames(&loud(), &quiet(), offset);
        judge_boundary(
            &format!("loud to quiet at +{offset}"),
            &run,
            &settled_reference(&quiet(), offset),
            Boundary::Different,
        );
    }
}

#[test]
fn ac_29_a_song_of_the_same_loudness_keeps_its_height_across_the_boundary() {
    let other_song = SyntheticMusic::new(2, RATE_HZ, LOUD_GAIN);
    for offset in BOUNDARY_OFFSETS_SECONDS {
        let run = boundary_frames(&other_song, &loud(), offset);
        judge_boundary(
            &format!("same loudness at +{offset}"),
            &run,
            &settled_reference(&loud(), offset),
            Boundary::Continuing,
        );
    }
}

#[test]
fn ac_29_a_seek_inside_a_song_keeps_its_height_across_the_boundary() {
    for offset in BOUNDARY_OFFSETS_SECONDS {
        let run = boundary_frames(&loud(), &loud(), offset);
        let reference = settled_reference(&loud(), offset);
        judge_boundary(
            &format!("seek at +{offset}"),
            &run,
            &reference,
            Boundary::Continuing,
        );
        assert_no_dip(&format!("seek at +{offset}"), &run, &reference);
    }
}

// The boundary hands back to cavacore's creep, which has to find the level a
// run that never had a boundary sits at. A gain held below it settles dim and
// then swells back at the creep's 6 % a second.
#[test]
fn ac_29_a_boundary_inside_a_song_settles_where_a_continuing_run_does() {
    for offset in BOUNDARY_OFFSETS_SECONDS {
        for music in [loud(), quiet()] {
            let continuing = settled_reference(&music, offset);
            let run = boundary_frames(&music, &music, offset);
            assert_settles_where_a_continuing_run_does(
                &format!("boundary at +{offset}"),
                &run,
                &continuing,
            );
        }
    }
}

// A silent chunk is part of the music: it must neither restart the boundary's
// measurement nor keep the tracking going once the seven seconds are over.
// Control arm: the same audio without the gaps in the first seven seconds.
#[test]
fn ac_29_silent_gaps_inside_a_song_do_not_keep_the_boundary_measuring() {
    const GAP_EVERY_FRAMES: usize = FRAMES_PER_SECOND;
    const GAP_RECOVERY_FRAMES: usize = 12;
    const MEASURED_FROM: usize = 12 * FRAMES_PER_SECOND;
    const MEASURED_FRAMES: usize = 12 * FRAMES_PER_SECOND;
    const GAPS_END_FRAME: usize = 7 * FRAMES_PER_SECOND;
    let (low, high) = (0.97, 1.03);

    for music in [loud(), quiet()] {
        let play = |gaps_until: usize| {
            let mut processor = processor();
            let first = boundary_sample(0.0);
            (0..MEASURED_FROM + MEASURED_FRAMES)
                .map(|frame| {
                    let silent =
                        frame % GAP_EVERY_FRAMES == GAP_EVERY_FRAMES - 1 && frame < gaps_until;
                    let hop = if silent {
                        vec![0.0; HOP]
                    } else {
                        music.mono(first + frame * HOP, HOP)
                    };
                    let bars: Frame = processor.process(&hop).try_into().unwrap();
                    bars
                })
                .collect::<Vec<Frame>>()
        };
        let with_gaps = play(usize::MAX);
        let control = play(GAPS_END_FRAME);

        // Frames next to a gap differ because the window holds silence in one
        // run only; judge the rest.
        let level = |frames: &[Frame]| -> f32 {
            let kept: Vec<f32> = (MEASURED_FROM..MEASURED_FROM + MEASURED_FRAMES)
                .filter(|frame| {
                    frame % GAP_EVERY_FRAMES >= GAP_RECOVERY_FRAMES
                        && frame % GAP_EVERY_FRAMES < GAP_EVERY_FRAMES - 1
                })
                .map(|frame| super::boundary_fixture::frame_mean(&frames[frame]))
                .collect();
            kept.iter().sum::<f32>() / kept.len() as f32
        };
        let ratio = level(&with_gaps) / level(&control);
        assert!(
            (low..=high).contains(&ratio),
            "silent gaps left the level at {ratio:.3} times the run without them"
        );
    }
}

// A song that opens quietly and then drops in at full level: the gain is
// measured on the intro, so the first loud bar has to pull it down at once
// instead of pinning the bars for as long as 2 % steps take. The intros run up
// to six seconds, inside the seven that braking lasts; a longer one is a known
// gap that nothing here claims.
#[test]
fn ac_29_a_loud_body_after_a_quiet_intro_does_not_pin_the_bars() {
    const INTRO_SECONDS: [f32; 2] = [2.5, 6.0];
    const JUDGED_AFTER_THE_STEP: usize = 2 * FRAMES_PER_SECOND;
    const INTRO_STEPS_DB: [f32; 3] = [14.0, 20.0, 30.0];
    const PINNED_SLACK: usize = 40;

    let reference = Measure::of(&settled_reference(&loud(), 0.0)[..JUDGED_AFTER_THE_STEP]);
    for intro_seconds in INTRO_SECONDS {
        let intro_frames = (intro_seconds * FRAMES_PER_SECOND as f32) as usize;
        for step_db in INTRO_STEPS_DB {
            for previous in [Some(loud()), None] {
                let mut processor = processor();
                if let Some(previous) = previous {
                    warm(&mut processor, &previous, 0.0);
                    processor.reset_stream();
                }
                let intro = loud().louder_by(-step_db);
                let frames: Vec<Frame> = (0..intro_frames + JUDGED_AFTER_THE_STEP)
                    .map(|frame| {
                        let music = if frame < intro_frames {
                            &intro
                        } else {
                            &loud()
                        };
                        let bars = processor.process(&music.mono(frame * HOP, HOP));
                        bars.try_into().unwrap()
                    })
                    .collect();

                let after_the_step = Measure::of(&frames[intro_frames..]);

                assert_eq!(
                    after_the_step.wall_frames, 0,
                    "a {intro_seconds} s intro {step_db} dB down walled the drop: \
                     {after_the_step:?}"
                );
                assert!(
                    after_the_step.pinned_frames <= reference.pinned_frames + PINNED_SLACK,
                    "a {intro_seconds} s intro {step_db} dB down pinned {} frames after the \
                     drop against the reference's {}",
                    after_the_step.pinned_frames,
                    reference.pinned_frames
                );
            }
        }
    }
}

// A song whose first window under-reads it: the level rises by a third of a
// second in, as a quiet song's first hit does. The gain was measured from the
// first window, so the rise lands above full height unless braking is tighter
// while the evidence is thin. Control arm: a processor that has already settled
// on the song and is surprised by the same rise, which is all cavacore would do.
#[test]
fn ac_29_a_first_window_that_under_reads_the_song_does_not_pin_the_bars() {
    const RISE_AT_FRAME: usize = 18;
    const RISE: f32 = 1.6;
    const PINNED_SLACK: usize = 8;

    let with_a_rise = |processor: &mut CavaBarProcessor, first: usize| -> Vec<Frame> {
        (0..FRAMES_PER_SECOND)
            .map(|frame| {
                let gain = if frame < RISE_AT_FRAME { 1.0 } else { RISE };
                let pcm: Vec<f32> = loud()
                    .mono(first + frame * HOP, HOP)
                    .into_iter()
                    .map(|sample| (sample * gain).clamp(-1.0, 1.0))
                    .collect();
                processor.process(&pcm).try_into().unwrap()
            })
            .collect()
    };

    for offset in BOUNDARY_OFFSETS_SECONDS {
        let first = boundary_sample(offset);
        let measured = Measure::of(&with_a_rise(&mut processor(), first));
        let mut settled = processor();
        warm(&mut settled, &loud(), offset);
        let control = Measure::of(&with_a_rise(&mut settled, first));

        assert!(
            measured.pinned_frames <= control.pinned_frames + PINNED_SLACK,
            "a rise at +{offset} pinned {} frames in the first second against the settled \
             processor's {}",
            measured.pinned_frames,
            control.pinned_frames
        );
        assert_eq!(measured.wall_frames, 0);
    }
}

/// Plays `music` from the boundary for `seconds` and returns the gain after
/// every frame.
fn gains_while_playing(
    processor: &mut CavaBarProcessor,
    music: &SyntheticMusic,
    seconds: usize,
) -> Vec<f32> {
    (0..seconds * FRAMES_PER_SECOND)
        .map(|frame| {
            processor.process(&music.mono(boundary_sample(0.0) + frame * HOP, HOP));
            processor.sensitivity()
        })
        .collect()
}

// Silence is not evidence that the song is quiet: the gain a processor has
// reached stays put through ten seconds of nothing, settled, still braking a
// fresh start, and still braking after a track change.
#[test]
fn ac_29_silence_does_not_raise_the_sensitivity() {
    const SILENT_FRAMES: usize = 10 * FRAMES_PER_SECOND;
    const STILL_BRAKING_SECONDS: usize = 3;
    let silence = vec![0.0; HOP];

    let settled = || {
        let mut processor = processor();
        warm(&mut processor, &loud(), 0.0);
        processor
    };
    let braking_a_fresh_start = || {
        let mut processor = processor();
        gains_while_playing(&mut processor, &loud(), STILL_BRAKING_SECONDS);
        processor
    };
    let braking_a_track_change = || {
        let mut processor = settled();
        processor.reset_stream();
        gains_while_playing(&mut processor, &quiet(), STILL_BRAKING_SECONDS);
        processor
    };
    for (label, mut processor) in [
        ("settled", settled()),
        ("braking a fresh start", braking_a_fresh_start()),
        ("braking a track change", braking_a_track_change()),
    ] {
        let before = processor.sensitivity();
        for _ in 0..SILENT_FRAMES {
            processor.process(&silence);
        }
        assert_eq!(
            processor.sensitivity(),
            before,
            "{label}: ten seconds of silence moved the gain"
        );
    }
}

// The braking span ends about seven seconds in, by the clock of the audio. The
// hand-over to the creep must not move the gain: the stretch around it steps no
// further than the creep itself does.
#[test]
fn ac_29_the_gain_does_not_jump_when_the_braking_span_ends() {
    const FROM_SECOND: usize = 6;
    const TO_SECOND: usize = 8;
    const LARGEST_STEP_DOWN: f32 = 0.97;
    const LARGEST_STEP_UP: f32 = 1.002;

    for music in [loud(), quiet()] {
        let gains = gains_while_playing(&mut processor(), &music, TO_SECOND + 1);
        for frame in FROM_SECOND * FRAMES_PER_SECOND..TO_SECOND * FRAMES_PER_SECOND {
            let step = gains[frame] / gains[frame - 1];
            assert!(
                (LARGEST_STEP_DOWN..=LARGEST_STEP_UP).contains(&step),
                "the gain stepped by {step:.4} at frame {frame}"
            );
        }
    }
}
