//! First-use model provisioning and onnxruntime library location.
//!
//! Two concerns, both pure enough to unit-test in the default build (no ort, no
//! network):
//!
//! 1. **Weights** ([`ensure_weights`]): download-on-first-use with SHA-256
//!    verification and a licence notice written beside the file (plan 2.4.9).
//!    The network fetch is injected, so tests drive it from local bytes; the
//!    real `ureq` fetcher ([`http_fetcher`]) is compiled only with the `ort`
//!    feature. A tampered download is rejected and never written.
//! 2. **onnxruntime library** ([`resolve_library`]): the default build loads
//!    onnxruntime dynamically (`load-dynamic`), so at runtime a
//!    `libonnxruntime.so` must be located from an explicit, optionally
//!    checksummed set of candidates — with a clear error when none is present.

use std::fmt::Write as _;
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::model::{WeightsSpec, HTDEMUCS_LICENSE_NOTICE};

/// Environment variable ort itself reads for the dynamic library path; we honor
/// it first so a host/Flatpak can point at its bundled, checksummed library.
pub const ORT_DYLIB_ENV: &str = "ORT_DYLIB_PATH";

/// Optional expected SHA-256 (lower hex) for the onnxruntime library, so a
/// Flatpak/host can pin the exact library it shipped.
pub const ORT_DYLIB_SHA256_ENV: &str = "REPRISE_ORT_DYLIB_SHA256";

/// Something went wrong provisioning the model or locating onnxruntime.
#[derive(Debug, thiserror::Error)]
pub enum ProvisionError {
    /// The download's SHA-256 did not match the pinned value — the file is
    /// tampered or corrupt and was NOT written to disk.
    #[error("checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
    /// The injected/real fetcher failed (network, HTTP status, size cap).
    #[error("model download failed: {0}")]
    Fetch(String),
    /// A filesystem operation failed.
    #[error("model provisioning I/O error: {0}")]
    Io(String),
    /// No onnxruntime library could be found among the candidates.
    #[error("onnxruntime library not found; looked in: {searched}. Set {env} to a libonnxruntime.so (onnxruntime 1.22.0).")]
    LibraryNotFound { searched: String, env: &'static str },
}

impl From<std::io::Error> for ProvisionError {
    fn from(error: std::io::Error) -> Self {
        ProvisionError::Io(error.to_string())
    }
}

/// A network fetcher: URL in, bytes out. Injected so the download path is
/// tested from local data and never touches the network in tests.
pub type Fetcher<'a> = dyn Fn(&str) -> Result<Vec<u8>, String> + 'a;

/// `<XDG data>/reprise/models` — the production model directory, resolved to
/// the same base as the database and staging store.
pub fn default_model_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("reprise/models")
}

/// The on-disk path a spec's weights live at inside `model_dir`.
pub fn weights_path(model_dir: &Path, spec: &WeightsSpec) -> PathBuf {
    model_dir.join(spec.file_name)
}

/// The path of the licence notice written beside a spec's weights.
pub fn license_path(model_dir: &Path, spec: &WeightsSpec) -> PathBuf {
    model_dir.join(spec.license_file_name())
}

/// Ensures the weights for `spec` are present and verified in `model_dir`,
/// returning their path.
///
/// * Already present **and** checksum-valid → returned immediately, no fetch
///   (so second use is offline and instant).
/// * Otherwise → `fetch(url)`, verify SHA-256 (a mismatch is
///   [`ProvisionError::ChecksumMismatch`] and nothing is written), then publish
///   atomically (temp file + rename) and write the licence notice beside it.
pub fn ensure_weights(
    model_dir: &Path,
    spec: &WeightsSpec,
    fetch: &Fetcher<'_>,
) -> Result<PathBuf, ProvisionError> {
    let target = weights_path(model_dir, spec);
    let already_valid = target.is_file() && file_sha256(&target).is_ok_and(|h| h == spec.sha256);
    if already_valid {
        // Present and intact — make sure the licence notice is there too, then
        // hand back the path without any network use.
        write_license_notice(model_dir, spec)?;
        return Ok(target);
    }

    let bytes = fetch(spec.url).map_err(ProvisionError::Fetch)?;
    let actual = sha256_hex(&bytes);
    if actual != spec.sha256 {
        return Err(ProvisionError::ChecksumMismatch {
            expected: spec.sha256.to_string(),
            actual,
        });
    }

    std::fs::create_dir_all(model_dir)?;
    write_atomic(&target, &bytes)?;
    write_license_notice(model_dir, spec)?;
    Ok(target)
}

/// Writes the htdemucs licence notice beside the weights (idempotent).
pub fn write_license_notice(model_dir: &Path, spec: &WeightsSpec) -> Result<(), ProvisionError> {
    std::fs::create_dir_all(model_dir)?;
    let path = license_path(model_dir, spec);
    std::fs::write(&path, HTDEMUCS_LICENSE_NOTICE)?;
    Ok(())
}

