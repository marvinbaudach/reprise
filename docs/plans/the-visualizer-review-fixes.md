---
slug: the-visualizer-review-fixes
worktree: /home/marvin/Projects/reprise-visualizer-review-fixes
branch: feature/visualizer-review-fixes
phase: planned
codex_session:
created: 2026-09-09
---
# The visualizer analysis keeps every sample it is handed

## Why

Follow-up to #895 (`157d3c74b3`, "The visualizer sees every beat"), which landed
without a code review. The review afterwards found one packaging defect and one
behaviour change that the plan had mislabelled as a documentation task.

### The mislabelled change

#895's task 5 claimed `process_into` "silently keeps only the newest
`min(len, 4096)` samples" and asked only for a doc comment and a test. That
premise was wrong. The clamp in place before #895 was
`mono_samples.len().min(self.input_buffer.len())`, and `input_buffer` is
`fft_size * 2` (`cava.rs:117`) — **8192** samples at 44.1 kHz, not 4096.

Adding `.min(MAX_INPUT_SAMPLES)` therefore halved what the processor accepts.
The main FFT does not notice (it reads `input_buffer[..main_fft.len()]`
regardless), but the **bass FFT runs over the full 8192** (`cava.rs:158-160`),
so every push larger than 4096 now loses fresh low-frequency content that
reached it before.

The desktop is not exposed: after #895 the CAVA branch delivers 735-sample
buffers. `reprise-android-ffi/src/visualizer.rs` is, and it shares this
processor — its drain can hand over ~22 000 samples in one call when the UI
thread stalls.

### The same clamp lies to the smoother

`new_samples` is the smoother's elapsed-time proxy: `Smoother::update_framerate`
(`cava/smoothing.rs:156-165`) derives `framerate ≈ sample_rate_hz / new_samples`
from it. Clamping it makes the smoother believe less time passed than really
did, so decay and gravity run too fast — worst exactly during the stalls that
produce oversized calls.

### The constant is right only by coincidence

`MAX_INPUT_SAMPLES = 4096` happens to equal `main_fft.len()` in the 32.5–75 kHz
bracket (`cava/bands.rs:57-65`). At 16–32 kHz the main FFT is 2048 while the
constant stays 4096, so a 3000-sample push would again land partly outside the
main FFT's window — the very defect #895 set out to fix, reappearing at another
sample rate. `CavaConfig::new` is public and takes an arbitrary rate.

## Goal

The processor analyses every sample it is handed, at any input size and any
sample rate, and reports elapsed time to the smoother truthfully.

## Non-goals

- Retuning CAVA's smoothing constants.
- Decoupling the analysis branch from playback in the GStreamer bin. Still
  pre-existing, still a separate change.
- A CI gate for PKGBUILD/`.SRCINFO` parity. Worth having, but it is new
  machinery rather than a fix to this change.

## Decision: hop internally instead of clamping

#895 rejected internal hopping as speculative, on the stated ground that "once
the split lands, no caller passes more than 735". That ground does not hold —
the Android FFI is a caller and does pass more. With a real caller the hop is
not speculative, and it is the only option that fixes all three findings at
once: nothing is discarded, every hop reports its own true length to the
smoother, and the hop size follows the FFT instead of a hardcoded number.

Rejected alternative: reverting to `min(input_buffer.len())`. It restores the
Android bass content but leaves oversized pushes skipping part of the main FFT
window, which is exactly the second defect #895 was written to remove.

## Tasks

1. **`cava.rs` — hop over the input.** Extract the body of `process_into` (push,
   both FFTs, band fill, `smoother.apply`) into a private per-chunk step, and
   have `process_into` run it over `mono_samples` in consecutive chunks of at
   most `self.main_fft.len()`, oldest first. `bars` is overwritten per chunk;
   the last chunk wins. Each chunk passes **its own length** as `new_samples`.
   - **Preserve the empty-input behaviour.** Today `process_into(&[], bars)`
     still fills `bars` from the buffered history. `chunks()` yields nothing for
     an empty slice, so run exactly one step in that case.
   - Delete `MAX_INPUT_SAMPLES`; the hop size is `self.main_fft.len()`, and
     `push_samples` needs no separate cap once no chunk exceeds it.
   - Rewrite the doc comment: it currently documents discarding, which stops
     being true.
2. **`cava_tests.rs` — retarget the contract test.** T4
   (`oversized_input_is_equivalent_to_its_newest_four_thousand_ninety_six_samples`)
   pins the discarding behaviour and must go. Replace it with the opposite
   contract: feeding N samples in one call equals feeding the same N as
   consecutive hops. Cover a size above one FFT window (8192) and a size that
   is not a multiple of the hop, so the tail chunk is exercised.
   - Keep the gap test and its control arm as they are.
   - Add a case at a **different sample rate** whose FFT bracket is 2048
     (e.g. 22 050 Hz), so L1's defect cannot come back unnoticed.
3. **`player_effects.rs` — name the analysis sink's queue depth.** `max_buffers(2)`
   with `drop(true)` now sees ~60 buffers/s instead of ~10. A short scheduling
   hiccup silently drops buffers, and because the drop happens inside the sink
   no `DISCONT` reaches the callback, so `processor.reset()` never runs and the
   ring buffer takes an unflagged hole. Raise the depth to a small named
   constant and say in a comment why it is bounded.
   - **Keep `drop(true)`.** The analysis branch shares the bin with playback;
     it must never back-pressure the streaming thread. Bounded loss is the
     correct trade, a stalled player is not.
4. **`packaging/aur/.SRCINFO` — regenerate.** It still carries the pre-#895
   state: `gst-plugins-bad` under `optdepends`, absent from `depends`.
   `packaging/aur/README.md:7-12` requires `makepkg --printsrcinfo > .SRCINFO`
   after every PKGBUILD change. Use that command; if `makepkg` is unavailable in
   the sandbox, edit the two affected lines by hand to match the PKGBUILD
   exactly and say so in the summary.

## Tests

- The retargeted contract test from task 2, at two sample rates.
- The existing gap test and control arm stay green unchanged.
- The full suites: `cargo test -p reprise-core` and
  `cargo test -p reprise-platform-linux -- --test-threads=1`.
- `cargo clippy --all-targets -p reprise-core -p reprise-platform-linux -- -D warnings`
  and `cargo fmt --check`.

Verify the new contract test is **red** against the current clamp before task 1
lands it green — a test that was never red proves nothing.

## Risks

- Hopping makes an oversized call do several FFTs instead of one. That is the
  point (it is catching up on audio that was previously thrown away), and it
  happens only on the Android stall path, but it lands on whichever thread
  called in. Note the cost in the summary; do not optimise it speculatively.
- Task 3 changes a latency/loss trade-off on the analysis branch. Bounded either
  way; if any playback test turns flaky, that is the signal to reconsider the
  number, not to remove the bound.

## Parallelität

One strand. Tasks 1–3 all sit in the same two crates and task 1 decides what
task 2 asserts; task 4 is four lines of generated metadata. Splitting costs more
than it buys.
