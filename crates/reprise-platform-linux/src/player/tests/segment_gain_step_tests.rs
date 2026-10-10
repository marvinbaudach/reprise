//! The next CUE track's gain applies from the exact sample its cut falls on
//! (PLAY-23a). The boundary probe sees whole buffers, so a boundary inside a
//! buffer used to leave the buffer's tail at the previous track's gain; the
//! file here holds a constant level, so the gain each frame left the gain
//! element with can be read straight off its value.

use std::path::Path;

use super::segment_support::{count, cue_item, gain_element, linear, slow_sink, Harness};
use super::*;

/// Generous: under a loaded parallel test run the pipeline can take a while.
const HANG_GUARD: Duration = Duration::from_secs(25);
const SAMPLE_RATE: u32 = 44_100;
/// The constant level of the file, in 16-bit steps.
const LEVEL: i16 = 8_000;
const FULL_SCALE: f64 = 32_768.0;
/// Whole file length. The last track ends within a second of it, so it plays
/// to the file's end and has no boundary of its own.
const FILE_MS: u32 = 9_000;
/// The boundaries fall between two frames, one nearer the frame before it
/// (3003 ms is frame 132432.3) and one nearer the frame after (6007 ms is
/// 264908.7), so the step must land on the nearest frame either way.
const FIRST: (i64, i64) = (1_000, 3_003);
const SECOND: (i64, i64) = (3_003, 6_007);
const THIRD: (i64, i64) = (6_007, 8_500);
const GAINS_DB: [f64; 3] = [-6.0, 6.0, -3.0];
/// A frame within this of a gain's level carries that gain: a few 16-bit steps
/// of the converters' rounding, far less than the gap between two gains.
const LEVEL_TOLERANCE: f64 = 4.0 / FULL_SCALE;
/// How many frames around a boundary are judged.
const WINDOW_FRAMES: i64 = 3_000;

fn finished(event: &PlayerEvent) -> bool {
    matches!(event, PlayerEvent::TrackFinished)
}

fn advanced(event: &PlayerEvent) -> bool {
    matches!(event, PlayerEvent::AdvancedToNext)
}

/// Writes a 16-bit WAV of `FILE_MS` milliseconds at a constant level.
fn write_constant_wav(path: &Path, channels: u16) {
    let frames = u64::from(SAMPLE_RATE) * u64::from(FILE_MS) / 1000;
    let data_size = u32::try_from(frames * 2 * u64::from(channels)).unwrap();
    let mut wav = Vec::with_capacity(44 + data_size as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_size).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(SAMPLE_RATE * 2 * u32::from(channels)).to_le_bytes());
    wav.extend_from_slice(&(2 * channels).to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    for _ in 0..frames * u64::from(channels) {
        wav.extend_from_slice(&LEVEL.to_le_bytes());
    }
    std::fs::write(path, wav).unwrap();
}

/// One buffer as it leaves the gain element: the file frame its first sample
/// is, and its first channel's samples as fractions of full scale.
struct FrameRun {
    first_frame: i64,
    samples: Vec<f64>,
}

fn decode(bytes: &[u8], format: &str) -> Vec<f64> {
    match format {
        "S16LE" => bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|chunk| f64::from(i16::from_le_bytes(*chunk)) / FULL_SCALE)
            .collect(),
        "S32LE" => bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|chunk| f64::from(i32::from_le_bytes(*chunk)) / f64::from(i32::MAX))
            .collect(),
        "F32LE" => bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|chunk| f64::from(f32::from_le_bytes(*chunk)))
            .collect(),
        "F64LE" => bytes
            .as_chunks::<8>()
            .0
            .iter()
            .map(|chunk| f64::from_le_bytes(*chunk))
            .collect(),
        other => panic!("the gain element negotiated a format this test cannot read: {other}"),
    }
}

/// Records every sample leaving the gain element with its frame in the file,
/// dropping what a flushing seek's `SEGMENT` makes stale.
fn record_frames(player: &Player, channels: usize) -> Arc<Mutex<Vec<FrameRun>>> {
    let runs = Arc::new(Mutex::new(Vec::new()));
    let recorded = runs.clone();
    let stream_segment = Mutex::new(None::<gst::FormattedSegment<gst::ClockTime>>);
    gain_element(player).static_pad("src").unwrap().add_probe(
        gst::PadProbeType::BUFFER | gst::PadProbeType::EVENT_DOWNSTREAM,
        move |pad, info| {
            match &info.data {
                Some(gst::PadProbeData::Event(event)) => {
                    if let gst::EventView::Segment(event) = event.view() {
                        *stream_segment
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner) =
                            event.segment().downcast_ref::<gst::ClockTime>().cloned();
                        recorded
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .clear();
                    }
                }
                Some(gst::PadProbeData::Buffer(buffer)) => {
                    let stream_time = buffer.pts().and_then(|pts| {
                        stream_segment
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .as_ref()
                            .and_then(|segment| segment.to_stream_time(pts))
                    });
                    let format = pad.current_caps().and_then(|caps| {
                        caps.structure(0)
                            .and_then(|structure| structure.get::<String>("format").ok())
                    });
                    if let (Some(stream_time), Some(format), Ok(map)) =
                        (stream_time, format, buffer.map_readable())
                    {
                        let first_frame = (stream_time.nseconds() as f64 * f64::from(SAMPLE_RATE)
                            / 1e9)
                            .round() as i64;
                        recorded
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .push(FrameRun {
                                first_frame,
                                samples: decode(map.as_slice(), &format)
                                    .into_iter()
                                    .step_by(channels)
                                    .collect(),
                            });
                    }
                }
                _ => {}
            }
            gst::PadProbeReturn::Ok
        },
    );
    runs
}

