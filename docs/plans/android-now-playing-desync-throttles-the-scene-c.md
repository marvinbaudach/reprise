---
slug: android-now-playing-desync-throttles-the-scene-c
worktree: /home/marvin/Projects/reprise-android-now-playing-desync-throttles-the-scene-c
branch: feature/android-now-playing-desync-throttles-the-scene-c
phase: refactored
codex_session:
created: 2026-08-22
---

# Strand C — the per-frame cost of the bars

Mother plan: `docs/plans/android-now-playing-desync-throttles-the-scene.md`.
Measurements and symptom record:
`docs/plans/android-now-playing-desync-throttles-the-scene.HANDOFF.md`.

Read against `origin/dev` @ `1515487599`, **rebased onto a landed strand B**.
Line numbers for `crates/reprise-android-ffi/src/visualizer.rs` and
`VisualizerScene.kt` come from `origin/dev`; the one function strand B moved is
noted where it matters.

## Preconditions

**Strand B must be landed before this strand is started.** B's first commit moves
`drawPlayedVisualizer` out of `NowPlayingScene.kt` and into `VisualizerScene.kt`,
which is the function this strand rewrites. Running the two side by side puts
both in the same twenty lines during a move.

## Purpose

This strand does **not** fix the stutter — strands A and B do. It removes the
cost the bars pay per frame, which is real and measured but was never the cause:

- **`scene()` returns a boxed float list.** `AndroidVisualEngine::scene`
  (`visualizer.rs:396-408`) returns `Vec<f32>`, which crosses UniFFI as
  `List<Float>` (`VisualizerScene.kt:24, 109`) — one `java.lang.Float` object per
  scalar. The scene is up to 1540 shapes
  (`bars.rs:46`, `BAR_COUNT * (SEGMENT_COUNT + REFLECTION_SEGMENTS + 2) + 4`),
  each encoding to an 8-scalar header plus 3–4 geometry scalars
  (`visualizer.rs:561-602`). **Measured with `adb logcat --pid`: 67 MB freed in
  one 12 s spectrum window against 0 MB in the identical cover window.** Costs
  p50 8 ms → 11 ms.
- **A `Brush` is constructed in the draw phase**, once per glow shape per frame
  (`VisualizerScene.kt:203-211`), and the bars emit one whenever `value > 0.10`
  (`bars.rs:93-106`) — up to 64 shader objects per frame. A `Path` is allocated
  per polyline in the same loop (`VisualizerScene.kt:172`).
- **The critical section is the whole build.** `scene()` holds the state mutex
  across `state.engine.scene(...)` **and** `encode_scene(...)`
  (`visualizer.rs:397-408`), while the audio thread's `ingest_pcm_i16` takes
  `try_lock()` and **returns false, dropping the band frame**, when it cannot get
  in (`visualizer.rs:313-315`).

Note what the last one means: dropped band frames are invisible today. Nothing
counts them. So the first task is not a fix — it is a counter.

## File ownership

```
crates/reprise-android-ffi/src/visualizer.rs
android/app/src/main/java/de/reprise/spike/VisualizerScene.kt
```

plus their tests: `crates/reprise-android-ffi/src/*` test modules,
`android/app/src/test/java/de/reprise/spike/VisualizerScenePixelsTest.kt`,
`VisualizerSceneDriverTest.kt`, and anything new you add.

**One named exception:** the single call site
`drawPlayedVisualizer(buffer = visualEngine.scene(...), …)` in
`android/app/src/main/java/de/reprise/spike/NowPlayingScene.kt:234-244` — after
strand B it is guarded by `visualizerOpacity > 0f`. You change **that call and
nothing else** in that file, because you change the type flowing through it. Name
the edit in your report.

## What is **not** yours

- `SceneDriver.kt`, and the rest of `NowPlayingScene.kt` — strand B, landed.
- `MainActivity.kt`, `ReprisePlaybackService.kt`, `Media3PlaybackPort.kt`,
  `NowPlayingState.kt`, `PlaybackUiState.kt`, `MobileSurfaceViewModel.kt`,
  `NowPlayingSheet.kt` — strand A, landed.
