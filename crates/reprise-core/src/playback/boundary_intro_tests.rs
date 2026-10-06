//! AC-29 at the core seam: a song that opens far below its body, either as a
//! quiet intro or as a fade-in, whether it starts a processor or follows another
//! song.
//!
//! The gain is measured from the opening, so the body arrives at a gain that
//! fitted something much quieter. The tests judge the bars the viewer sees in
//! the stretch where the body (or the end of the fade) lands.

use super::boundary_fixture::{
    dimming_complaints, pinning_complaints, settled_seconds, Frame, Measure, Opening,
    SyntheticMusic, FADE_LEVEL_FLOOR, FRAMES_PER_SECOND, INTRO_LEVEL_FLOOR,
};
use super::{CavaBarProcessor, CavaConfig, SPECTRUM_BAND_COUNT};

const RATE_HZ: u32 = 44_100;
const HOP: usize = 735;
const LOUD_GAIN: f32 = 0.9;
/// Seconds the previous song plays before a track change, and the reference
/// plays before it is compared: long enough for the braking span of its own
/// first boundary to be over.
fn warm_seconds() -> usize {
    settled_seconds(RATE_HZ)
}
/// Seconds of the body judged after a quiet intro ends.
const JUDGED_BODY_SECONDS: usize = 3;
/// Seconds judged from the start of a fade-in.
const JUDGED_FADE_SECONDS: usize = 10;
/// How many frames more than a settled engine may touch full height.
const INTRO_PINNED_SLACK: usize = 4;
const FADE_PINNED_SLACK: usize = 6;

fn processor() -> CavaBarProcessor {
    CavaBarProcessor::new(CavaConfig::new(RATE_HZ, SPECTRUM_BAND_COUNT)).unwrap()
}

fn loud() -> SyntheticMusic {
    SyntheticMusic::new(1, RATE_HZ, LOUD_GAIN)
}

/// Hops into the loud song at which the opening starts, so a song that was
/// playing before it has audio to play.
const SONG_START_FRAME: usize = 60 * FRAMES_PER_SECOND;

/// One hop of the song `opening` opens, `frame` hops in.
fn hop(opening: Opening, frame: usize) -> Vec<f32> {
    loud()
        .opened_by(opening, SONG_START_FRAME * HOP)
        .mono((SONG_START_FRAME + frame) * HOP, HOP)
}

/// The loud song itself, with no opening.
const BODY: Opening = Opening::Intro {
    seconds: 0,
    db: 0.0,
};

/// What the processor is doing when the song begins.
#[derive(Debug, Clone, Copy)]
enum Start {
    /// A new processor: nothing is on screen.
    Fresh,
    /// A loud song was playing and the stream changed.
    TrackChange,
}

fn feed(processor: &mut CavaBarProcessor, opening: Opening, frames: usize) -> Vec<Frame> {
    (0..frames)
        .map(|frame| processor.process(&hop(opening, frame)).try_into().unwrap())
        .collect()
}

/// A processor that has played the loud song for long enough to settle on it,
/// up to the moment `opening` starts.
fn settled_on_the_body() -> CavaBarProcessor {
    let mut processor = processor();
    let warm_frames = warm_seconds() * FRAMES_PER_SECOND;
    for frame in 0..warm_frames {
        processor.process(&loud().mono((SONG_START_FRAME - warm_frames + frame) * HOP, HOP));
    }
    processor
}

/// The frames the song `opening` draws from `start`.
fn run(start: Start, opening: Opening, frames: usize) -> Vec<Frame> {
    let mut processor = match start {
        Start::Fresh => processor(),
        Start::TrackChange => {
            let mut processor = settled_on_the_body();
            processor.reset_stream();
            processor
        }
    };
    feed(&mut processor, opening, frames)
}

// A long, quiet opening, then the song at full level. The gain was measured on
// the opening, so the body has to be caught however late it comes: the intro
// outlasts the span in which the loudest frames were first checked. The
// reference is a processor settled on the body, which is what the viewer would
// see had the intro not been there.
#[test]
fn ac_29_a_loud_body_after_a_long_quiet_intro_does_not_pin_the_bars() {
    let judged = JUDGED_BODY_SECONDS * FRAMES_PER_SECOND;
    let mut complaints = Vec::new();
    for seconds in [8, 10] {
        for db in [14.0, 30.0] {
            let opening = Opening::Intro { seconds, db };
            let first = seconds * FRAMES_PER_SECOND;
            let mut settled = settled_on_the_body();
            let reference: Vec<Frame> = (first..first + judged)
                .map(|frame| settled.process(&hop(BODY, frame)).try_into().unwrap())
                .collect();
            let reference = Measure::of(&reference);
            let allowed = reference.pinned_frames + INTRO_PINNED_SLACK;
            for start in [Start::Fresh, Start::TrackChange] {
                let label = format!("a {seconds} s intro {db} dB down, {start:?}");
                let frames = run(start, opening, first + judged);
                complaints.extend(pinning_complaints(
                    &format!("{label}, whole run"),
                    Measure::of(&frames),
                    usize::MAX,
                ));
                complaints.extend(pinning_complaints(
                    &format!("{label}, {JUDGED_BODY_SECONDS} s after the drop"),
                    Measure::of(&frames[first..]),
                    allowed,
                ));
                complaints.extend(dimming_complaints(
                    &format!("{label}, {JUDGED_BODY_SECONDS} s after the drop"),
                    Measure::of(&frames[first..]),
                    reference,
                    INTRO_LEVEL_FLOOR,
                ));
            }
        }
    }
    assert!(complaints.is_empty(), "{complaints:#?}");
}

// A song that rises out of silence. The reference is the processor that has
// been playing the loud song and keeps going into the fade: cavacore itself.
#[test]
fn ac_29_a_fade_in_does_not_pin_the_bars() {
    let judged = JUDGED_FADE_SECONDS * FRAMES_PER_SECOND;
    let mut complaints = Vec::new();
    for seconds in [3, 5] {
        for opening in [Opening::LinearFade { seconds }, Opening::DbFade { seconds }] {
            let reference = Measure::of(&feed(&mut settled_on_the_body(), opening, judged));
            let allowed = reference.pinned_frames + FADE_PINNED_SLACK;
            for start in [Start::Fresh, Start::TrackChange] {
                let label = format!("a {seconds} s fade ({opening:?}), {start:?}");
                let frames = Measure::of(&run(start, opening, judged));
                complaints.extend(pinning_complaints(&label, frames, allowed));
                complaints.extend(dimming_complaints(
                    &label,
                    frames,
                    reference,
                    FADE_LEVEL_FLOOR,
                ));
            }
        }
    }
    assert!(complaints.is_empty(), "{complaints:#?}");
}
