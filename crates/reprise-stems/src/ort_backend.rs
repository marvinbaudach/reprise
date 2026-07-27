//! The real [`StemSeparationBackend`] — ort (ONNX Runtime) running htdemucs.
//!
//! Wires the pure orchestration ([`crate::separate`]) to a real ONNX session:
//! decode the source ([`crate::audio_io`]), run each segment through htdemucs,
//! reduce to the instrumental, and encode the FLAC render. onnxruntime is
//! located and loaded dynamically ([`crate::provision::resolve_library`]) so the
//! build stays Flatpak-offline compatible.

use std::cell::RefCell;
use std::path::Path;

use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;

use reprise_core::stem_separation::{ProgressPermille, StemError, StemSeparationBackend};

use crate::chunk::Geometry;
use crate::model::{self, WeightsSpec};
use crate::{audio_io, provision, separate};

/// Stereo — htdemucs works in two channels.
const CHANNELS: usize = 2;

/// The production stem-separation backend: one loaded htdemucs ONNX session
/// used for one job at a time.
///
/// The ort [`Session`] needs `&mut` to run, but the trait method takes `&self`,
/// so the session lives in a [`RefCell`]. That makes the backend `Send` (it can
/// be moved onto a worker thread, as the plan's host does) but not `Sync` —
/// exactly the shape [`reprise_core::stem_separation`] leaves room for. A single
/// worker drives one job at a time, so the interior mutability is never
/// contended.
pub struct OrtStemBackend {
    session: RefCell<Session>,
    geometry: Geometry,
    model_id: String,
}

impl OrtStemBackend {
    /// Loads the default htdemucs fp32 backend, provisioning the weights on
    /// first use: download (checksum-verified) + licence notice, into
    /// `<XDG data>/reprise/models`. Subsequent calls reuse the verified file
    /// offline.
    pub fn with_default_model() -> Result<Self, StemError> {
        Self::with_provisioned_weights(&model::HTDEMUCS_FP32)
    }

    /// Provisions and loads a specific weights set (fp32 or fp16).
    pub fn with_provisioned_weights(spec: &WeightsSpec) -> Result<Self, StemError> {
        let model_dir =
            provision::default_model_dir().map_err(|e| StemError::Backend(e.to_string()))?;
        let path = provision::ensure_weights(&model_dir, spec, &provision::http_fetcher)
            .map_err(|e| StemError::Backend(e.to_string()))?;
        Self::from_model_file(&path, spec.model_id)
    }

    /// Loads the default htdemucs backend **only if the model is already
    /// provisioned** on disk — never downloads. `Ok(None)` means the model file
    /// is absent, so a host can degrade gracefully with the download flow still
    /// pending; `Err` is a genuine load failure (corrupt model, or onnxruntime
    /// unavailable). This is the constructor the worker hosts use, so a normal
    /// `jobs work` / app launch never blocks on a 316 MB network fetch and the
    /// hermetic test suite never touches the network.
    pub fn from_provisioned_default() -> Result<Option<Self>, StemError> {
        match provision::runtime_readiness() {
            provision::RuntimeReadiness::Ready(assets) => {
                Self::from_verified_runtime(&assets).map(Some)
            }
            provision::RuntimeReadiness::ModelRequired { .. } => Ok(None),
            provision::RuntimeReadiness::Unavailable { detail, .. } => {
                Err(StemError::Backend(detail))
            }
        }
    }

    /// [`from_provisioned_default`](Self::from_provisioned_default) with the
    /// model directory injected, so the "not provisioned" path is unit-testable
    /// without touching the real XDG data dir or the network.
    pub fn from_provisioned_in(model_dir: &Path) -> Result<Option<Self>, StemError> {
        let path = provision::weights_path(model_dir, &model::HTDEMUCS_FP32);
        if !path.is_file() {
            return Ok(None);
        }
        Self::from_model_file(&path, model::HTDEMUCS_FP32.model_id).map(Some)
    }

    /// Loads a backend from an explicit local ONNX file with an explicit model
    /// identity — the entry point the end-to-end evidence run and any offline
    /// host use. onnxruntime is located via the load-dynamic strategy first, so
    /// a clear error surfaces here if no library is available.
    pub fn from_model_file(model_path: &Path, model_id: &str) -> Result<Self, StemError> {
        configure_onnxruntime()?;
        Self::from_configured_model_file(model_path, model_id)
    }

    /// Constructs the backend from paths already verified by the shared
    /// readiness probe. Production hosts use this so their go/no-go decision
    /// and the backend initialization consume the same assets.
    pub fn from_verified_runtime(assets: &provision::RuntimeAssets) -> Result<Self, StemError> {
        std::env::set_var(provision::ORT_DYLIB_ENV, &assets.library_path);
        Self::from_configured_model_file(&assets.model_path, &assets.model_id)
    }