- `crates/reprise-core/src/visuals/**` — the portable engine. `Scene`, `Shape`,
  `Geom` and the modes are **not** yours; you change how the scene crosses the
  bridge, not what the scene is. In particular: **no temporal smoothing.** The
  bars being a step function of decoder buffer arrivals
  (`engine.rs:212-215, 252`) is a visible-quality question with its own design
  pass, and it is deliberately not in this plan.
- Everything else in the repo.

## A warning before you start

The main checkout carried **uncommitted** work in
`crates/reprise-android-ffi/src/visualizer.rs` on 2026-08-22 (+39/−10 against
`HEAD`) that already attacks this strand's lock contention: it puts `tick()` and
`scene()` on `try_lock` and adds a `cached_scene: Mutex<Vec<f32>>` fallback. It
is **not landed and not part of this plan.** Branch from `origin/dev`, ignore it,
and do not merge the two blind. If it has landed by the time you start, rebase
onto it and say in your report which of the two designs survived.

## Test discipline

First the test, then the run that sees it fail, then the implementation.
`cargo test --exact` runs into the void easily — evaluate with
`grep -c '^test result: FAILED'` on the log file, never by looking at the last
line, and never through a pipe.

```sh
TMPDIR=/tmp cargo test --locked -p reprise-android-ffi > $LOG/c-ffi.log 2>&1
grep -c '^test result: FAILED' $LOG/c-ffi.log      # must be 0
```

**The Kotlin half does not compile before the Rust half stands.** The UniFFI
bindings under `android/app/src/main/java/uniffi/` are generated and gitignored;
`scripts/check-android-suite.sh` deletes and regenerates them from
`libreprise_android_ffi.so`. Within each task: Rust first, regenerate, then
Kotlin.

---

## Task C-1 — count the dropped band frames before changing anything

**Goal:** know whether the audio thread actually loses frames to contention, and
by how much, before touching the lock. Two failed repairs in this repo have
already been paid for by patching ahead of instrumenting.

**Files:**
- Modify: `crates/reprise-android-ffi/src/visualizer.rs` (`:274-332`, `:313-315`)
- Test: the test module in `crates/reprise-android-ffi/src/`

### Step 1: write the failing test

`aBandFrameDroppedOnContentionIsCounted` — hold the state mutex, call
`ingest_pcm_i16` with a valid buffer, assert it returns `false` **and** that the
engine's dropped-frame counter went up by one. Assert too that an uncontended
ingest does not move the counter.

### Step 2: implement

Add an `AtomicU64` on `AndroidVisualEngine`, incremented at the `try_lock`
failure at `:313-315` (and at the live-audio `try_lock` failure at `:297-299`,
which drops for the same reason), plus a `#[uniffi::export]` reader
`dropped_audio_frames()`. Do **not** change any behaviour in this task: the
counter is the whole deliverable.

Expose it in `NativeVisualSceneEngine`/`VisualSceneEngine` only if the device
measurement in C-5 needs to read it from Kotlin; a `Log` line at the Kotlin
boundary is enough, and it keeps the interface small.

---

## Task C-2 — the scene crosses the bridge as bytes

**Goal:** no `java.lang.Float` per scalar.

**Files:**
- Modify: `crates/reprise-android-ffi/src/visualizer.rs` (`:396-408`, `:561-602`)
- Modify: `android/app/src/main/java/de/reprise/spike/VisualizerScene.kt`
  (`:24`, `:109`, `:122-140`, `:222-268`, and `drawPlayedVisualizer`, which
  strand B moved into this file)
- Modify: `android/app/src/main/java/de/reprise/spike/NowPlayingScene.kt` — the
  one call site only
- Test: the Rust encode tests; `VisualizerScenePixelsTest.kt`

### Step 1: write the failing test

