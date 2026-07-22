//! Stub library for `reprise-stems`. Package E spikes the ML runtime (candle
//! vs. ort) and package G implements the real `StemSeparationBackend` — the
//! contract `reprise-core` gains in package D (Track 2). Nothing depends on
//! this crate yet; it exists so the workspace member compiles.
//!
//! Package E outcome: the runtime spike lives in `examples/` behind the
//! optional `spike-candle`/`spike-ort` features (skipped by the default build).
//! The measurements and the runtime/model recommendation are written up in
//! `docs/research/stem-separation-runtime.md` — recommendation: `ort` (ONNX
//! Runtime) with MIT-licensed htdemucs weights shipped as ONNX.

// Forward-looking dependency edge on the engine whose backend trait this crate
// will implement; package G replaces this with the real implementation.
use reprise_core as _;
