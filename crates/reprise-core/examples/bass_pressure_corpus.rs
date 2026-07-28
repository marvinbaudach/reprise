//! Replays raw PCM through the real [`BassPressureDetector`] and reports what
//! the visualizer's glow layer would do, so its calibration can be re-checked
//! against actual songs instead of guessed.
//!
//! The core crate stays dependency-pure, so this reads decoded samples rather
//! than audio files. Produce them with GStreamer or ffmpeg, for example:
//!
//! ```text
//! gst-launch-1.0 -q filesrc location="track.flac" ! decodebin ! audioconvert \
//!   ! audioresample ! audio/x-raw,format=F32LE,channels=1,rate=44100 \
//!   ! filesink location=track.raw
//! cargo run -p reprise-core --example bass_pressure_corpus -- track.raw
//! ```

use std::fs;

use reprise_core::playback::BassPressureDetector;

const RATE: u32 = 44_100;
/// PCM chunk size, matching what the player's audio sink delivers.
const CHUNK: usize = 1_024;
/// Readings are bucketed into segments of this length for a coarse profile.
const SEGMENT_S: f32 = 5.0;

struct Segment {
    level_dbfs: Vec<f32>,
    impact: Vec<f32>,
    aura: Vec<f32>,
}

impl Segment {
    fn new() -> Self {
        Self {
            level_dbfs: Vec::new(),
            impact: Vec::new(),
            aura: Vec::new(),
        }
    }
}

fn percentile(values: &mut [f32], percent: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f32::total_cmp);
    let index = ((values.len() - 1) as f32 * percent / 100.0).round() as usize;
    values[index]
}

fn main() {
    let mut arguments = std::env::args().skip(1).peekable();
    if arguments.peek().is_none() {
        eprintln!("usage: bass_pressure_corpus <raw-mono-f32-44100.raw>...");
        std::process::exit(2);
    }

    for path in arguments {
        let Ok(bytes) = fs::read(&path) else {
            eprintln!("cannot read {path}");
            continue;
        };
        let samples: Vec<f32> = bytes
            .chunks_exact(size_of::<f32>())
            .map(|bytes| f32::from_le_bytes(bytes.try_into().expect("four-byte PCM chunk")))
            .collect();

        let mut detector = BassPressureDetector::new(RATE);
        let mut segments: Vec<Segment> = Vec::new();
        for (index, chunk) in samples.chunks(CHUNK).enumerate() {
            let reading = detector.observe(chunk);
            let seconds = (index * CHUNK) as f32 / RATE as f32;
            let segment_index = (seconds / SEGMENT_S) as usize;
            while segments.len() <= segment_index {
                segments.push(Segment::new());
            }
            let segment = &mut segments[segment_index];
            segment.level_dbfs.push(reading.level_dbfs);
            segment.impact.push(reading.impact);
            segment.aura.push(reading.aura);
        }

        println!(
            "\n=== {path} — {:.0} s ===",
            samples.len() as f32 / RATE as f32
        );
        println!("   time   bass p50   impact p50/p95   aura p95");
        for (index, segment) in segments.iter_mut().enumerate() {
            println!(
                "  {:4.0}s     {:+7.1}      {:5.2} {:5.2}       {:5.2}",
                index as f32 * SEGMENT_S,
                percentile(&mut segment.level_dbfs, 50.0),
                percentile(&mut segment.impact, 50.0),
                percentile(&mut segment.impact, 95.0),
                percentile(&mut segment.aura, 95.0),
            );
        }
    }
}