    fn from_configured_model_file(model_path: &Path, model_id: &str) -> Result<Self, StemError> {
        let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
        let session = Session::builder()
            .map_err(|e| session_error(&e))?
            // Level3 (all optimisations) is ort's default; set it explicitly to
            // match the model's reference inference.
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| session_error(&e))?
            // A fixed intra-op thread count keeps onnxruntime's reduction order
            // stable, so identical input yields identical output on this host.
            .with_intra_threads(threads)
            .map_err(|e| session_error(&e))?
            .commit_from_file(model_path)
            .map_err(|e| StemError::Backend(format!("load model {}: {e}", model_path.display())))?;
        Ok(Self {
            session: RefCell::new(session),
            geometry: Geometry::htdemucs(),
            model_id: model_id.to_string(),
        })
    }
}

impl StemSeparationBackend for OrtStemBackend {
    fn separate_instrumental(
        &self,
        source: &Path,
        output: &Path,
        progress: &mut dyn FnMut(ProgressPermille),
        cancel: &dyn Fn() -> bool,
    ) -> Result<(), StemError> {
        // Decode up front so a cancel before any inference is cheap and leaves
        // no output.
        let stereo = audio_io::decode_to_stereo_44100(source)?;

        let geometry = self.geometry;
        let mut session = self.session.borrow_mut();
        let mut infer = |mix: &[f32]| run_segment(&mut session, geometry.segment, mix);

        let instrumental =
            separate::separate_instrumental(&stereo, &geometry, progress, cancel, &mut infer)?;

        // Only a fully completed run reaches here; a cancel/error returned above
        // via `?` and no output was written.
        audio_io::encode_flac(output, &instrumental)
    }

    fn model_id(&self) -> String {
        self.model_id.clone()
    }
}

/// Runs one segment through the ONNX session: planar mix `[ch0…, ch1…]` in,
/// planar stems `[source][channel][sample]` out — exactly the layout
/// [`separate`] expects, since htdemucs' `stems` output is `[1, 4, 2, N]`
/// row-major and the batch axis is 1.
fn run_segment(session: &mut Session, segment: usize, mix: &[f32]) -> Result<Vec<f32>, StemError> {
    let tensor = Tensor::from_array(([1_i64, CHANNELS as i64, segment as i64], mix.to_vec()))
        .map_err(|e| StemError::Backend(format!("build input tensor: {e}")))?;

    let outputs = session
        .run(ort::inputs!["mix" => tensor])
        .map_err(|e| StemError::Backend(format!("inference: {e}")))?;

    let (_shape, data) = outputs["stems"]
        .try_extract_tensor::<f32>()
        .map_err(|e| StemError::Backend(format!("extract stems: {e}")))?;
    Ok(data.to_vec())
}

/// Locates onnxruntime for the load-dynamic linkage and points ort at it. A
/// missing library is a clear [`StemError::Backend`] naming where it looked.
fn configure_onnxruntime() -> Result<(), StemError> {
    let location = provision::onnxruntime_location();
    let unverified = location.expected_sha256.is_none();
    let library =
        provision::resolve_library(&location).map_err(|e| StemError::Backend(e.to_string()))?;
    if unverified {
        // load-dynamic dlopens native code into this process; without a pinned
        // checksum a swapped/planted library would execute with full privileges.
        // Loud, so a mis-packaged production build is caught. No tracing dep in
        // this lean crate, so stderr is the loud channel.
        eprintln!(
            "WARNING: loading onnxruntime from {} WITHOUT checksum verification. \
             Production packaging MUST set {} to the pinned SHA-256 of the shipped \
             libonnxruntime.so so a swapped or planted library cannot execute in-process.",
            library.display(),
            provision::ORT_DYLIB_SHA256_ENV,
        );
    }
    // ort reads this env var to `dlopen` the runtime. Edition 2021: `set_var`
    // is safe; this runs during single-threaded backend construction, before
    // any session exists.
    std::env::set_var(provision::ORT_DYLIB_ENV, &library);
    Ok(())
}

fn session_error(e: &ort::Error) -> StemError {
    StemError::Backend(format!("onnxruntime session: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_provisioned_in_is_none_when_the_model_is_absent() {
        // Graceful "not provisioned yet": no model file in the dir yields None
        // (download flow pending) — never a network fetch or a panic.
        let dir = tempfile::tempdir().unwrap();
        assert!(OrtStemBackend::from_provisioned_in(dir.path())
            .unwrap()
            .is_none());
    }
}
