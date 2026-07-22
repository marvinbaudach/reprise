//! Package E SPIKE — ort/ONNX MDX-Net timing harness (candidate b).
//!
//! Measures, on this machine and in the release profile, the runtime cost of an
//! MDX-class separation model under `ort` (ONNX Runtime): cold-start, first-
//! chunk latency, realtime factor (pure inference and full STFT->infer->ISTFT
//! pipeline), and peak RSS. Quality is NOT judged here — the input is a
//! synthesised tone+noise mix, sized for faithful TIMING only.
//!
//! The model graph is real: input/output are [batch, 4, dim_f=3072, dim_t=256]
//! (probed from UVR-MDX-NET-Inst_HQ_1.onnx). MDX standard hop=1024, n_fft=6144,
//! so one chunk spans dim_t*hop = 262 144 samples = 5.945 s at 44.1 kHz.
//!
//! Run:
//!   cargo run --release --features spike-ort --example ort_mdx_bench -- <model.onnx> [seconds]

use ort::value::Tensor;
use rustfft::{num_complex::Complex32, FftPlanner};
use std::time::Instant;

const SR: usize = 44_100;
const HOP: usize = 1024;
const N_FFT: usize = 6144;
const DIM_F: usize = 3072; // frequency bins kept by the model
const DIM_T: usize = 256; // frames per chunk

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

/// Synthesise a stereo tone+noise mix: `secs` seconds, 44.1 kHz, f32 in [-1,1].
fn synth_stereo(secs: usize) -> (Vec<f32>, Vec<f32>) {
    let n = secs * SR;
    let mut l = vec![0f32; n];
    let mut r = vec![0f32; n];
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    for i in 0..n {
        let t = i as f32 / SR as f32;
        // a few partials so the spectrogram is non-trivial across bands
        let tone = 0.3 * (2.0 * std::f32::consts::PI * 220.0 * t).sin()
            + 0.2 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
            + 0.15 * (2.0 * std::f32::consts::PI * 1760.0 * t).sin();
        // cheap xorshift noise
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let noise = ((seed >> 40) as f32 / 8_388_608.0 - 1.0) * 0.05;
        l[i] = tone + noise;
        r[i] = 0.9 * tone - noise;
    }
    (l, r)
}

