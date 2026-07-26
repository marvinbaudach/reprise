use std::path::Path;

use gstreamer as gst;
use gstreamer::prelude::*;
use reprise_core::playback::{
    SpectrumAnalyzer, SPECTRUM_ANALYSIS_BAND_COUNT, SPECTRUM_INTERVAL_MS,
};
use reprise_core::visuals::{Geom, VisualEngine};

const PIPELINE_DESCRIPTION: &str = "uridecodebin name=decoder ! audioconvert ! \
    spectrum name=analyzer bands=256 threshold=-80 interval=16000000 \
    message-phase=false post-messages=true ! fakesink sync=false";

#[derive(Clone, Copy)]
struct Sample {
    time_s: f32,
    bass_energy: f32,
    bass: f32,
    level: f32,
    beat: f32,
    visible_mean: f32,
    visible_max: f32,
}

fn decibels(structure: &gst::StructureRef) -> Option<[f32; SPECTRUM_ANALYSIS_BAND_COUNT]> {
    if structure.name() != "spectrum" {
        return None;
    }
    let magnitudes = structure.get::<gst::List>("magnitude").ok()?;
    if magnitudes.len() != SPECTRUM_ANALYSIS_BAND_COUNT {
        return None;
    }
    let mut values = [0.0; SPECTRUM_ANALYSIS_BAND_COUNT];
    for (slot, magnitude) in values.iter_mut().zip(magnitudes.iter()) {
        *slot = magnitude.get::<f32>().ok()?;
    }
    Some(values)
}

fn visible_heights(engine: &VisualEngine) -> (f32, f32) {
    const WIDTH: f32 = 548.0;
    const HEIGHT: f32 = 300.0;
    const BASELINE: f32 = HEIGHT * 0.82;
    const SEGMENTS: f32 = 16.0;

    let mut heights = Vec::new();
    for shape in engine.scene(WIDTH, HEIGHT).shapes {
        let Geom::Rect { x, y, h, .. } = shape.geom else {
            continue;
        };
        if y >= BASELINE || h <= 3.0 {
            continue;
        }
        let segment = ((BASELINE - y) / (h + 2.5)).round() / SEGMENTS;
        if let Some((_, height)) = heights
            .iter_mut()
            .find(|(bar_x, _): &&mut (f32, f32)| (*bar_x - x).abs() < 0.01)
        {
            *height = height.max(segment);
        } else {
            heights.push((x, segment));
        }
    }
    let mean = heights.iter().map(|(_, height)| height).sum::<f32>() / 20.0;
    let max = heights
        .iter()
        .map(|(_, height)| *height)
        .fold(0.0_f32, f32::max);
    (mean, max)
}

fn linear_amplitude(db: f32) -> f32 {
    10.0_f32.powf(db.clamp(-80.0, 0.0) / 20.0)
}

fn quantile(values: &[f32], fraction: f32) -> f32 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f32::total_cmp);
    sorted[((sorted.len() - 1) as f32 * fraction).round() as usize]
}

fn correlation(samples: &[Sample], lag: isize) -> f32 {
    let (start_visual, start_bass, len) = if lag >= 0 {
        (lag as usize, 0, samples.len() - lag as usize)
    } else {
        (0, (-lag) as usize, samples.len() - (-lag) as usize)
    };
    let visual = &samples[start_visual..start_visual + len];
    let bass = &samples[start_bass..start_bass + len];
    let visual_mean = visual.iter().map(|sample| sample.visible_mean).sum::<f32>() / len as f32;
    let bass_mean = bass
        .iter()
        .map(|sample| sample.bass_energy.log10())
        .sum::<f32>()
        / len as f32;
    let mut covariance = 0.0;
    let mut visual_variance = 0.0;
    let mut bass_variance = 0.0;
    for (visual, bass) in visual.iter().zip(bass) {
        let visual_delta = visual.visible_mean - visual_mean;
        let bass_delta = bass.bass_energy.log10() - bass_mean;
        covariance += visual_delta * bass_delta;
        visual_variance += visual_delta * visual_delta;
        bass_variance += bass_delta * bass_delta;
    }
    covariance / (visual_variance * bass_variance).sqrt().max(f32::EPSILON)
}

fn profile(path: &Path) -> Vec<Sample> {
    gst::init().expect("GStreamer");
    let pipeline = gst::parse::launch(PIPELINE_DESCRIPTION)
        .expect("pipeline")
        .downcast::<gst::Pipeline>()
        .expect("parsed pipeline");
    let uri = gst::glib::filename_to_uri(path, None).expect("track URI");
    pipeline
        .by_name("decoder")
        .expect("decoder")
        .set_property("uri", uri.to_string());
    pipeline.set_state(gst::State::Playing).expect("play");
    let bus = pipeline.bus().expect("bus");
    let mut analyzer = SpectrumAnalyzer::new();
    let mut engine = VisualEngine::new();
    engine.set_playing(true);
    let mut samples = Vec::new();

    loop {
        let message = bus
            .timed_pop(gst::ClockTime::NONE)
            .expect("pipeline message");
        match message.view() {
            gst::MessageView::Element(element) => {
                let Some(structure) = element.structure() else {
                    continue;
                };
                let Some(raw) = decibels(structure) else {
                    continue;
                };
                let time_s = structure
                    .get::<u64>("stream-time")
                    .unwrap_or(samples.len() as u64 * SPECTRUM_INTERVAL_MS * 1_000_000)
                    as f32
                    / 1_000_000_000.0;
                let bass_energy =
                    raw[..8].iter().map(|db| linear_amplitude(*db)).sum::<f32>() / 8.0;
                let frame = analyzer.ingest(raw);
                engine.ingest(&frame);
                engine.tick();
                let (visible_mean, visible_max) = visible_heights(&engine);
                samples.push(Sample {
                    time_s,
                    bass_energy: bass_energy.max(1.0e-6),
                    bass: frame.bass(),
                    level: frame.level(),
                    beat: frame.beat().strength,
                    visible_mean,
                    visible_max,
                });
            }
            gst::MessageView::Eos(_) => break,
            gst::MessageView::Error(error) => {
                panic!("decode failed: {} ({:?})", error.error(), error.debug())
            }
            _ => {}
        }
    }
    pipeline.set_state(gst::State::Null).expect("stop");
    samples
}