/// Where to find the onnxruntime dynamic library and, optionally, the SHA-256
/// it must match.
#[derive(Debug, Clone, Default)]
pub struct LibraryLocation {
    /// Candidate paths in priority order (env-provided first).
    pub candidates: Vec<PathBuf>,
    /// Optional expected SHA-256 (lower hex) the resolved library must match.
    pub expected_sha256: Option<String>,
}

/// Resolves the first existing candidate onnxruntime library, verifying its
/// SHA-256 when one is pinned. Returns [`ProvisionError::LibraryNotFound`]
/// listing every path tried when none exists — the "clear error when absent"
/// the plan requires for the load-dynamic path.
pub fn resolve_library(location: &LibraryLocation) -> Result<PathBuf, ProvisionError> {
    for candidate in &location.candidates {
        if !candidate.is_file() {
            continue;
        }
        if let Some(expected) = &location.expected_sha256 {
            let actual = file_sha256(candidate)?;
            if &actual != expected {
                return Err(ProvisionError::ChecksumMismatch {
                    expected: expected.clone(),
                    actual,
                });
            }
        }
        return Ok(candidate.clone());
    }
    let searched = location
        .candidates
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Err(ProvisionError::LibraryNotFound {
        searched: if searched.is_empty() {
            "(no candidates)".to_string()
        } else {
            searched
        },
        env: ORT_DYLIB_ENV,
    })
}

/// Assembles the production candidate list for onnxruntime from the environment
/// (`ORT_DYLIB_PATH` first, then a Reprise-bundled library beside the model
/// directory) plus an optional pinned checksum from [`ORT_DYLIB_SHA256_ENV`].
///
/// Reads the environment (a side effect), so the pure resolution logic lives in
/// [`resolve_library`], which this feeds.
pub fn onnxruntime_location() -> LibraryLocation {
    let mut candidates = Vec::new();
    if let Some(explicit) = std::env::var_os(ORT_DYLIB_ENV) {
        candidates.push(PathBuf::from(explicit));
    }
    // A library the host bundled next to the models (e.g. a Flatpak extension).
    candidates.push(default_model_dir().join("libonnxruntime.so"));
    LibraryLocation {
        candidates,
        expected_sha256: std::env::var(ORT_DYLIB_SHA256_ENV).ok(),
    }
}

/// The real network fetcher (blocking `ureq`), compiled only with the `ort`
/// feature. Enforces a generous size cap so a mispointed URL cannot exhaust
/// memory. Tests never use this — they inject a local-bytes fetcher.
#[cfg(feature = "ort")]
pub fn http_fetcher(url: &str) -> Result<Vec<u8>, String> {
    // htdemucs fp32 is ~316 MB; cap comfortably above it.
    const MAX_BYTES: u64 = 512 * 1024 * 1024;
    let response = ureq::get(url)
        .call()
        .map_err(|e| format!("request failed: {e}"))?;
    let mut bytes = Vec::new();
    response
        .into_body()
        .into_reader()
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("read failed: {e}"))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("download exceeded the size cap".to_string());
    }
    Ok(bytes)
}

fn write_atomic(target: &Path, bytes: &[u8]) -> Result<(), ProvisionError> {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let tmp = parent.join(format!(
        ".{}.{}.tmp",
        target
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("model"),
        temp_token()
    ));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, target).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        ProvisionError::Io(e.to_string())
    })
}

/// A unique-enough token for the temp file name, without pulling a dependency:
/// the process id mixed with a nanosecond clock reading.
fn temp_token() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    format!(
        "{:016x}",
        nanos ^ (std::process::id() as u64).rotate_left(17)
    )
}

/// Streams a file through SHA-256, returning lower-case hex. Avoids loading a
/// 300 MB model fully into memory just to re-verify it.
pub fn file_sha256(path: &Path) -> Result<String, ProvisionError> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex_lower(&hasher.finalize()))
}

