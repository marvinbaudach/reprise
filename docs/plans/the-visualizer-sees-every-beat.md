---
slug: the-visualizer-sees-every-beat
worktree: /home/marvin/Projects/reprise-the-visualizer-sees-every-beat
branch: feature/the-visualizer-sees-every-beat
phase: planned
codex_session:
created: 2026-09-08
---
# The visualizer sees every beat

## Why

Measured 2026-09-08, full evidence in
`docs/plans/desktop-visualizer-is-starved-on-flac.findings.md`.

The desktop visualizer's data rate **is** the decoder's block rate: the CAVA
branch re-chunks nothing, and `player_pipeline.rs:317-352` emits exactly one
`PlayerEvent::Spectrum` per GStreamer buffer.

| Format | Samples/buffer | Bar updates |
|---|---|---|
| FLAC (4096 / 4608) | 4096 / 4608 | **10.8 / 9.6 Hz** |
| MP3 | 1152 | 38.3 Hz |
| AAC | 1024 | 43.1 Hz |

The engine is tuned for `CAVA_REFERENCE_FRAMERATE = 66.0`. All 400 sampled FLACs
in this library are 4096 or 4608 and the library is 80% FLAC, so the normal case
runs at a seventh of the design rate. A 60 Hz renderer interpolating 10 Hz of
data is the reported symptom: fluid, but late and soft on beats.

Second defect, same root: the main FFT is 2048 and reads the newest 2048 samples
of a 4096-sample sliding window (`cava.rs:154`, `push_samples` at `cava.rs:186`).
Once the hop exceeds 2048 the windows leave **gaps** — at hop 4608, 55.6% of the
audio never enters the main FFT (at 4096: 50%), plus 512 samples per block
discarded by the `min(4096)` clamp. A kick in a gap is not softened; it is
invisible.

Android does not have this bug: `PcmRingBuffer`
(`reprise-android-ffi/src/visualizer.rs:100-134`) is drained at display cadence
in ~432–528-sample hops. This plan does not touch Android.

## Goal

The desktop visualizer updates at ~60 Hz and analyses every sample, on every
codec.

## Non-goals

- Android — already correct.
- The stored-spectrogram / spectral seek bar path.
- Retuning CAVA's smoothing constants. They are tuned for ~66 Hz; the point is
  to deliver that rate, not to compensate for its absence.
