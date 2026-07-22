//! Package E SPIKE — ort/ONNX probe (candidate b).
//!
//! Minimal: initialise ONNX Runtime via `ort`, load an MDX-Net ONNX model, and
//! print its input/output tensor shapes so the full timing harness
//! (`ort_mdx_bench`) can be built against the real graph. Spike code: messy is
//! fine, it only has to compile and run.
//!
//! Run:  cargo run --release --features spike-ort --example ort_probe -- <model.onnx>

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let model_path: PathBuf = std::env::args()
        .nth(1)
        .expect("usage: ort_probe <model.onnx>")
        .into();

    let t0 = std::time::Instant::now();
    let session = ort::session::Session::builder()?.commit_from_file(&model_path)?;
    eprintln!("session init: {:?}", t0.elapsed());

    println!("== inputs ==");
    for input in &session.inputs {
        println!("  name={:?} type={:?}", input.name, input.input_type);
    }
    println!("== outputs ==");
    for output in &session.outputs {
        println!("  name={:?} type={:?}", output.name, output.output_type);
    }
    Ok(())
}
