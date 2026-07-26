//! # reprise-stems — production stem-separation backend
//!
//! Implements [`reprise_core::stem_separation::StemSeparationBackend`] with the
//! runtime the package E spike chose (`docs/research/stem-separation-runtime.md`):
//! **ort (ONNX Runtime)** driving **htdemucs** (Hybrid-Transformer Demucs v4)
//! exported to ONNX, whose weights are MIT (Meta) and pass the `LICENSING.md`
//! model gate. Only the **instrumental** stem is written (Beschluss 19).
//!
//! ## What the backend does
//!
//! `OrtStemBackend::separate_instrumental` (behind the `ort` feature) decodes
//! the source to 44.1 kHz stereo f32, runs htdemucs **segment by segment** with
//! the Demucs overlap-add window, sums the non-vocal sources (drums + bass +
//! other) into the instrumental, and encodes it to FLAC at the caller's output
//! path. Cancel is honoured **between segments**, progress is reported per
//! segment in permille, and the stitching path is deterministic.
//!
//! ## Crate feature layout
//!
//! The default build is intentionally light — no onnxruntime, no audio codecs —
//! so the architecture gate stays green and the pure logic is unit-testable
//! without any native library or model:
//!
//! * **default**: [`chunk`] (segment geometry + fade window), [`pcm`]
//!   (f32→integer PCM), [`separate`] (the cancel/progress/stitch orchestration,
//!   generic over an inference fn), [`model`] (the pinned weights + identity),
//!   and [`provision`] (checksummed download + licence notice + onnxruntime
//!   library resolution, the network fetch injected). Dependencies: `sha2`,
//!   `dirs` — both tiny and pure-Rust.
//! * **`provision-http`**: the blocking `ureq` model fetcher without inference.
//! * **`ort`**: the real `OrtStemBackend` plus audio
//!   decode/resample/encode; it also enables `provision-http`. Pulls `ort`,
//!   `symphonia` (MPL-2.0), `flacenc` (Apache-2.0), `rubato` (MIT), `ndarray`
//!   and `ureq`. Only the dedicated worker binary enables this.
//!
//! ## onnxruntime linkage — the Flatpak-offline story
//!
//! The `ort` feature links onnxruntime with **`load-dynamic`**: nothing is
//! downloaded or linked at build time, and `libonnxruntime.so` is `dlopen`ed at
//! runtime. This keeps the build **offline-buildable** (the requirement for
//! Flathub) — even `cargo build --all-features` fetches no onnxruntime.
//! [`provision::resolve_library`] locates the library from an explicit,
//! optionally checksummed candidate list ([`provision::onnxruntime_location`]):
//! `ORT_DYLIB_PATH` first, then a host-bundled `libonnxruntime.so` beside the
//! models; if none exists it fails with a clear message. A Flatpak ships the
//! library as a checksum-declared source, installs it under `/app/lib/reprise`,
//! and embeds its path plus SHA-256 into the app build. onnxruntime is pinned
//! to **1.22.0** (ort 2.0-rc.10).
//!
//! For local development, point `ORT_DYLIB_PATH` at any onnxruntime 1.22.0
//! `libonnxruntime.so` (e.g. the official
//! `onnxruntime-linux-x64-1.22.0` release). The alternative build-time strategy
//! — cargo's `download-binaries`, which statically links a prebuilt runtime — is
//! deliberately **not** wired in, because it breaks offline builds and cannot
//! coexist as a clean override of `load-dynamic`.
//!
//! ## Model provisioning
//!
//! Weights are never bundled. On first use [`provision::ensure_weights`]
//! downloads [`model::HTDEMUCS_FP32`] to `<XDG data>/reprise/models`, verifies
//! its SHA-256 (a tampered file is rejected and never written), and writes the
//! MIT licence notice beside it. A present, valid model is reused offline. Tests
//! inject a local-bytes fetcher, so the suite needs no network.
//!
//! ## fp16 note
//!
//! [`model::HTDEMUCS_FP16`] (~166 MB) is a documented, checksummed alternative.
//! It is **not** the default: onnxruntime's CPU EP up-casts fp16 to fp32, so
//! runtime memory/latency do not improve (only the download shrinks), and its
//! output differs numerically — hence its own `model_id`. Switching to it is a
//! deliberate change of the produced identity, not a transparent swap.

pub mod chunk;
pub mod model;
pub mod pcm;
pub mod provision;
pub mod separate;

#[cfg(feature = "ort")]
mod audio_io;
#[cfg(feature = "ort")]
mod ort_backend;

#[cfg(feature = "ort")]
pub use ort_backend::OrtStemBackend;

// Re-exported so hosts can name the contract and its errors through this crate.
pub use reprise_core::stem_separation::{
    ProgressPermille, StemError, StemSeparationBackend, PROGRESS_COMPLETE,
};
