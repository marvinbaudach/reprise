//! AC-29 at the core seam: music that falls to digital silence again and again,
//! as hard-gated electronic music and chiptune do, whether it starts a processor
//! or follows another song.
//!
//! A silent hop restarts the boundary measurement, so a gap that recurs before a
//! window of signal has gathered would keep it restarting for as long as the
//! pattern lasts: the gain is then set from every frame's own tallest bar, or,
//! after a track change, never leaves the gain the last song left. The tests
//! judge the tallest bar a viewer sees once the braking span is long over,
//! against the same music without the gaps.

use super::boundary_fixture::{
    settled_seconds, spread_complaints, Frame, Spread, SyntheticMusic, FRAMES_PER_SECOND,
    SPREAD_FRAMES,
};
use super::{CavaBarProcessor, CavaConfig, SPECTRUM_BAND_COUNT};

const RATE_HZ: u32 = 44_100;
const HOP: usize = 735;
const LOUD_GAIN: f32 = 0.9;
/// Hops into the song the run starts at, so that the song that was playing
/// before a track change has audio to play.
const SONG_START_HOP: usize = 60 * FRAMES_PER_SECOND;
/// Seconds the run lasts: the stretch the spread is judged over ends at 30.
const RUN_SECONDS: usize = 30;
/// A gap every this many hops: one silent hop in each. The window is 8192
/// samples, a little over eleven hops at 44.1 kHz, so every gap here comes
/// before a window of signal has gathered; the last comes just short of it.
const HOPS_BETWEEN_GAPS: [usize; 3] = [5, 10, 11];

fn processor() -> CavaBarProcessor {
    CavaBarProcessor::new(CavaConfig::new(RATE_HZ, SPECTRUM_BAND_COUNT)).unwrap()
}

fn loud() -> SyntheticMusic {
    SyntheticMusic::new(1, RATE_HZ, LOUD_GAIN)
}

/// What the processor is doing when the music begins.
#[derive(Debug, Clone, Copy)]
enum Start {
    /// A new processor: nothing is on screen.
    Fresh,
    /// A song `previous_db` dB louder than this one was playing and the stream
    /// changed. A gain the last song left that is not this song's, by less than
    /// the factor of two a boundary replaces it at, is what the creep has to
    /// correct and a frozen measurement cannot.
    TrackChange { previous_db: f32 },
}

/// The frames `song` draws over its first `RUN_SECONDS`.
fn run(start: Start, song: &SyntheticMusic) -> Vec<Frame> {
    let mut processor = processor();
    if let Start::TrackChange { previous_db } = start {
        let previous = loud().louder_by(previous_db);
        let warm_hops = settled_seconds(RATE_HZ) * FRAMES_PER_SECOND;
        for hop in 0..warm_hops {
            processor.process(&previous.mono((SONG_START_HOP - warm_hops + hop) * HOP, HOP));
        }
        processor.reset_stream();
    }
    (0..RUN_SECONDS * FRAMES_PER_SECOND)
        .map(|hop| {
            processor
                .process(&song.mono((SONG_START_HOP + hop) * HOP, HOP))
                .try_into()
                .unwrap()
        })
        .collect()
}

// Seconds 15 to 30 are the creep's alone, so the spread of the tallest bar there
// is the song's dynamics. It has to be the continuous run's, not a flat line.
#[test]
fn ac_29_a_gap_that_recurs_before_the_window_fills_does_not_flatten_the_tallest_bar() {
    let mut complaints = Vec::new();
    for start in [
        Start::Fresh,
        Start::TrackChange { previous_db: -4.0 },
        Start::TrackChange { previous_db: 4.0 },
    ] {
        let continuous = Spread::of_tallest_bars(&run(start, &loud())[SPREAD_FRAMES]);
        for hops in HOPS_BETWEEN_GAPS {
            let gated = loud().gated(hops * HOP, HOP);
            let measured = Spread::of_tallest_bars(&run(start, &gated)[SPREAD_FRAMES]);
            complaints.extend(spread_complaints(
                &format!("a gap every {hops} hops, {start:?}"),
                measured,
                continuous,
            ));
        }
    }
    assert!(complaints.is_empty(), "{complaints:#?}");
}
