//! The platform contract for AI stem separation (vocal removal), plus a
//! deterministic [`FakeStemBackend`] for tests.
//!
//! `reprise-core` stays runtime-free: the real ONNX/htdemucs implementation
//! lives in `crates/reprise-stems` (package G) behind this trait, exactly like
//! `playback`/`waveform`/`fingerprint`. The spike
//! (`docs/research/stem-separation-runtime.md`) picked ort + htdemucs; the
//! trait is deliberately shaped so that choice — chunked inference, a high
//! memory peak forcing one job at a time, cancel between chunks, progress
//! callbacks — needs no change to this contract.
//!
//! ## Shape rationale
//!
//! `separate_instrumental` takes a **source path** and an **output path**, a
//! **progress** sink and a **cancel** probe, and returns nothing on success
//! (the result is the file at `output`). Callbacks (rather than a channel or
//! an async stream) keep the trait object-safe and free of any runtime type,
//! so a synchronous worker on its own thread — the only host the plan builds
//! (2.4/2) — drives it directly. Progress is **permille** (0..=1000) to match
//! `ai_jobs.progress_permille`; the same number reaches the GTK bar, the CLI
//! and MCP unchanged (plan 2.2). `cancel` is polled **between chunks**: a
//! backend must check it at every chunk boundary and stop promptly with
//! [`StemError::Cancelled`], leaving no complete output behind.

use std::path::Path;

/// Progress reported by a backend, in permille (0..=1000) of the whole job —
/// the same unit stored in `ai_jobs.progress_permille`.
pub type ProgressPermille = u16;

/// The largest valid [`ProgressPermille`] — a completed job.
pub const PROGRESS_COMPLETE: ProgressPermille = 1000;

/// The canonical `"<name>@<version>"` model id for the v1 instrumental
/// pipeline — the single source of truth every frontend stamps so dedup
/// (`ai_jobs.params_fingerprint`) and provenance (`REPRISE_AI_MODEL`) line up
/// across the app, CLI and MCP instead of each hardcoding its own literal.
///
/// The shipped [`StemSeparationBackend`] MUST report a **matching**
/// [`StemSeparationBackend::model_id`]: this const names the weights a caller
/// asks for, and the backend confirms the weights it actually ran with. A
/// backend whose produced result would differ must report — and this const
/// must then become — a different id, since the fingerprint gates re-renders.
pub const CURRENT_MODEL_ID: &str = "htdemucs@4";

/// Why a separation run ended without producing its output.
#[derive(Debug, thiserror::Error)]
pub enum StemError {
    /// The caller's cancel probe returned `true` at a chunk boundary. No
    /// complete output file exists; the job becomes `cancelled`.
    #[error("stem separation was cancelled")]
    Cancelled,
    /// The source audio could not be opened/decoded.
    #[error("stem separation source could not be read: {0}")]
    SourceUnreadable(String),
    /// Writing the output (or a temporary) failed.
    #[error("stem separation I/O error: {0}")]
    Io(String),
    /// The inference runtime failed for any other reason (model load, shape
    /// mismatch, …). The string is diagnostic, not user-facing.
    #[error("stem separation backend error: {0}")]
    Backend(String),
}

/// A vocal-removal / stem-separation runtime. One implementation lives per
/// runtime choice; `reprise-core` only ever sees this trait.
///
/// Object-safe on purpose: a worker holds `Box<dyn StemSeparationBackend +
/// Send>` and runs exactly one job at a time (the spike's ~6 GB peak rules out
/// concurrency). No `Send`/`Sync` bound is baked in here so a runtime whose
/// session type is `!Sync` (e.g. ort) can still implement it — hosts that move
/// the backend to a worker thread ask for `+ Send` at the use site.
pub trait StemSeparationBackend {
    /// Produce the instrumental (vocals-removed) stem of `source` at `output`
    /// (a FLAC path the caller owns, inside the staging store). Report
    /// progress via `progress` and check `cancel` at every chunk boundary,
    /// returning [`StemError::Cancelled`] promptly — and writing no complete
    /// output — when it flips. Only the instrumental stem is written
    /// (Beschluss 19); a model that computes more stems reconstructs and
    /// discards the rest internally.
    fn separate_instrumental(
        &self,
        source: &Path,
        output: &Path,
        progress: &mut dyn FnMut(ProgressPermille),
        cancel: &dyn Fn() -> bool,
    ) -> Result<(), StemError>;

    /// A stable `"<name>@<version>"` identifier of the model/weights this
    /// backend produces output with. Feeds the job's `params_fingerprint`
    /// (dedup) and the `REPRISE_AI_MODEL` provenance tag, so it must change
    /// whenever the produced result would change.
    fn model_id(&self) -> String;
}

/// A deterministic stand-in backend for tests across every crate (core,
/// `reprise-cli` worker, `reprise-mcp`). It does no real DSP: it simulates a
/// chunked run — reporting `steps` progress updates and polling `cancel`
/// before each — then, on full completion, produces the output by copying the
/// source through unchanged (a valid, readable audio file the promotion path
/// can tag and register). Cancelling before the last step leaves no output.
///
/// `pub` (not `#[cfg(test)]`) because packages H1/H2 drive their job
/// round-trips through it; it pulls in nothing beyond `std`.
pub struct FakeStemBackend {
    steps: u16,
    model: String,
    outcome: FakeOutcome,
}

#[derive(Clone)]
enum FakeOutcome {
    /// Copy source → output on completion.
    Succeed,
    /// Fail at the given chunk index (0-based) with a backend error, without
    /// producing output — exercises the `failed` job path.
    FailAtStep(u16),
}

