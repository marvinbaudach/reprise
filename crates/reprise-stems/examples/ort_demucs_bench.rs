//! Package E SPIKE — ort/ONNX HTDemucs timing harness (candidate b, Demucs-class).
//!
//! Same runtime (`ort` / ONNX Runtime) as `ort_mdx_bench`, but running a real
//! Hybrid-Transformer Demucs (htdemucs) ONNX export. This is the model FAMILY
//! the plan actually requires ("Demucs-Klasse-Qualität ist die
//! Einschlussbedingung") and whose weights are cleanly MIT (Meta). It proves
//! Demucs runs under ort WITHOUT any hand-port into candle.
//!
//! htdemucs is waveform-in / waveform-out (its STFT is inside the graph). The
//! harness probes the input shape at runtime and adapts: channels = shape[1],
//! segment length = shape[2] (falls back to 7.8 s if the axis is dynamic).
//! Quality is NOT judged — synthesised audio, TIMING only.
//!
//! Run:
//!   cargo run --release --features spike-ort --example ort_demucs_bench -- <htdemucs.onnx> [seconds]

use ort::value::Tensor;
use std::time::Instant;

const SR: usize = 44_100;
const DEFAULT_SEG: usize = (7.8 * SR as f64) as usize; // htdemucs default segment

fn peak_rss_mib() -> f64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmHWM"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|kb| kb.parse::<f64>().ok())
        })
        .map_or(0.0, |kb| kb / 1024.0)
}

fn synth_stereo(secs: usize) -> Vec<f32> {
    // interleaved-free: returns 2*n planar [L.., R..]
    let n = secs * SR;
    let mut buf = vec![0f32; 2 * n];
    let mut seed = 0x1234_5678_9abc_def0u64;
    for i in 0..n {
        let t = i as f32 / SR as f32;
        let tone = 0.3 * (2.0 * std::f32::consts::PI * 220.0 * t).sin()
            + 0.2 * (2.0 * std::f32::consts::PI * 660.0 * t).sin();
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let noise = ((seed >> 40) as f32 / 8_388_608.0 - 1.0) * 0.05;
        buf[i] = tone + noise;
        buf[n + i] = 0.9 * tone - noise;
    }
    buf
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let model_path = args
        .get(1)
        .expect("usage: ort_demucs_bench <htdemucs.onnx> [seconds]");
    let secs: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(45);

    let t_cold = Instant::now();
    let mut session = ort::session::Session::builder()?.commit_from_file(model_path)?;
    let cold_start = t_cold.elapsed();
    let threads = std::thread::available_parallelism().map_or(0, |n| n.get());

    let in_name = session.inputs[0].name.clone();
    let out_name = session.outputs[0].name.clone();
    let in_shape = format!("{:?}", session.inputs[0].input_type);
    let out_shape = format!("{:?}", session.outputs[0].output_type);

    // Derive channels/segment from the input tensor shape [b, C, L].
    let dims = extract_dims(&in_shape);
    let channels = dims.get(1).copied().filter(|&d| d > 0).unwrap_or(2) as usize;
    let seg_len = dims
        .get(2)
        .copied()
        .filter(|&d| d > 0)
        .unwrap_or(DEFAULT_SEG as i64) as usize;

    let audio = synth_stereo(secs); // planar [L.., R..], length 2*secs*SR
    let total_samples = secs * SR;
    let audio_secs = total_samples as f64 / SR as f64;
    let n_chunks = total_samples.div_ceil(seg_len);

    let mut infer_total = std::time::Duration::ZERO;
    let mut first_chunk_latency = std::time::Duration::ZERO;
    let mut sink = 0.0f64;
    for c in 0..n_chunks {
        let start = c * seg_len;
        let mut data = vec![0f32; channels * seg_len];
        for ch in 0..channels.min(2) {
            let src_base = ch * total_samples;
            for j in 0..seg_len {
                let idx = start + j;
                data[ch * seg_len + j] = if idx < total_samples {
                    audio[src_base + idx]
                } else {
                    0.0
                };
            }
        }
        let tensor = Tensor::from_array(([1i64, channels as i64, seg_len as i64], data))?;
        let t_inf = Instant::now();
        let outputs = session.run(ort::inputs![in_name.as_str() => tensor])?;
        let (_shape, out) = outputs[out_name.as_str()].try_extract_tensor::<f32>()?;
        let dt = t_inf.elapsed();
        if c == 0 {
            first_chunk_latency = cold_start + dt;
        }
        infer_total += dt;
        sink += f64::from(out[0]) + f64::from(out[out.len() / 2]);
    }

    let infer_secs = infer_total.as_secs_f64();
    println!("== ort / HTDemucs (Hybrid-Transformer Demucs v4) ==");
    println!("threads (intra-op default): {threads}");
    println!("input='{in_name}' {in_shape}");
    println!("output='{out_name}' {out_shape}");
    println!(
        "channels={channels} segment={seg_len} samples = {:.3}s audio/chunk",
        seg_len as f64 / SR as f64
    );
    println!("audio: {audio_secs:.2}s  chunks={n_chunks}");
    println!(
        "cold start (session build):   {:.1} ms",
        cold_start.as_secs_f64() * 1000.0
    );
    println!(
        "first-chunk latency (cold+1 infer): {:.1} ms",
        first_chunk_latency.as_secs_f64() * 1000.0
    );
    println!("pure inference total:         {:.2} s", infer_secs);
    println!(
        "  -> realtime factor:         {:.3}x  ({:.2} audio-s / wall-s)",
        infer_secs / audio_secs,
        audio_secs / infer_secs
    );
    println!("peak RSS:                     {:.1} MiB", peak_rss_mib());
    println!("(sink={sink:.3})");
    Ok(())
}

/// Pull integer dims out of ort's Debug shape string, e.g. "... shape: [-1, 2, 343980] ...".
fn extract_dims(s: &str) -> Vec<i64> {
    let Some(open) = s.find('[') else {
        return vec![];
    };
    let Some(close) = s[open..].find(']') else {
        return vec![];
    };
    s[open + 1..open + close]
        .split(',')
        .filter_map(|t| t.trim().parse::<i64>().ok())
        .collect()
}
