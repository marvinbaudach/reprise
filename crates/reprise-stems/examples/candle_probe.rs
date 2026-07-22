//! Package E SPIKE — candle probe (candidate a).
//!
//! candle-transformers ships NO Demucs/HTDemucs model, and the only Rust
//! Demucs port (`demucs-rs`) targets Burn, not candle. A real htdemucs RTF
//! therefore cannot be measured without hand-porting the Hybrid-Transformer
//! Demucs architecture into candle and converting the PyTorch weights — the
//! spike's timeboxed finding (see docs/research/stem-separation-runtime.md).
//!
//! What this probe DOES establish, factually and on this machine:
//!   * candle compiles and runs pure-Rust on CPU (no native ML lib);
//!   * a rough CPU throughput for the conv/matmul work a Demucs-class encoder
//!     is built from, so we can reason about whether a port could reach
//!     realtime — this is a micro-benchmark, deliberately NOT a model RTF.
//!
//! Run:  cargo run --release --features spike-candle --example candle_probe

use candle_core::{DType, Device, Tensor};
use std::time::Instant;

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

/// One Demucs-ish spectrogram-encoder conv step: [B,Cin,F,T] -> conv3x3 -> GELU.
fn conv_block(x: &Tensor, w: &Tensor, b: &Tensor) -> candle_core::Result<Tensor> {
    // padding=1, stride=2 (downsample), dilation=1, groups=1.
    let y = x.conv2d(w, 1, 2, 1, 1)?;
    let y = y.broadcast_add(&b.reshape((1, b.dim(0)?, 1, 1))?)?;
    y.gelu()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dev = Device::Cpu;
    println!("device: {dev:?} (Cpu => pure-Rust candle backend)");

    // Encoder-shaped conv stack over a spectrogram-like input, channels growing
    // 4 -> 48 -> 96 -> 192 -> 384, each halving F and T (htdemucs-ish widths).
    let mut x = Tensor::randn(0f32, 1.0, (1, 4, 2048, 256), &dev)?;
    let chans = [(4usize, 48usize), (48, 96), (96, 192), (192, 384)];
    let mut weights = Vec::new();
    for &(cin, cout) in &chans {
        let w = Tensor::randn(0f32, 0.05, (cout, cin, 3, 3), &dev)?;
        let b = Tensor::zeros(cout, DType::F32, &dev)?;
        weights.push((w, b));
    }

    // Warm-up (allocator, code paths) then timed passes.
    for _ in 0..2 {
        let mut y = x.clone();
        for (w, b) in &weights {
            y = conv_block(&y, w, b)?;
        }
        let _ = y.sum_all()?.to_scalar::<f32>()?;
    }

    let iters = 20;
    let t0 = Instant::now();
    for _ in 0..iters {
        let mut y = x.clone();
        for (w, b) in &weights {
            y = conv_block(&y, w, b)?;
        }
        let _ = y.sum_all()?.to_scalar::<f32>()?; // force materialisation
        x = x.affine(1.0, 0.0)?; // keep x live, defeat caching
    }
    let per = t0.elapsed().as_secs_f64() / f64::from(iters);
    println!(
        "conv-encoder stack: {:.1} ms/pass over {iters} passes",
        per * 1000.0
    );

    // A transformer-ish matmul (htdemucs bottleneck: seq ~ 512, dim 384).
    let a = Tensor::randn(0f32, 1.0, (1, 512, 384), &dev)?;
    let bmat = Tensor::randn(0f32, 1.0, (1, 384, 384), &dev)?;
    let t1 = Instant::now();
    let mm_iters = 200;
    for _ in 0..mm_iters {
        let _ = a.broadcast_matmul(&bmat)?.sum_all()?.to_scalar::<f32>()?;
    }
    let mm_per = t1.elapsed().as_secs_f64() / f64::from(mm_iters);
    // 2*M*N*K flops for the matmul.
    let flops = 2.0 * 512.0 * 384.0 * 384.0;
    println!(
        "matmul 512x384x384: {:.3} ms/op => {:.1} GFLOP/s",
        mm_per * 1000.0,
        flops / mm_per / 1e9
    );

    println!("peak RSS: {:.1} MiB", peak_rss_mib());
    println!("candle probe OK (pure-Rust CPU inference confirmed)");
    Ok(())
}