fn main() {
    let path = std::env::args().nth(1).expect("track path");
    let samples = profile(Path::new(&path));
    let bass = samples
        .iter()
        .map(|sample| sample.bass_energy)
        .collect::<Vec<_>>();
    let bass_p50 = quantile(&bass, 0.50);
    let bass_p95 = quantile(&bass, 0.95);
    let derived_bass = samples.iter().map(|sample| sample.bass).collect::<Vec<_>>();
    let levels = samples
        .iter()
        .map(|sample| sample.level)
        .collect::<Vec<_>>();
    let beats = samples.iter().map(|sample| sample.beat).collect::<Vec<_>>();
    let quiet = samples
        .iter()
        .filter(|sample| sample.bass_energy <= bass_p50)
        .collect::<Vec<_>>();
    let hits = samples
        .iter()
        .filter(|sample| sample.bass_energy >= bass_p95)
        .collect::<Vec<_>>();
    let quiet_full = quiet
        .iter()
        .filter(|sample| sample.visible_mean >= 0.75)
        .count();
    let hit_full = hits
        .iter()
        .filter(|sample| sample.visible_mean >= 0.75)
        .count();
    let quiet_mean =
        quiet.iter().map(|sample| sample.visible_mean).sum::<f32>() / quiet.len() as f32;
    let hit_mean = hits.iter().map(|sample| sample.visible_mean).sum::<f32>() / hits.len() as f32;
    let (best_lag, best_correlation) = (-12..=12)
        .map(|lag| (lag, correlation(&samples, lag)))
        .max_by(|left, right| left.1.total_cmp(&right.1))
        .expect("lag candidates");

    println!("track={path}");
    println!(
        "frames={} duration={:.3}s bass_p50={bass_p50:.6} bass_p95={bass_p95:.6}",
        samples.len(),
        samples.last().map_or(0.0, |sample| sample.time_s)
    );
    println!(
        "derived_bass_p50={:.3} p95={:.3} level_p50={:.3} p95={:.3} beat_p95={:.3} p99={:.3}",
        quantile(&derived_bass, 0.50),
        quantile(&derived_bass, 0.95),
        quantile(&levels, 0.50),
        quantile(&levels, 0.95),
        quantile(&beats, 0.95),
        quantile(&beats, 0.99),
    );
    println!(
        "quiet_mean={quiet_mean:.3} quiet_full_rate={:.3}",
        quiet_full as f32 / quiet.len() as f32
    );
    println!(
        "hit_mean={hit_mean:.3} hit_full_rate={:.3}",
        hit_full as f32 / hits.len() as f32
    );
    println!(
        "best_lag_ms={} correlation={best_correlation:.3}",
        best_lag * SPECTRUM_INTERVAL_MS as isize
    );
    let mut strongest = samples.iter().collect::<Vec<_>>();
    strongest.sort_by(|left, right| right.bass_energy.total_cmp(&left.bass_energy));
    let mut selected_times = Vec::new();
    for sample in strongest {
        if selected_times
            .iter()
            .all(|time: &f32| (*time - sample.time_s).abs() >= 2.0)
        {
            println!(
                "bass_event={:.3}s energy={:.6} visible_mean={:.3} visible_max={:.3}",
                sample.time_s, sample.bass_energy, sample.visible_mean, sample.visible_max
            );
            selected_times.push(sample.time_s);
            if selected_times.len() == 5 {
                break;
            }
        }
    }
    for (start, end) in [(11.5, 13.0), (15.5, 17.5), (35.0, 37.5)] {
        let window = samples
            .iter()
            .filter(|sample| sample.time_s >= start && sample.time_s < end)
            .collect::<Vec<_>>();
        if window.is_empty() {
            continue;
        }
        let mean =
            window.iter().map(|sample| sample.visible_mean).sum::<f32>() / window.len() as f32;
        let mean_max = window
            .iter()
            .map(|sample| sample.visible_mean)
            .fold(0.0_f32, f32::max);
        let bar_max = window
            .iter()
            .map(|sample| sample.visible_max)
            .fold(0.0_f32, f32::max);
        let bass_max = window
            .iter()
            .map(|sample| sample.bass)
            .fold(0.0_f32, f32::max);
        let level_max = window
            .iter()
            .map(|sample| sample.level)
            .fold(0.0_f32, f32::max);
        let beat_max = window
            .iter()
            .map(|sample| sample.beat)
            .fold(0.0_f32, f32::max);
        println!(
            "window={start:.1}-{end:.1}s visible_mean={mean:.3} mean_max={mean_max:.3} \
             bar_max={bar_max:.3} \
             bass_max={bass_max:.3} level_max={level_max:.3} beat_max={beat_max:.3}"
        );
    }
}