impl Default for FakeStemBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeStemBackend {
    /// A four-chunk backend that succeeds — the common test double.
    pub fn new() -> Self {
        Self {
            steps: 4,
            model: "fake-stems@1".to_string(),
            outcome: FakeOutcome::Succeed,
        }
    }

    /// Overrides the number of simulated chunks (must be >= 1).
    pub fn with_steps(mut self, steps: u16) -> Self {
        self.steps = steps.max(1);
        self
    }

    /// Overrides the reported model id (to test fingerprint/tag plumbing).
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// A backend that fails at `step` (0-based) with [`StemError::Backend`],
    /// producing no output — for the `failed` job path.
    pub fn failing_at(mut self, step: u16) -> Self {
        self.outcome = FakeOutcome::FailAtStep(step);
        self
    }
}

impl StemSeparationBackend for FakeStemBackend {
    fn separate_instrumental(
        &self,
        source: &Path,
        output: &Path,
        progress: &mut dyn FnMut(ProgressPermille),
        cancel: &dyn Fn() -> bool,
    ) -> Result<(), StemError> {
        if !source.exists() {
            return Err(StemError::SourceUnreadable(format!(
                "no such source file: {}",
                source.display()
            )));
        }
        for step in 0..self.steps {
            // Cancel is honored at the chunk boundary, before doing the chunk.
            if cancel() {
                return Err(StemError::Cancelled);
            }
            if let FakeOutcome::FailAtStep(fail_step) = self.outcome {
                if step == fail_step {
                    return Err(StemError::Backend(format!(
                        "fake backend failed at step {step}"
                    )));
                }
            }
            // Report progress AFTER completing this chunk. The final chunk
            // reports exactly PROGRESS_COMPLETE.
            let done = u32::from(step) + 1;
            let permille = (done * u32::from(PROGRESS_COMPLETE)) / u32::from(self.steps);
            progress(permille as ProgressPermille);
        }
        // One last cancel check after the final chunk, before committing the
        // output — a cancel that arrives at the very end still wins.
        if cancel() {
            return Err(StemError::Cancelled);
        }
        std::fs::copy(source, output).map_err(|error| StemError::Io(error.to_string()))?;
        Ok(())
    }

    fn model_id(&self) -> String {
        self.model.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn source_file(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("source.flac");
        std::fs::write(&path, b"fake audio bytes").unwrap();
        path
    }

    #[test]
    fn fake_backend_reports_monotonic_progress_ending_at_complete() {
        let dir = tempfile::tempdir().unwrap();
        let source = source_file(dir.path());
        let output = dir.path().join("out.flac");
        let backend = FakeStemBackend::new().with_steps(5);
        let reported = std::cell::RefCell::new(Vec::new());

        backend
            .separate_instrumental(
                &source,
                &output,
                &mut |p| reported.borrow_mut().push(p),
                &|| false,
            )
            .unwrap();

        let reported = reported.into_inner();
        assert_eq!(reported.len(), 5);
        assert!(
            reported.windows(2).all(|w| w[0] < w[1]),
            "progress must rise"
        );
        assert_eq!(*reported.last().unwrap(), PROGRESS_COMPLETE);
        assert!(output.exists(), "a completed run leaves an output file");
    }

    #[test]
    fn fake_backend_copies_source_bytes_to_output() {
        let dir = tempfile::tempdir().unwrap();
        let source = source_file(dir.path());
        let output = dir.path().join("out.flac");

        FakeStemBackend::new()
            .separate_instrumental(&source, &output, &mut |_| {}, &|| false)
            .unwrap();

        assert_eq!(std::fs::read(&output).unwrap(), b"fake audio bytes");
    }

    #[test]
    fn fake_backend_honors_cancel_between_chunks_and_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let source = source_file(dir.path());
        let output = dir.path().join("out.flac");
        // Cancel becomes true after the second progress step.
        let seen = Cell::new(0u16);
        let backend = FakeStemBackend::new().with_steps(8);

        let error = backend
            .separate_instrumental(&source, &output, &mut |_| seen.set(seen.get() + 1), &|| {
                seen.get() >= 2
            })
            .unwrap_err();

        assert!(matches!(error, StemError::Cancelled));
        assert!(!output.exists(), "a cancelled run leaves no output");
        assert!(seen.get() < 8, "cancel must stop the run early");
    }

    #[test]
    fn fake_backend_can_simulate_a_backend_failure() {
        let dir = tempfile::tempdir().unwrap();
        let source = source_file(dir.path());
        let output = dir.path().join("out.flac");

        let error = FakeStemBackend::new()
            .with_steps(4)
            .failing_at(1)
            .separate_instrumental(&source, &output, &mut |_| {}, &|| false)
            .unwrap_err();

        assert!(matches!(error, StemError::Backend(_)));
        assert!(!output.exists());
    }

    #[test]
    fn missing_source_is_reported_not_panicked() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("out.flac");
        let error = FakeStemBackend::new()
            .separate_instrumental(
                &dir.path().join("absent.flac"),
                &output,
                &mut |_| {},
                &|| false,
            )
            .unwrap_err();
        assert!(matches!(error, StemError::SourceUnreadable(_)));
    }

    #[test]
    fn model_id_is_stable_and_overridable() {
        assert_eq!(FakeStemBackend::new().model_id(), "fake-stems@1");
        assert_eq!(FakeStemBackend::new().with_model("x@2").model_id(), "x@2");
    }

    #[test]
    fn current_model_id_is_a_name_at_version() {
        assert!(!CURRENT_MODEL_ID.is_empty());
        assert!(
            CURRENT_MODEL_ID.contains('@'),
            "the canonical model id is <name>@<version>"
        );
    }
}