- Explaining "why it feels new". Recorded as unresolved in the findings file
  (candidate `425ec78212` / #826, untested).
- **Decoupling the analysis branch from playback.** `build_audio_filter` builds
  one bin carrying both, so a failure in the analysis branch can take down
  playback. That coupling is pre-existing, not introduced here, and fixing it is
  a separate change.

## Decision: `audiobuffersplit`, as a hard dependency

`audiobuffersplit output-buffer-duration=1/60` in the CAVA branch. Measured on a
4608-blocksize FLAC: **13537 buffers x 735 samples = 60.0 Hz**, with PTS spaced
exactly 16.666 ms apart and `duration: 0:00:00.016666667`. Because the appsink is
already `sync(true)`, those timestamps are what paces delivery — **alignment is
guaranteed by the pipeline clock, not by logic we write.**

Rejected alternative: a PCM ring buffer with a 60 Hz drain thread, ported from
Android (~200 lines: thread, ring, consumption controller, three reset points).
It solves by hand what GStreamer already solves, and invents the drift and
latency risk that `sync(true)` otherwise gives for free.

**The dependency is declared, not worked around.** `audiobuffersplit` ships in
gst-plugins-bad, which is currently `optdepends` in `packaging/aur/PKGBUILD:33`
and is not installed by CI (`ci.yml:189,234,291`). It **is** already present in
`org.gnome.Platform` 50 (verified: `libgstaudiobuffersplit.so` in the installed
runtime), so the Flatpak — the primary channel — costs nothing.

A soft fallback (link the chain without the splitter when the element is missing)
was considered and rejected: it creates a second, silently untested operating
mode. One code path, consistent with all 19 existing `ElementFactory::make` sites.

The one hardening this requires: the failure must name the missing package, not
surface as the generic `GStreamer: {error}`.

## Tasks

1. **`player_effects.rs`** — insert into the CAVA branch, between `cava_resample`
   and `cava_sink`:
   - an explicit `capsfilter` carrying the existing `cava_caps` (today those caps
     live only on the appsink; the explicit element mirrors the measured
     configuration and removes negotiation ambiguity),
   - `audiobuffersplit` with `output-buffer-duration = 1/60` (a `gst::Fraction`;
     the element's own default is 1/50).
   Add both to `all_elements` and to the `link_many` chain. **Leave the
   appsink's own `caps` property in place** — carrying the same caps on both the
   capsfilter and the sink is harmless, and removing it as "duplication" would
   change negotiation. Leave `gapless` at
   its default `false`: a real discontinuity should raise `DISCONT`, which the
   callback already answers with `processor.reset()` — cheap since #826.
2. **Error message** — when `make("audiobuffersplit")` fails, return a
   `PlaybackError::Backend` naming gst-plugins-bad, rather than the shared
   `format!("GStreamer: {error}")`.
3. **`packaging/aur/PKGBUILD`** — add `'gst-plugins-bad'` to `depends` and
   **remove** it from `optdepends:33`; listing it in both is contradictory.
4. **`.github/workflows/ci.yml`** — add `gst-plugins-bad` to the package lists on
   lines 189, 234 and 291.
5. **`cava.rs`** — pin the oversized-input contract: `process_into` silently keeps
   only the newest `min(len, 4096)` samples. Document it on the public method and
   pin it with a test. Deliberately *not* internal hopping — once the split lands,
   no caller passes more than 735, so hopping inside would be speculative.
6. Tests, below.

## Tests

The seams already exist. `cava_tests.rs` runs real playbin tests that observe
`PlayerEvent::Spectrum` through an injected channel
(`Player::new(Box::new(move |event| tx.send(event)))`) under
`REPRISE_AUDIO_SINK=fakesink`; `build_audio_filter` is reachable in-crate; the
gate runs `cargo test -p reprise-platform-linux -- --test-threads=1`.

**T1 — the rate regression (the test that would have caught this).**
The existing fixture `crates/reprise-core/tests/fixtures/sine.flac` is exactly the
worst case: **blocksize 4608**, 51200 samples = 1.161 s. Play it with spectrum
enabled and count `PlayerEvent::Spectrum` to EOS.
- today: 51200/4608 ≈ **11 frames** (~9.6 Hz)
- after the fix: 1.161 s x 60 ≈ **70 frames**
Assert a threshold clear of both, e.g. `>= 40`. Runs in ~1.2 s.

**Termination is `PlayerEvent::TrackFinished`**, which arrives on the same
injected channel the `Spectrum` frames do (`playback.rs:274`). Count until it
lands, then assert. A wall-clock timeout exists only as a **failure** guard — it
must fail the test, never end the count, or the assertion silently becomes
wall-clock-shaped instead of frame-shaped.

Verify it goes **red before the fix** — a regression test that was never red
proves nothing.

(Note for future readers: `>= 40` discriminates for *this* fixture, not in
general. A 1.161 s MP3 clip already yields ~44 frames today. The threshold works
because the fixture is a 4608-blocksize FLAC.)

**T2 — structural.** `build_audio_filter` produces a bin containing an
`audiobuffersplit` whose `output-buffer-duration` is 1/60. Cheap, and catches a
silent removal or a mistyped fraction that T1 might still pass on some codec.

**T3 — the gap defect, in core, fast.** Place a transient in the sample range
that a single 4608-sample call skips (everything but the newest 2048). Assert it
is visible when the same audio is fed in 735-sample hops and invisible when
passed as one block. This encodes the second defect precisely.
Control arm: place the same transient in the **newest 2048 samples** of the 4608
block and assert it is visible in *both* paths. That proves the test detects the
transient's **position** rather than merely reacting to hop size — comparing two
735-hop runs would be tautological, since both are the same code path.

**T4 — the contract from task 5.** Feeding 8192 samples yields the same bars as
feeding only the newest 4096.

No skip-if-plugin-missing guard (the `fingerprint_tests.rs:263-268` pattern) is
added: under a declared hard dependency, a missing plugin *should* fail loudly.

## Verification beyond the suite

- Re-run the findings file's probe against the built binary: emitted Spectrum
  rate on a **FLAC** track ~60 Hz. **Control arm:** an **MP3** track, already at
  38 Hz, must not regress.
- Check the cost: 60 main FFTs/s (2048) + 60 bass FFTs/s (4096) instead of ~10.
  Cheap in absolute terms but unmeasured — confirm it does not show up on the
  streaming thread.
- **Ear check.** The reported lag on FLAC is gone. The numbers are necessary but
  not sufficient; this is the only test that speaks to the complaint.

## Risks

- **The dependency is now load-bearing for playback**, because the analysis
  branch shares the bin (see non-goals). Under a declared `depends` this is
  correct behaviour, but it makes task 3 mandatory, not cosmetic — shipping the
  code without the PKGBUILD change breaks playback for AUR users lacking the
  optional package.
- **T1 is a timing-shaped test.** It counts frames, not wall-clock, and the
  appsink's `sync(true)` throttles the clip to roughly realtime; with
  `--test-threads=1` that is ~1.2 s. If it proves flaky, widen the band rather
  than deleting the assertion — the rate is the thing under test.

## Parallelität

**The cut is not worth taking. One strand.**

With `audiobuffersplit` chosen over the ring buffer, the whole change is roughly
twenty lines of Rust plus four lines of packaging and CI. The candidate split
("GStreamer chain in `reprise-platform-linux`" vs "core contract in
`reprise-core`") has genuinely disjoint file groups —
`crates/reprise-platform-linux/src/player_effects.rs` + `cava_tests.rs` against
`crates/reprise-core/src/playback/cava.rs` + `cava_tests.rs` — but task 5 is a
doc comment and one test, and it is conditional on what task 1 guarantees about
hop size. Two worktrees and two Codex runs cost more wall-clock than the task
contains, and would force that dependency to be resolved twice with no way to
compare the answers.

Merge order: n/a. Post-merge cross-checks: n/a.
