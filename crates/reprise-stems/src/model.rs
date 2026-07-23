//! The model identity and weight artifacts this backend provisions.
//!
//! The spike (`docs/research/stem-separation-runtime.md`) picked **htdemucs**
//! (Hybrid-Transformer Demucs v4) exported to ONNX, whose weights are cleanly
//! MIT (Meta Platforms) and pass the `LICENSING.md` model gate. Weights are
//! **never** bundled into the build or the Flatpak; they arrive through a
//! first-use download that verifies a SHA-256 and records the licence notice
//! next to the file (plan 2.4.9).

/// The stable `"<name>@<version>"` identity of the output this backend
/// produces. It is stored verbatim as the job dedup `params_fingerprint` and as
/// the `REPRISE_AI_MODEL` provenance tag, so it **must change whenever the
/// produced result would change** — different weights, precision, overlap-add
/// window or source reduction all warrant a new identity.
///
/// `4` is the Demucs v4 (Hybrid Transformer) architecture; the default build
/// uses the fp32 weights ([`HTDEMUCS_FP32`]).
pub const MODEL_ID: &str = "htdemucs@4";

/// A downloadable set of ONNX weights: where to fetch it, how large it is, and
/// the SHA-256 that must match before it is trusted.
#[derive(Debug, Clone, Copy)]
pub struct WeightsSpec {
    /// File name the weights are stored under in the model directory.
    pub file_name: &'static str,
    /// The `"<name>@<version>"` identity produced with these weights.
    pub model_id: &'static str,
    /// HTTPS source (a Hugging Face LFS `resolve` URL).
    pub url: &'static str,
    /// Lower-case hex SHA-256 of the exact file at `url`.
    pub sha256: &'static str,
    /// Expected size in bytes (a cheap pre-check before hashing).
    pub size_bytes: u64,
}

impl WeightsSpec {
    /// The file name of the licence notice written beside these weights.
    pub fn license_file_name(&self) -> String {
        format!("{}.LICENSE.txt", self.file_name)
    }
}

/// The fp32 htdemucs export — the default, parity-verified weights
/// (`StemSplitio/htdemucs-onnx`). ~316 MB; ~6 GB peak RSS at inference, which
/// is why the plan serialises stem jobs one at a time.
pub const HTDEMUCS_FP32: WeightsSpec = WeightsSpec {
    file_name: "htdemucs.onnx",
    model_id: MODEL_ID,
    url: "https://huggingface.co/StemSplitio/htdemucs-onnx/resolve/main/htdemucs.onnx",
    sha256: "68d0bf16428ef66e692cdff8a9ccf28f1ef3f69440d57e58605a4cc55fcc5e74",
    size_bytes: 316_446_953,
};

/// The fp16-weights htdemucs export — a documented, checksummed alternative
/// that halves the download (~166 MB). onnxruntime's CPU EP may up-cast fp16 to
/// fp32 internally, so its runtime memory/latency match fp32 (per the model
/// card); it is **not** the default because it produces slightly different
/// numbers and so would need its own `model_id` before shipping (see the crate
/// docs' fp16 note).
pub const HTDEMUCS_FP16: WeightsSpec = WeightsSpec {
    file_name: "htdemucs_fp16weights.onnx",
    // A distinct identity: fp16 output differs numerically from fp32, so it must
    // never be tagged as the fp32 model.
    model_id: "htdemucs@4-fp16",
    url: "https://huggingface.co/StemSplitio/htdemucs-onnx/resolve/main/htdemucs_fp16weights.onnx",
    sha256: "d05c269d0178d2a72ad484b10b11dd370193fc923201c3b27a99f848745db70a",
    size_bytes: 165_612_636,
};

/// The licence notice written next to the downloaded weights. It records the
/// model, its provenance and its MIT terms so the file's licence travels with
/// it on disk (plan 2.4.9), independent of this repository.
pub const HTDEMUCS_LICENSE_NOTICE: &str = "\
htdemucs (Hybrid-Transformer Demucs v4) — ONNX export
======================================================

Model weights: MIT License.
Copyright (c) Meta Platforms, Inc. and affiliates.

Origin
------
Architecture and original weights: Demucs (https://github.com/adefossez/demucs),
released by Meta Platforms under the MIT License.
ONNX export: StemSplitio/htdemucs-onnx
(https://huggingface.co/StemSplitio/htdemucs-onnx), stated MIT, matching the
original HT-Demucs.

These weights were downloaded on first use and verified by SHA-256. They are
NOT part of the Reprise source distribution or its Flatpak. Redistribution and
commercial use are permitted under the MIT License; retain this notice.

MIT License
-----------
Permission is hereby granted, free of charge, to any person obtaining a copy of
this software and associated documentation files (the \"Software\"), to deal in
the Software without restriction, including without limitation the rights to
use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software is furnished to do so,
subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_id_is_a_name_at_version() {
        let (name, version) = MODEL_ID.split_once('@').expect("must be name@version");
        assert_eq!(name, "htdemucs");
        assert!(!version.is_empty());
    }

    #[test]
    fn fp32_is_the_default_identity_and_fp16_differs() {
        assert_eq!(HTDEMUCS_FP32.model_id, MODEL_ID);
        assert_ne!(
            HTDEMUCS_FP16.model_id, HTDEMUCS_FP32.model_id,
            "fp16 output differs numerically, so it needs its own identity"
        );
    }

    #[test]
    fn specs_carry_a_full_lower_hex_sha256() {
        for spec in [HTDEMUCS_FP32, HTDEMUCS_FP16] {
            assert_eq!(spec.sha256.len(), 64, "sha256 is 32 bytes = 64 hex chars");
            assert!(
                spec.sha256
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
                "sha256 must be lower-case hex"
            );
            assert!(spec.size_bytes > 0);
            assert!(spec.url.starts_with("https://"));
        }
    }

    #[test]
    fn license_notice_names_the_license_and_holder() {
        assert!(HTDEMUCS_LICENSE_NOTICE.contains("MIT License"));
        assert!(HTDEMUCS_LICENSE_NOTICE.contains("Meta Platforms"));
        assert!(HTDEMUCS_LICENSE_NOTICE.contains("SHA-256"));
    }

    #[test]
    fn license_file_name_is_derived_from_the_weights() {
        assert_eq!(
            HTDEMUCS_FP32.license_file_name(),
            "htdemucs.onnx.LICENSE.txt"
        );
    }
}
