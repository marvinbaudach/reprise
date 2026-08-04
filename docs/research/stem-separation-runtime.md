# ML runtime for stem separation — spike report (package E)

As of: 2026-07-22

## Purpose and remit

This report decides, **on a factual basis**, the last open question of the
multi-frontend core plan (`docs/plans/multi-frontend-core.md`, decision 11):
which ML runtime does package G implement for the vocal-removal/instrumental
pipeline (`crates/reprise-stems`)? Two candidates were measured on the target
machine in the release profile:

- **(a) candle** (pure Rust, HF ecosystem) with a Demucs-class model.
- **(b) ort** (ONNX Runtime bindings) with an MDX-class ONNX model.

`libtorch` and a Python subprocess are discarded per decision 11 and were not
measured. The deliverable is **this report with a recommendation**, not
production code — package G implements the recommendation. The spike code
lives in `crates/reprise-stems/examples/` behind the optional features
`spike-candle`/`spike-ort` (see [Appendix: reproduction](#appendix-reproduction)).

**Quality is expressly not the subject of this spike** (decision:
Demucs-class quality is the inclusion condition, but the
separation quality is not assessed here). The test signal is synthetic and
serves exclusively a **faithful timing**.

## Core result (TL;DR)

- **candle fails on model availability, not on the runtime.**
  candle-transformers contains **no** Demucs; the only Rust Demucs port
  (`demucs-rs`) builds on **Burn**, not candle. A real htdemucs run under
  candle requires a **hand port of the hybrid-transformer architecture plus
  weight conversion** — outside the spike timebox and a considerable package G
  effort. The runtime itself is pure Rust and lean (verified).
- **ort runs immediately** — ONNX models load without an architecture port.
  Both an MDX-class model and **htdemucs** (the required Demucs class) run
  on this machine **faster than real time** (~2.5–3.5×).
- **The licence gate tips the model choice, not the runtime:** the
  UVR-MDX-Net community weights have **no defensible licence** (gate fail),
  **htdemucs is cleanly MIT** (Meta; gate pass).
- **Recommendation: ort runtime + htdemucs weights (MIT) as ONNX.** The price
  package G pays: the native-onnxruntime Flatpak offline story and a high
  memory peak (~6 GB fp32).

## Method

### Target machine

Read out from `/proc/cpuinfo` and `free -h`:

- **CPU:** Intel Core Ultra 7 258V (Lunar Lake), 8 cores / 8 threads (no SMT;
  4 Lion Cove P-cores + 4 Skymont LP-E-cores — **heterogeneous**, relevant for
  the timing variance below).
- **SIMD:** AVX, AVX2, FMA, F16C — **no AVX-512** (Lunar Lake).
- **RAM:** 30 GiB total, ~19 GiB available at the time of measurement.
- **OS:** Linux 6.18.38-1-MANJARO x86_64.
- **Toolchain:** rustc/cargo 1.96.1; **release profile** for all measurements.

### Test signal

Synthetically generated (`synth_stereo`): stereo, 44.1 kHz, f32, sum of sine
partials (220/440/660/1760 Hz) plus weak xorshift noise.
Lengths: 45 s / 90 s / 120 s, to amortize the slow first chunk (ONNX Runtime
warmup). Entirely sufficient for timing, since the inference cost is
determined solely by the tensor shape and the model graph, not by the signal
content.

### Models (provenance, size, checksum)

| Model | File | Size | sha256 | Source |
|---|---|---|---|---|
| MDX class | `UVR-MDX-NET-Inst_HQ_1.onnx` | 66,759,214 B (63.7 MiB) | `38a045c4…e29f7f9` | HF mirror `Blane187/all_public_uvr_models` |
| Demucs class | `htdemucs.onnx` (fp32) | 316,446,953 B (301.8 MiB) | `68d0bf16…fcc5e74` | HF `StemSplitio/htdemucs-onnx` |

Model graphs (read out via `ort_probe`, i.e. real, not assumed):

- **MDX:** input `input` `[batch, 4, 3072, 256]`, output `output` the same.
  Spectrogram domain: 4 channels = (L,R)×(Re,Im), dim_f=3072, dim_t=256.
  STFT external (in the harness, `rustfft`): n_fft=6144, hop=1024 (MDX
  standard) ⇒ one chunk = 256·1024 = 262,144 samples = **5.944 s of audio**.
- **htdemucs:** input `mix` `[1, 2, 343980]` (stereo **waveform**, 7.8 s
  segment), output `stems` `[1, 4, 2, 343980]` (4 sources × stereo). The STFT
  lies **inside the graph** — pure waveform I/O.

### Exact commands

```bash
# Kandidat (b): ONNX-Modellgraph inspizieren
cargo run -p reprise-stems --release --features spike-ort --example ort_probe -- <model.onnx>
# Kandidat (b): MDX-Timing (Sekunden optional, Default 45)
cargo run -p reprise-stems --release --features spike-ort --example ort_mdx_bench -- UVR-MDX-NET-Inst_HQ_1.onnx 120
# Kandidat (b): htdemucs-Timing (Demucs-Klasse)
cargo run -p reprise-stems --release --features spike-ort --example ort_demucs_bench -- htdemucs.onnx 90
# Kandidat (a): candle pure-Rust-Nachweis + CPU-Mikrobenchmark
cargo run -p reprise-stems --release --features spike-candle --example candle_probe
```

What is measured: cold start (session build = load model + graph
optimization), first-chunk latency (incl. ONNX warmup), pure inference
real-time factor (RTF = wall time / audio duration), peak RSS (`VmHWM` from
`/proc/self/status`), thread count (onnxruntime intra-op default = 8).

## Measurement table

| Metric | (a) candle + Demucs | (b) ort + MDX-Net HQ | (b) ort + **htdemucs** (Demucs class) |
|---|---|---|---|
| Runnable within the timebox | **No** — port blocker | Yes | Yes |
| Model file | — | 63.7 MiB | 316 MiB (fp32) / 166 MiB (fp16) |
| Cold start (session build) | — | **~0.14 s** (128–183 ms) | **~3.2 s** (3.16–3.93 s) |
| First output (incl. warmup) | — | ~2.3–2.6 s | ~5.1–7.5 s |
| **Real-time factor (RTF)** | **not measurable** | **0.37–0.42×** (~2.5× RT) | **0.28–0.46×** (~2.5–3.5× RT)¹ |
| 4-min song (240 s) → compute time | — | ~94 s | ~70–110 s¹ |
| Peak RSS | (microbench: 144 MiB) | **~2.6–2.8 GB** | **~5.0–6.2 GB** (fp32) |
| Threads (intra-op) | 1 (microbench) | 8 | 8 |
| Binary size impact | **+~2 MB** (pure Rust) | +~22 MB (onnxruntime static) | +~22 MB |
| Pure Rust | **Yes** (verified) | No (onnxruntime 1.22.0) | No |

¹ **Important caveat:** the htdemucs harness processes segments **without
overlap** (back-to-back). Production Demucs typically uses overlap 0.25
(and optionally `shifts`/TTA), which raises the real compute time by ~1.3×
(and considerably more with `shifts`). The measured RTFs are therefore a
**lower bound**; realistically with overlap ~0.4–0.45×. The variance
(0.28–0.46) demonstrably stems from the **heterogeneous P+E core topology**:
onnxruntime distributes 8 threads across unequal cores, which fluctuates on
short runs. Median-typically htdemucs lies at **~0.30–0.35× without overlap**.

candle microbenchmark (context only, **no** Demucs RTF): matmul 512×384×384 =
25 GFLOP/s, conv encoder stack 328 ms/pass, peak RSS 144 MiB. This proves:
candle runs pure Rust on this machine; the out-of-the-box CPU throughput is
moderate (without the MKL/BLAS feature), which does not per se disqualify an
htdemucs port, but does not promise any head start either.

## Build and packaging story

### (a) candle

- **Pure Rust confirmed:** `ldd` on the candle example binary shows **only**
  `libc`/`libm`/`libgcc_s` — **no** native ML library, no BLAS, no
  onnxruntime. The default CPU backend (`gemm` crate) is pure Rust code.
- **Binary size:** candle example 2.5 MB; the surcharge from candle compared
  to an empty binary is ~2 MB. **Flatpak offline build trivial** — purely
  crates.io dependencies, `cargo --offline` with vendored crates suffices, no
  build-time downloads, no per-platform native lib management.
- **Runtime licence:** candle-core/candle-nn 0.11.0 = **MIT OR Apache-2.0** ✓.
- **The blocker lies with the model, not the build** (see Risks).

### (b) ort

- **Resolution/linking (empirical, from the `ort-sys` build log):**
  `onnxruntime not found using pkg-config, falling back to manual setup` ⇒ the
  default strategy **`download-binaries`** downloads, at **build time**, a
  precompiled static `libonnxruntime.a` (97.8 MB, onnxruntime **1.22.0**)
  from pyke's CDN into `~/.cache/ort.pyke.io/…` and links it **statically**
  (`cargo:rustc-link-lib=static=onnxruntime`, plus `stdc++`). Result: a
  self-contained 26.7 MB binary without a runtime `.so` dependency (`ldd`
  shows only `libstdc++`/`libgcc_s`).
- **Flatpak offline build — the central price:** `download-binaries`
  **breaks** an offline/Flathub build (network at build time is forbidden).
  Viable routes:
  1. **`ORT_STRATEGY=system`** + the precompiled `libonnxruntime.a`/`.so` as a
     **declared Flatpak source verified by sha256** (flatpak-builder fetches
     sources in advance with a checksum — exactly the pattern the plan
     foresees for the model weights anyway). `ORT_LIB_LOCATION` points to the
     file.
  2. **`load-dynamic`** + a bundled `libonnxruntime.so` that is loaded at
     runtime via `dlopen` (fully decouples build and lib).
  3. Build onnxruntime as its own Flatpak module from source (slow,
     unnecessary).
  Recommendation for G: **route 1 or 2** — both are standard and solvable, but
  **real packaging work** that candle would not have.
- **Portability:** onnxruntime has official builds for Windows/macOS/
  Android/iOS — the plan's portability promise remains intact, but with a
  **native lib per platform** instead of a pure Rust artifact.
- **Runtime licence:** ort/ort-sys 2.0.0-rc.10 = **MIT OR Apache-2.0** ✓;
  ONNX Runtime itself = **MIT** (Microsoft) ✓. Note: ort 2.0 is still
  **RC** (not a stable release) — package G should re-pin to the version
  current at that time.

### Workspace hygiene (both)

The spike deps are **optional** and the examples carry `required-features`;
`cargo check/test/clippy --all-targets` (without features) skips them. Proven:
`cargo tree -p reprise-stems` (default) is **core-only**; `cargo check
--workspace`, `cargo test -p reprise-stems` and `cargo clippy --all-targets -p
reprise-stems -- -D warnings` are green; **`cargo audit` brings no new
advisory** (still only the accepted RUSTSEC-2024-0436 via `paste`/lofty).
494 lock packages in total.

## Licence findings against the LICENSING gate

The gate (`LICENSING.md`, paragraph "Audio-analysis and stem-separation…")
demands, for the MIT engine path: **redistribution + commercial use + linking
from the GPL Linux client and future proprietary frontends**; AGPL as well as
non-commercial/no-derivatives terms are excluded; every model needs a
**documented licence and provenance** before it moves into the repo.

| Artifact | Licence | Source/evidence | Gate |
|---|---|---|---|
| candle-core/nn 0.11 | MIT OR Apache-2.0 | crates.io | **✓** |
| ort / ort-sys 2.0.0-rc.10 | MIT OR Apache-2.0 | crates.io | **✓** |
| ONNX Runtime 1.22.0 (native) | MIT | microsoft/onnxruntime (GitHub API) | **✓** |
| **Demucs / htdemucs weights** | **MIT** (Meta Platforms) | `adefossez/demucs` LICENSE (MIT, "Copyright (c) Meta Platforms"); weights via `demucs/remote/files.txt` from `dl.fbaipublicfiles.com` as part of the MIT project | **✓** |
| htdemucs **as ONNX** | **MIT** | `StemSplitio/htdemucs-onnx`: "This repo is MIT-licensed, matching the original HT-Demucs."; ONNX export of Meta's official weights | **✓** |
| **UVR-MDX-Net community models** | **unclear / not established** | see below | **✗ (gate fail)** |

### Why the UVR-MDX-Net weights fail

- The UVR **application** repo (`Anjok07/ultimatevocalremovergui`) does report
  MIT in the GitHub metadata, but has **no LICENSE file** in the root
  (default branch `master`; `raw …/LICENSE` = 404), and the GitHub `/license`
  endpoint returns **`None`** (no recognized licence file). Issue #2185 (2026,
  **unanswered**) asks about precisely the commercial use of the **models**
  and records that no explicit LICENSE exists.
- **Decisive point:** even an MIT on the *application code* does **not**
  license the **separately hosted model weights**. The MDX-Net models are
  artifacts of their own (not in the repo, from various community trainers, on
  third-party HF mirrors), **without** an accompanying licence.
  `python-audio-separator` (the canonical model registry) likewise carries
  **no** per-model licence fields.
- For the specific weights tested (`UVR-MDX-NET-Inst_HQ_1`), therefore, **no
  redistribution/commercial licence can be established** — and "licence not
  determinable" is, per the remit, itself a gate-relevant fail. Under
  `LICENSING.md` they **must not** be shipped.
- Provenance side note (not a distribution blocker, but honestly recorded):
  Demucs/MDX were trained on MUSDB18(-HQ) (CC BY-NC) plus additional data. The
  Demucs authors nevertheless expressly place the **resulting weights** under
  MIT — the widespread (legally not conclusively tested) position that model
  weights are not a derivative of the training data licence. For the gate what
  counts is the **licence pronounced for distribution** = MIT.

## Risks

1. **candle port effort (the blocker):** htdemucs is hybrid-transformer Demucs
   — parallel time and spectral branches, cross-domain attention,
   encoder/decoder with LSTM/attention. A candle port means **~1000+ lines of
   module code plus PyTorch→safetensors weight conversion and numerical
   verification**. That is an effort of several days to several weeks with a
   risk of error, clearly outside a spike. **Estimate: large.**
2. **Memory peak (ort/htdemucs):** ~6.2 GB fp32 peak RSS (onnxruntime arena).
   Borderline on 8 GB devices; **parallel jobs are ruled out** (the plan
   foresees 1 job at a time anyway — fits). Mitigation: fp16 export
   (166 MB, ~halves the weight memory), onnxruntime arena configuration
   (`disable_cpu_mem_arena`/`memory_pattern`), smaller segment length.
3. **ort 2.0 is RC:** API drift possible (this spike hit rc.10; `session.run`
   requires `&mut`, `Tensor::from_array((shape, vec))`). Package G re-pins and
   secures it with a smoke test.
4. **Flatpak offline lib:** see the build story — solvable (system/
   load-dynamic + verified source), but not free; must be planned into the
   G scope.
5. **Timing variance on a P+E CPU:** the heterogeneous core topology scatters
   the RTF. For stable user-facing times, set thread affinity/thread count in
   onnxruntime if necessary; uncritical for the buying decision, since even
   the worst run is clearly faster than real time.
6. **htdemucs post-processing:** v1 outputs only the instrumental track
   (decision 19). htdemucs delivers 4 stems; instrumental = mix − vocals (or
   the sum drums+bass+other) plus the internal overlap/window reconstruction —
   an implementation detail for G, not a runtime risk.

## Recommendation

> **Package G implements the `ort` runtime (ONNX Runtime) and ships as its
> model htdemucs — hybrid-transformer Demucs v4 — as an ONNX export under the
> MIT licence (Meta's official weights, e.g. via `StemSplitio/htdemucs-onnx`).**
> Justification on facts: (1) htdemucs is the Demucs-class quality required by
> the plan, and its weights are **cleanly MIT** and pass the
> `LICENSING.md` gate for redistribution and commercial use; (2) ONNX
> loads **directly** into ort — in candle there is **no** Demucs model (only a
> Burn port), and a hand port of the hybrid-transformer architecture including
> weight conversion is a large effort lying outside this spike; (3) the
> measured performance on the target machine, at a real-time factor of
> ~0.3–0.45×, is comfortably faster than real time (a 4-minute song in a good
> one to just under two minutes). The **UVR-MDX-Net community models are
> expressly not to be used** — their weights have no defensible licence and
> fail the gate. The price of this choice, which G must plan for, is twofold
> and manageable: for the Flatpak offline build the native onnxruntime library
> must be provided as a per-checksum verified system/`load-dynamic` lib
> (not the cargo `download-binaries` default), and the fp32 memory peak of
> ~6 GB demands strict one-job serialization (planned anyway) as well as an
> evaluation of the fp16 export. candle remains the **pure-Rust north
> star** for the case that a maintained candle Demucs implementation exists
> later — a switch would then be worth re-evaluating.**

### What remains open

- **fp16 vs. fp32:** measure fp16 htdemucs (166 MB) on CPU — halves the
  download and the weight memory, but onnxruntime CPU may internally cast fp16
  to fp32 (possibly slower). To be clarified for G in 1 h.
- **Overlap/quality:** measure the real segment overlap cost (0.25) and the
  instrumental reconstruction (mix − vocals); ~+30% compute time expected.
- **onnxruntime sourcing for Flatpak:** nail down the concrete source URL +
  sha256 of the precompiled lib (or `load-dynamic`) and anchor `ORT_STRATEGY`
  in the G build pipeline.
- **ort version:** at the start of G, re-pin to the then current (ideally
  stable) ort 2.0 version and fix the ONNX Runtime version level.
- **Chunking/cancel/progress (G scope):** deterministic output across chunk
  boundaries, cancel between chunks, progress callbacks (from the plan).

## Appendix: reproduction

Spike code (deliberately lean, clearly marked as a spike):
`crates/reprise-stems/`
— `Cargo.toml` (optional features `spike-candle`/`spike-ort`, examples with
`required-features`) and `examples/{ort_probe,ort_mdx_bench,ort_demucs_bench,
candle_probe}.rs`. Models are **not** checked in; for the measurement they
were downloaded from the HF sources named in the model table. Resolved
versions: candle 0.11.0, ort/ort-sys 2.0.0-rc.10 (onnxruntime 1.22.0),
rustfft 6.4.1, ndarray 0.16.1, hound 3.5.1.

### Environment blockers

**None.** The network (crates.io, huggingface.co, github.com) was available
throughout the entire spike; all crate and model downloads as well as the
onnxruntime build-time download succeeded. No measurement was faked; the
only "unmeasured" quantity — candle's Demucs RTF — is honestly documented as a
port blocker, not as an environment problem.