- Rust: `theEncodedSceneIsLittleEndianFloatBytes` — encode a known two-shape
  scene and assert the exact byte sequence, so the wire format is pinned rather
  than assumed by both sides.
- Kotlin: the **existing** pixel tests in `VisualizerScenePixelsTest.kt` are the
  behaviour proof and must pass unchanged after the type change. Add
  `aTruncatedBufferFailsClosed` — a byte array that ends mid-record draws nothing
  and does not throw. `FlatSceneCursor` fails closed today
  (`VisualizerScene.kt:229-267`); it must still fail closed on bytes.

### Step 2: implement

Rust:

- `encode_scene` returns `Vec<u8>`, writing each scalar as
  `f32::to_le_bytes()`. Capacity becomes the old scalar count times 4.
- `scene()` returns `Vec<u8>`; the empty case returns `Vec::new()`.

Kotlin:

- `VisualSceneEngine.scene` (`:24`) and `NativeVisualSceneEngine.scene` (`:109`)
  return `ByteArray`.
- `drawVisualizerScene` (`:122`) and `drawPlayedVisualizer` take `ByteArray`.
- `FlatSceneCursor` (`:222-268`) is the **only** place that indexes the buffer,
  which is why this change is contained. Wrap once:

  ```kotlin
  private class FlatSceneCursor(bytes: ByteArray) {
      private val values: FloatBuffer =
          ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).asFloatBuffer()
  }
  ```

  `values.size` becomes `values.limit()`, `values[i]` becomes `values.get(i)`,
  and `next()` becomes `values.get(index++)`. Nothing else in the file changes
  shape.
- A byte array whose length is not a multiple of 4 must fail closed, not throw.

Check `MAX_POINT_COUNT` (`:279`) still guards a hostile buffer: the bound is on
the decoded point count, not on the byte length, so it survives — but say so in
the report, with the test that shows it.

---

## Task C-3 — the draw loop stops allocating per shape

**Goal:** the per-frame allocations that can be hoisted are hoisted, and the ones
that cannot are measured before being redesigned.

**Files:**
- Modify: `android/app/src/main/java/de/reprise/spike/VisualizerScene.kt`
  (`:160-191`, `:193-212`)
- Test: `VisualizerScenePixelsTest.kt`

### The free one, do it first

`drawFlatPolyline` allocates a `Path()` per polyline (`:172`) — up to 64 per
frame. Hoist one `Path` per `drawVisualizerScene` call and `reset()` it per
shape. The path is drawn synchronously inside the same call, so reuse is safe.
One allocation per frame instead of 64, and no pixel changes.

### The one that needs a decision

`drawFlatRadialGlow` builds `Brush.radialGradient(...)` per glow per frame
(`:204-208`). The radius varies continuously with the bar's value
(`bars.rs:99`), so a cache keyed on the exact radius would thrash. Two options,
and the measurement decides:

- **(a) A reused `android.graphics.Paint` with a unit-radius `RadialGradient`
  shader and a local `Matrix`** scaled and translated per shape, drawn through
  `drawContext.canvas.nativeCanvas`. No allocation per shape at all. The cost is
  that this file learns an Android-graphics API it does not use today.
- **(b) A brush cache keyed on the colour and a quantised radius**, held across
  frames, with a hard entry cap. Cheaper to write, and it only helps if the
  quantisation is coarse enough to hit — which is exactly what the measurement
  has to show.

Take (a) unless it breaks a pixel test. Whichever you take, the pixel tests in
`VisualizerScenePixelsTest.kt` are the contract: **no visible change.** If (a)
shifts a pixel, that is a finding, not an accepted cost.

`FlatShapeHeader` (`:214-220`) is a data class allocated per shape — roughly 190
per frame. Leave it. It is small next to what C-2 removes, and turning it into
loose locals would cost the readability that makes this decoder auditable. Say in
the report that you left it deliberately.

---

## Task C-4 — the critical section stops covering the whole build