fn boundary_frame(boundary_ms: i64) -> i64 {
    (boundary_ms * i64::from(SAMPLE_RATE) + 500) / 1000
}

/// Which gain a frame carries: `Some(index)` into `GAINS_DB`, or `None` when
/// it matches none of them.
fn gain_of(sample: f64) -> Option<usize> {
    let input = f64::from(LEVEL) / FULL_SCALE;
    GAINS_DB
        .iter()
        .position(|&db| (sample - input * linear(db)).abs() <= LEVEL_TOLERANCE)
}

/// For the boundary at `boundary_ms` between `GAINS_DB[outgoing]` and the next:
/// the frame offset from the boundary at which the next gain first applies, and
/// the frames in the judged window that carry a gain other than their side's.
fn gain_step(runs: &[FrameRun], boundary_ms: i64, outgoing: usize) -> (i64, usize) {
    let boundary = boundary_frame(boundary_ms);
    // The decoder cuts the file into buffers of one size from the first. A
    // boundary off that grid lies inside one of them as the probe receives it.
    let buffer_frames = runs
        .iter()
        .map(|run| run.samples.len() as i64)
        .max()
        .unwrap();
    let grid_start = runs.first().unwrap().first_frame;
    assert_ne!(
        (boundary - grid_start) % buffer_frames,
        0,
        "the {boundary_ms} ms boundary must fall inside a buffer, or this test proves nothing"
    );
    let mut first_new = None;
    let mut wrong = 0;
    for run in runs {
        for (index, &sample) in run.samples.iter().enumerate() {
            let frame = run.first_frame + index as i64;
            if (frame - boundary).abs() > WINDOW_FRAMES {
                continue;
            }
            let expected = if frame < boundary {
                outgoing
            } else {
                outgoing + 1
            };
            let carried = gain_of(sample);
            if carried != Some(expected) {
                wrong += 1;
            }
            if carried == Some(outgoing + 1) && first_new.is_none_or(|first| frame < first) {
                first_new = Some(frame);
            }
        }
    }
    let first_new =
        first_new.unwrap_or_else(|| panic!("no frame at {boundary_ms} ms carried the next gain"));
    (first_new - boundary, wrong)
}

/// Plays three contiguous CUE tracks of a `channels`-channel file and judges
/// the gain step at both boundaries.
fn assert_exact_gain_steps(channels: u16) {
    let harness = Harness::new();
    let rendered = slow_sink(&harness.player);
    drop(rendered);
    let directory = tempfile::tempdir().unwrap();
    let album = directory.path().join("album.wav");
    write_constant_wav(&album, channels);
    let runs = record_frames(&harness.player, usize::from(channels));

    harness.start(|| {
        harness
            .player
            .play(cue_item(&album, FIRST, GAINS_DB[0]))
            .unwrap();
        harness
            .player
            .set_next(Some(cue_item(&album, SECOND, GAINS_DB[1])));
    });
    let mut events = harness.pump_until(HANG_GUARD, |events| count(events, advanced) > 0);
    harness
        .player
        .set_next(Some(cue_item(&album, THIRD, GAINS_DB[2])));
    events.extend(harness.pump_until(HANG_GUARD, |events| count(events, finished) > 0));
    assert_eq!(
        count(&events, advanced),
        2,
        "both boundaries hand over: {events:?}"
    );

    let runs = runs.lock().unwrap_or_else(PoisonError::into_inner);
    let steps = [
        (FIRST.1, gain_step(&runs, FIRST.1, 0)),
        (SECOND.1, gain_step(&runs, SECOND.1, 1)),
    ];
    for (boundary_ms, (offset, wrong)) in steps {
        eprintln!(
            "gain step at {boundary_ms} ms: {offset} frames after the boundary, {wrong} frames off"
        );
    }
    for (boundary_ms, (offset, wrong)) in steps {
        assert_eq!(
            (offset, wrong),
            (0, 0),
            "the gain must step at the exact frame of the {boundary_ms} ms boundary"
        );
    }
}

#[test]
fn play_23a_the_next_tracks_gain_applies_from_the_exact_sample_of_each_boundary() {
    assert_exact_gain_steps(1);
}

#[test]
fn play_23a_the_exact_sample_holds_for_a_stereo_file() {
    assert_exact_gain_steps(2);
}