/// Full STFT of one channel: returns (real, imag) laid out [frame][DIM_F].
fn stft(
    x: &[f32],
    planner: &mut FftPlanner<f32>,
    window: &[f32],
) -> (Vec<Vec<f32>>, Vec<Vec<f32>>) {
    let fft = planner.plan_fft_forward(N_FFT);
    let n_frames = x.len().div_ceil(HOP);
    let mut re = Vec::with_capacity(n_frames);
    let mut im = Vec::with_capacity(n_frames);
    let mut buf = vec![Complex32::new(0.0, 0.0); N_FFT];
    for f in 0..n_frames {
        let start = f * HOP;
        for (j, slot) in buf.iter_mut().enumerate() {
            let idx = start + j;
            let s = if idx < x.len() {
                x[idx] * window[j]
            } else {
                0.0
            };
            *slot = Complex32::new(s, 0.0);
        }
        fft.process(&mut buf);
        re.push(buf[..DIM_F].iter().map(|c| c.re).collect());
        im.push(buf[..DIM_F].iter().map(|c| c.im).collect());
    }
    (re, im)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let model_path = args
        .get(1)
        .expect("usage: ort_mdx_bench <model.onnx> [seconds]");
    let secs: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(45);

    // ---- cold start: session build (model load + graph optimise) ----
    let t_cold = Instant::now();
    let mut session = ort::session::Session::builder()?.commit_from_file(model_path)?;
    let cold_start = t_cold.elapsed();
    let threads = std::thread::available_parallelism().map_or(0, |n| n.get());
    let in_name = session.inputs[0].name.clone();
    let out_name = session.outputs[0].name.clone();

    // ---- prepare audio + STFT ----
    let (l, r) = synth_stereo(secs);
    let audio_secs = l.len() as f64 / SR as f64;
    let window: Vec<f32> = (0..N_FFT)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / N_FFT as f32).cos())
        .collect();
    let mut planner = FftPlanner::<f32>::new();

    let t_pipe = Instant::now();
    let (lre, lim) = stft(&l, &mut planner, &window);
    let (rre, rim) = stft(&r, &mut planner, &window);
    let stft_time = t_pipe.elapsed();
    let n_frames = lre.len();
    let n_chunks = n_frames.div_ceil(DIM_T);

    // ---- run every chunk; time first chunk and the pure-inference total ----
    let mut infer_total = std::time::Duration::ZERO;
    let mut first_chunk_latency = std::time::Duration::ZERO;
    let mut sink = 0.0f64; // keep outputs live
    for c in 0..n_chunks {
        // pack [1, 4, DIM_F, DIM_T] = channels [L_re, L_im, R_re, R_im]
        let mut data = vec![0f32; 4 * DIM_F * DIM_T];
        for t in 0..DIM_T {
            let frame = c * DIM_T + t;
            if frame >= n_frames {
                break;
            }
            for bin in 0..DIM_F {
                data[(0 * DIM_F + bin) * DIM_T + t] = lre[frame][bin];
                data[(1 * DIM_F + bin) * DIM_T + t] = lim[frame][bin];
                data[(2 * DIM_F + bin) * DIM_T + t] = rre[frame][bin];
                data[(3 * DIM_F + bin) * DIM_T + t] = rim[frame][bin];
            }
        }
        let tensor = Tensor::from_array(([1i64, 4, DIM_F as i64, DIM_T as i64], data))?;
        let t_inf = Instant::now();
        let outputs = session.run(ort::inputs![in_name.as_str() => tensor])?;
        let (_shape, out) = outputs[out_name.as_str()].try_extract_tensor::<f32>()?;
        let dt = t_inf.elapsed();
        if c == 0 {
            first_chunk_latency = cold_start + stft_time + dt;
        }
        infer_total += dt;
        sink += f64::from(out[0]) + f64::from(out[out.len() / 2]);
    }
    let pipeline_total = t_pipe.elapsed();

    let infer_secs = infer_total.as_secs_f64();
    let pipe_secs = pipeline_total.as_secs_f64();
    println!("== ort / MDX-Net (UVR-MDX-NET-Inst_HQ) ==");
    println!("threads (intra-op default): {threads}");
    println!(
        "input='{in_name}' output='{out_name}'  chunk={DIM_T} frames = {:.3}s audio",
        (DIM_T * HOP) as f64 / SR as f64
    );
    println!("audio: {audio_secs:.2}s  frames={n_frames}  chunks={n_chunks}");
    println!(
        "cold start (session build):   {:.1} ms",
        cold_start.as_secs_f64() * 1000.0
    );
    println!(
        "first-chunk latency (cold+stft+1 infer): {:.1} ms",
        first_chunk_latency.as_secs_f64() * 1000.0
    );
    println!(
        "STFT (both channels):         {:.1} ms",
        stft_time.as_secs_f64() * 1000.0
    );
    println!("pure inference total:         {:.2} s", infer_secs);
    println!(
        "  -> realtime factor (infer): {:.3}x  ({:.2} audio-s / wall-s)",
        infer_secs / audio_secs,
        audio_secs / infer_secs
    );
    println!("full pipeline total:          {:.2} s", pipe_secs);
    println!(
        "  -> realtime factor (pipe):  {:.3}x  ({:.2} audio-s / wall-s)",
        pipe_secs / audio_secs,
        audio_secs / pipe_secs
    );
    println!("peak RSS:                     {:.1} MiB", peak_rss_mib());
    println!("(sink={sink:.3})");
    Ok(())
}