/// SHA-256 of an in-memory buffer, lower-case hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex_lower(&Sha256::digest(bytes))
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::HTDEMUCS_FP32;

    // A tiny fake weights spec whose checksum matches `PAYLOAD`, so provisioning
    // is exercised without the 316 MB real model. The sha256 is computed from
    // the payload (leaked to `'static`) so the pinned checksum is always right.
    const PAYLOAD: &[u8] = b"pretend onnx weights";

    fn fake_spec() -> WeightsSpec {
        WeightsSpec {
            file_name: "fake.onnx",
            model_id: "fake@1",
            url: "https://example.invalid/fake.onnx",
            sha256: Box::leak(sha256_hex(PAYLOAD).into_boxed_str()),
            size_bytes: PAYLOAD.len() as u64,
        }
    }

    #[test]
    fn sha256_hex_is_lower_and_64_chars() {
        let h = sha256_hex(b"abc");
        assert_eq!(h.len(), 64);
        assert_eq!(
            h,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn ensure_weights_downloads_verifies_and_writes_license() {
        let dir = tempfile::tempdir().unwrap();
        let spec = fake_spec();
        let calls = std::cell::Cell::new(0);
        let fetch = |_url: &str| {
            calls.set(calls.get() + 1);
            Ok(PAYLOAD.to_vec())
        };

        let path = ensure_weights(dir.path(), &spec, &fetch).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), PAYLOAD);
        // The licence notice is written beside the weights.
        let license = license_path(dir.path(), &spec);
        assert!(license.is_file());
        assert!(std::fs::read_to_string(&license)
            .unwrap()
            .contains("MIT License"));

        // Second call: present + valid -> no fetch.
        let path2 = ensure_weights(dir.path(), &spec, &fetch).unwrap();
        assert_eq!(path, path2);
        assert_eq!(calls.get(), 1, "a valid, present model is not re-fetched");
    }

    #[test]
    fn a_tampered_download_is_rejected_and_not_written() {
        let dir = tempfile::tempdir().unwrap();
        let spec = fake_spec();
        // Fetcher returns different bytes than the pinned checksum expects.
        let fetch = |_url: &str| Ok(b"malicious payload".to_vec());

        let err = ensure_weights(dir.path(), &spec, &fetch).unwrap_err();
        assert!(matches!(err, ProvisionError::ChecksumMismatch { .. }));
        assert!(
            !weights_path(dir.path(), &spec).exists(),
            "a tampered download must never be written to disk"
        );
    }

    #[test]
    fn a_corrupt_existing_file_is_replaced_by_a_fresh_download() {
        let dir = tempfile::tempdir().unwrap();
        let spec = fake_spec();
        // Pre-place a corrupt file at the target path.
        std::fs::create_dir_all(dir.path()).unwrap();
        std::fs::write(weights_path(dir.path(), &spec), b"corrupt").unwrap();
        let fetch = |_url: &str| Ok(PAYLOAD.to_vec());

        let path = ensure_weights(dir.path(), &spec, &fetch).unwrap();
        assert_eq!(
            std::fs::read(&path).unwrap(),
            PAYLOAD,
            "corrupt file replaced"
        );
    }

    #[test]
    fn fetch_failure_surfaces_as_fetch_error() {
        let dir = tempfile::tempdir().unwrap();
        let spec = fake_spec();
        let fetch = |_url: &str| Err("offline".to_string());
        let err = ensure_weights(dir.path(), &spec, &fetch).unwrap_err();
        assert!(matches!(err, ProvisionError::Fetch(_)));
    }

    #[test]
    fn resolve_library_finds_the_first_existing_candidate() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("libonnxruntime.so");
        std::fs::write(&real, b"\x7fELF fake").unwrap();
        let location = LibraryLocation {
            candidates: vec![dir.path().join("missing.so"), real.clone()],
            expected_sha256: None,
        };
        assert_eq!(resolve_library(&location).unwrap(), real);
    }

    #[test]
    fn resolve_library_verifies_a_pinned_checksum() {
        let dir = tempfile::tempdir().unwrap();
        let lib = dir.path().join("libonnxruntime.so");
        std::fs::write(&lib, b"library bytes").unwrap();

        let good = LibraryLocation {
            candidates: vec![lib.clone()],
            expected_sha256: Some(sha256_hex(b"library bytes")),
        };
        assert_eq!(resolve_library(&good).unwrap(), lib);

        let bad = LibraryLocation {
            candidates: vec![lib],
            expected_sha256: Some(sha256_hex(b"different")),
        };
        assert!(matches!(
            resolve_library(&bad).unwrap_err(),
            ProvisionError::ChecksumMismatch { .. }
        ));
    }

    #[test]
    fn resolve_library_absent_is_a_clear_error_listing_paths() {
        let location = LibraryLocation {
            candidates: vec![
                PathBuf::from("/no/such/a.so"),
                PathBuf::from("/no/such/b.so"),
            ],
            expected_sha256: None,
        };
        let err = resolve_library(&location).unwrap_err();
        match err {
            ProvisionError::LibraryNotFound { searched, env } => {
                assert!(searched.contains("/no/such/a.so"));
                assert!(searched.contains("/no/such/b.so"));
                assert_eq!(env, ORT_DYLIB_ENV);
            }
            other => panic!("expected LibraryNotFound, got {other:?}"),
        }
    }

    #[test]
    fn default_model_dir_lives_under_reprise() {
        assert!(default_model_dir().ends_with("reprise/models"));
    }

    #[test]
    fn real_htdemucs_license_file_name_is_stable() {
        assert_eq!(
            license_path(Path::new("/m"), &HTDEMUCS_FP32),
            Path::new("/m/htdemucs.onnx.LICENSE.txt")
        );
    }
}