**Goal:** the audio thread stops losing band frames to the render thread — or, if
C-1's counter says it never did, the change is not made and the plan says so.

**Files:**
- Modify: `crates/reprise-android-ffi/src/visualizer.rs` (`:396-408`)
- Test: the test module in `crates/reprise-android-ffi/src/`

### Step 1: read C-1's counter

Run the device arm of C-5 with the counter in place, spectrum on, 12 s. If the
dropped-frame count is **zero**, this task is not done: record the number, say
the contention was not real, and move on. A change that fixes nothing measurable
is not an improvement, it is a risk.

### Step 2, if the counter is non-zero: write the failing test

`aSceneBuildDoesNotBlockAnIngest` — with the scene build in flight, an
`ingest_pcm_i16` must not be refused. Express it against the counter from C-1:
after N interleaved scene builds and ingests, the dropped count is 0.

### Step 3: implement

`VisualEngine::scene` (`engine.rs:370`) takes `&self`, so the `Scene` itself must
be built under the lock. What need not be is the encode:

```rust
pub fn scene(&self, width: f32, height: f32) -> Vec<u8> {
    let scene = {
        let state = self.lock();
        if !state.has_ingested || !width.is_finite() || !height.is_finite()
            || width <= 0.0 || height <= 0.0
        {
            return Vec::new();
        }
        state.engine.scene(width, height)
    };
    encode_scene(&scene)
}
```

Measure again with the counter. If the drops persist, the remaining contention is
the `Scene` build itself, and the fix is a design change — publishing an
already-encoded buffer from `tick()` rather than building on demand — which needs
the draw size at set-up time. **Do not start that here.** Record the number and
raise it as a finding; it is a separate plan.

---

## Task C-5 — the measurement

**Goal:** the acceptance is a number against a control arm, on the phone, not a
claim.

`scripts/android-scene-framerate.sh` (strand B) already reports GC bytes freed
per window per arm. Run it on this branch:

1. Spectrum on, 12 s, on a track **with** live audio, state verified in sync at
   both ends.
2. The control arm: cover mode, same track, same window.
3. The dropped-frame counter from C-1, read at both ends of the spectrum window.

Report the table. Baseline from the handoff, on the same phone: **67 MB freed
with the spectrum against 0 MB with the cover.** Note that strand B changed the
cover arm — the live analysis engine now runs in cover mode too, so the cover
baseline is no longer 0 MB, and B's report carries the new one. Compare against
**that**, not against the handoff's 0 MB.

Frame times to hold: the in-sync spectrum arm measured p50 11 ms / p90 13 ms with
the boxed buffer. After C-2 they should not be worse; if they are, that is a
finding.

---

## Acceptance for this strand

```sh
TMPDIR=/tmp cargo test --locked -p reprise-android-ffi > $LOG/c-ffi.log 2>&1
grep -c '^test result: FAILED' $LOG/c-ffi.log        # 0

cargo test --locked -p reprise-core > $LOG/c-core.log 2>&1
grep -c '^test result: FAILED' $LOG/c-core.log       # 0

JAVA_HOME=/usr/lib/jvm/java-21-openjdk \
  scripts/verify-now-playing-scene.sh > $LOG/c-scene.log 2>&1

JAVA_HOME=/usr/lib/jvm/java-21-openjdk \
  scripts/check-android-suite.sh > $LOG/c-android.log 2>&1
grep -E '^suites=' $LOG/c-android.log                # failures=0 errors=0 verdict=fresh

cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
```

## For the report

- The red run of every new test, quoted.
- The dropped-frame counts from C-1, before and after C-4 — including the case
  where they were zero and C-4 was correctly **not** done.
- The GC table from C-5, against strand B's new cover baseline.
- The one edit made to `NowPlayingScene.kt`, quoted.
- Anything you deliberately left alone (`FlatShapeHeader`, temporal smoothing,
  the `tick()`-publishes-the-buffer redesign) and why.
