---
slug: android-fog-crossfade
worktree: /home/marvin/Projects/reprise-android-fog-crossfade
branch: feature/android-fog-crossfade
phase: refactored
codex_session:
created: 2026-09-18
---
# Android: the fog turns instead of cutting

Spec: `docs/superpowers/specs/2026-09-18-android-fog-crossfade-design.md` —
read it first; this plan is the cut of that spec into tasks. Everything below
is Android-only (`android/` and `docs/`).

## Goal

On a track change the now-playing background (oil film + blurred cover disc,
together "the fog") switches in ~200 ms while sliding with the panel. Make it
behave like the desktop: the cover still settles in 480 ms, but the fog is one
stationary layer that crossfades over **1000 ms, linear**. The cover ↔
visualizer switch keeps its 220 ms for cover and bars but moves the film palette
to the same slow 1000 ms clock. And `rememberCoverFogBitmap` stops fading to
black while a new cover is blurred.

## Shared context for every task

- Files (all under `android/app/src/main/java/io/github/marvinbaudach/reprise/`
  unless stated): `NowPlayingScene.kt` (scene + `NowPlayingPanelLayer` +
  `LiveSceneHandle` + `drawPlayedNowPlayingFog/Shimmer`), `NowPlayingSheet.kt`
  (`visualizerOpacity`, `VISUALIZER_CROSSFADE_MS = 220`), `NowPlayingFog.kt`
  (`drawNowPlayingFog(palette, center, seconds, level, opacity, driftEnabled)`),
  `NowPlayingShimmer.kt` (`drawNowPlayingShimmer(fog, center, coverDiameterDp,
  elapsedSeconds, swell, opacity, rotationsEnabled, alphaScale)`),
  `CoverFogBitmap.kt` (`CoverFogBitmap(disc, palette)`,
  `rememberCoverFogBitmap`), `OilFilmPalette.kt` (`blendedTo(other, fraction)`,
  `VisualizerRampPalette`), `scene/SceneState.kt` (`oilFilmSeconds`,
  `oilFilmLevel`, `fogLevel`, `shimmerElapsedSeconds`,
  `SHIMMER_TURN_SECONDS = 60.0`).
- Tests under `android/app/src/test/java/io/github/marvinbaudach/reprise/`:
  `NowPlayingPanelsTest.kt` (glow tests to remove), `NowPlayingSceneVerificationTest.kt`
  (`renderPlayedFog()` pixel harness: `CanvasDrawScope().draw(...)` onto an
  Android `Bitmap`, luma read back from an `IntArray`),
  `NowPlayingGesturesTest.kt` (pins `VISUALIZER_CROSSFADE_MS == 220`),
  `CoverFogBitmapTest.kt`, `NowPlayingSceneEngineTest.kt` (compose-rule tests
  that pass `visualizerOpacity` into the scene).
- House idioms to keep: every blend/transform is a **pure function beside its
  data class** (`nowPlayingVisualBlend`, `nowPlayingPanelTransform`) and unit
  tested; canvases read state, they never decide. Comments explain *why*, in
  the register of the surrounding files. Raw `Color(`/`Color.` outside
  `ui/theme/` fails `scripts/check-android-theme.sh` — pass colours through as
  packed ARGB ints or `OilFilmPalette`, exactly as the existing fog code does.
- Animation gate: `motion.sceneAnimationsEnabled` (from
  `LocalAmbientMotionController`) — animate when true, `snapTo` when false,
  as `NowPlayingSheet.kt:140–146` does for `visualizerOpacity`.
- **This plan touches ONLY `android/` and `docs/`. The Rust gates in AGENTS.md
  do not apply — do NOT run any cargo command yourself. One Gradle invocation
  at a time.** Evidence is `scripts/check-android-suite.sh` (it builds the host
  `.so` and the UniFFI bindings itself; needs `JAVA_HOME` on a JDK 21 and
  `TMPDIR=/tmp`; do not export `LD_LIBRARY_PATH` by hand), then
  `npm --prefix android run lint`, then the contract scripts
  `scripts/check-android-theme.sh`, `scripts/check-accessibility-semantics.sh`,
  `scripts/check-input-parity.sh`, `scripts/check-architecture.sh`,
  `scripts/check-project-quality.sh`, `scripts/check-ai-hygiene.sh`. A fresh
  worktree needs `android/local.properties` with
  `sdk.dir=/home/marvin/.local/share/android-sdk` (copy it from the main
  checkout; it is gitignored).
- Never hand-edit the status block in this file's frontmatter.

## Tasks

### Task 1 — `rememberCoverFogBitmap` keeps the last fog while the next one is blurred

`CoverFogBitmap.kt:68–80`. Today `prepared` is `remember(artwork, fallbackArgb,
cache) { mutableStateOf(artwork?.let(cache::fog)) }`, so every artwork change
resets it to the cache hit or `null`, and `drawNowPlayingShimmer` /
`drawNowPlayingFog` draw nothing for a `null` fog until the blur on
`Dispatchers.Default` finishes.

Change: one `remember { mutableStateOf<CoverFogBitmap?>(null) }` that is never
re-keyed; the `LaunchedEffect(artwork, fallbackArgb)` assigns the cache hit
synchronously when there is one and otherwise leaves the previous value in
place until the freshly prepared fog arrives. The only `null` is the very first
composition without a cache hit. Guard against a stale result overtaking a
newer one: the effect is keyed on the artwork, so a cancelled effect's result
must not be written (check `isActive` / rely on cancellation of `withContext`).

Test (`CoverFogBitmapTest.kt` or a new `CoverFogBitmapHoldTest.kt`, using the
compose rule pattern of `NowPlayingSceneEngineTest.kt`): compose
`rememberCoverFogBitmap(A)` with a cache that has A → fog A; switch to artwork
B that is not cached → the composable still returns fog A on the next frame;
after `waitForIdle()` it returns a fog whose palette differs from A's (B is a
different colour). A control assertion: the old behaviour would have returned
`null` in between — assert the value is never `null` after the first fog.

### Task 2 — the handover model: `FogHandover` + `fogHandover()` (new file, pure)

New file `NowPlayingFogHandover.kt`:

```kotlin
internal data class FogHandover(
    val outgoing: CoverFogBitmap?,        // disc on its way out; null before the first fog
    val outgoingPalette: OilFilmPalette?, // the film mixture the outgoing disc left behind
    val outgoingAlpha: Float,             // 1 after a plain handover, the arrival t after a restart
    val incoming: CoverFogBitmap?,
) { companion object { val EMPTY = FogHandover(null, null, 1f, null) } }

internal fun fogHandover(previous: FogHandover, liveFog: CoverFogBitmap?, arrival: Float): FogHandover
```

Rules (arrival clamped to 0..1):

- `liveFog === previous.incoming` → return `previous` unchanged (identity, not
  equality — a re-prepared bitmap for the same track *is* a new fog).
- `liveFog == null` → return `previous` unchanged. A newly live panel has no
  fog until its own blur finishes; that absence must not erase or restart the
  handover already on screen.
- `previous.incoming == null` (first fog) → `FogHandover(null, null, 1f, liveFog)`.
- `arrival >= 1f` (**handover**) → `outgoing = previous.incoming`,
  `outgoingPalette = previous.incoming.palette`, `outgoingAlpha = 1f`,
  `incoming = liveFog`.
- otherwise (**restart** mid-fade) → the mixture on screen at `t = arrival`
  becomes the new outgoing: `outgoingPalette = (previous.outgoingPalette ?:
  previous.incoming.palette).blendedTo(previous.incoming.palette, t)`,
  `outgoing = previous.incoming`, `outgoingAlpha = t`, `incoming = liveFog`.
  The older disc is dropped — never three layers.

Film palette shown at arrival `t`: `outgoingPalette.blendedTo(incoming.palette,
t)` when both exist, else whichever exists. Put that in a small pure
`FogHandover.paletteAt(t: Float): OilFilmPalette?` on the class. Disc alphas at
`t`: outgoing `outgoingAlpha · (1 − t)`, incoming `t` — expose as
`FogHandover.outgoingDiscAlpha(t)` / `incomingDiscAlpha(t)`; when
`outgoing == null` the incoming alpha is `1f` regardless of `t` (there is
nothing to fade from).

Clock continuity, same file, pure:

```kotlin
internal data class FogClockOffsets(val filmSeconds: Float, val shimmerSeconds: Double)
internal fun continuedFogClocks(shownFilmSeconds: Float, shownShimmerSeconds: Double,
                                newFilmSeconds: Float, newShimmerSeconds: Double): FogClockOffsets
```
`filmSeconds = shownFilm − newFilm`; `shimmerSeconds = ((shownShimmer −
newShimmer) mod SHIMMER_TURN_SECONDS + SHIMMER_TURN_SECONDS) mod
SHIMMER_TURN_SECONDS`. The layer draws `state.oilFilmSeconds + offsets.filmSeconds`
and `(state.shimmerElapsedSeconds + offsets.shimmerSeconds) mod SHIMMER_TURN_SECONDS`.

`internal const val FOG_CROSSFADE_MS = 1000` lives here too, with a KDoc that
carries the desktop's reasoning: linear on purpose, because two layers painted
one over the other must sum to 1 at the midpoint or the light dips
(`crates/reprise-gnome/src/ui/now_playing/cover_cloud.rs`, `cover_fade`).

Tests, new `NowPlayingFogHandoverTest.kt` (plain JUnit, build small
`CoverFogBitmap`s via the existing `prepareCoverFogBitmap` with two solid
colours, e.g. black and white):

- first fog: no outgoing, `incomingDiscAlpha(0f) == 1f`;
- plain handover at rest: outgoing is the old fog, `outgoingAlpha == 1f`,
  `paletteAt(0.5f)` equals `old.palette.blendedTo(new.palette, 0.5f)`;
- restart at `t = 0.4`: `outgoingAlpha == 0.4f`, `outgoingPalette` equals the
  mixture the previous handover showed at 0.4, `outgoing` is the *previous
  incoming*, the previous outgoing is gone;
- same fog object again → the state is returned unchanged (`===`);
- disc alphas sum to 1 for every `t` after a plain handover
  (`outgoingDiscAlpha(t) + incomingDiscAlpha(t) == 1f` at 0, 0.25, 0.5, 1);
- `continuedFogClocks`: shown 100.0 s, new panel at 3.0 s → offset 97.0 and the
  displayed value equals 100.0; shimmer shown 59.5 s, new 2.0 s → displayed
  `(2.0 + offset) mod 60 == 59.5` within 1e-9; a negative difference wraps into
  `[0, 60)`;
- `FOG_CROSSFADE_MS == 1000` pinned next to the existing
  `VISUALIZER_CROSSFADE_MS == 220` assertion in `NowPlayingGesturesTest.kt`.

### Task 3 — the live panel publishes, and stops drawing the fog

`NowPlayingScene.kt`:

- `LiveSceneHandle` (line ~696) grows three observable fields next to `engine`:
  `var fog: CoverFogBitmap? by mutableStateOf(null)`, `var state: SceneState?
  by mutableStateOf(null)`, `var drawRevision: Int by mutableIntStateOf(0)`.
- In `NowPlayingPanelLayer`, when `isLivePanel`: publish `fog`, `state` and
  `drawRevision` into the handle in a `SideEffect`, and clear `fog`/`state` in
  the existing `DisposableEffect(liveScene, visualEngine)`'s `onDispose` when
  they are still this panel's (same `===` guard as `engine`). The revision write
  is what invalidates the layer's canvas each scene frame — say so in a comment,
  it is not obvious.
- Delete the panel's fog `Canvas` block (lines ~528–550: the
  `Modifier.fillMaxSize().graphicsLayer { translationX = glow.translationX }`
  canvas that calls `drawPlayedNowPlayingFog` and `drawPlayedNowPlayingShimmer`).
  The panel now draws only the cover box and the bars.
- Remove `GLOW_TRANSLATION_FACTOR`, `NowPlayingGlowTransform`,
  `nowPlayingGlowTransform` and the `val glow = …` line; nothing else consumes
  them.
- `drawPlayedNowPlayingFog` / `drawPlayedNowPlayingShimmer` (lines ~748–778)
  move to the new layer file in Task 4 and change signature there; delete
  them here.

Tests: in `NowPlayingPanelsTest.kt` remove
`each_track_glow_uses_the_spatial_factor_and_distance_fade` and strip the glow
half out of `panel_and_glow_rest_state_stays_bit_exact_at_a_non_round_screen_width`
(keep the panel-transform assertions, rename to drop "glow"). Add a compose-rule
test in `NowPlayingSceneEngineTest.kt` (or a sibling) that composes the scene
with one live panel and asserts the handle's `fog` and `state` are non-null
after idle and become `null` when the panel leaves — Codex may need to expose
the handle for the test the same way `LiveSceneHandle.engine` is reached today;
follow whatever `theSceneEngineExistsWhileTheScreenIsUpWithTheCoverShowing`
does.

### Task 4 — `NowPlayingFogLayer`: one stationary canvas behind the panels

New file `NowPlayingFogLayer.kt`:

```kotlin
@Composable
internal fun NowPlayingFogLayer(
    liveScene: LiveSceneHandle,
    motion: AmbientMotionController,
    visualizerLight: Float,
    modifier: Modifier = Modifier,
)
```

Composition:

- `val arrival = remember { Animatable(1f) }`,
  `var handover by remember { mutableStateOf(FogHandover.EMPTY) }`,
  `val clocks = remember { FogClocks() }` — a tiny holder with `offsets:
  FogClockOffsets`, `lastShownFilmSeconds`, `lastShownShimmerSeconds`, and the
  `SceneState` identity the offsets were computed for.
- `LaunchedEffect(liveScene.fog, motion.sceneAnimationsEnabled)`:
  `val next = fogHandover(handover, liveScene.fog, arrival.value)`; if `next
  === handover` only make sure a disabled-animations toggle lands at
  `arrival.snapTo(1f)` and return; else `handover = next`; if
  `!sceneAnimationsEnabled || next.outgoing == null` → `arrival.snapTo(1f)`
  else `arrival.snapTo(0f); arrival.animateTo(1f, tween(FOG_CROSSFADE_MS,
  easing = LinearEasing))`. Keying the effect on the fog is what cancels a
  running fade on a restart — the mixture was already captured by
  `fogHandover` from `arrival.value` before the snap.
- Clock offsets: whenever `liveScene.state` is a different object from the one
  the offsets were computed for, `clocks.offsets = continuedFogClocks(
  clocks.lastShownFilmSeconds, clocks.lastShownShimmerSeconds,
  newState.oilFilmSeconds, newState.shimmerElapsedSeconds)`. Do this at the
  top of the draw lambda (it is the only place the shown values are known and
  it must precede the draw that would otherwise jump); the first state gets
  zero offsets. Every draw stores the shown seconds back into `clocks`.

Draw (`Canvas(modifier.fillMaxSize())`), all through one pure entry that the
renderer test can call without Compose:

```kotlin
internal fun DrawScope.drawNowPlayingFogLayer(
    handover: FogHandover, arrival: Float, state: SceneState, clocks: FogClockOffsets,
    visualizerLight: Float, center: Offset, rotationsEnabled: Boolean,
)
```
- `observeSceneFrame(liveScene.drawRevision)` (move that helper into this file
  or keep it internal in `NowPlayingScene.kt`; `arrival.value` is also read
  here so the fade invalidates on its own). `state == null` → draw nothing.
- centre `Offset(size.width / 2f, size.height * PLAYED_CENTRE_FRACTION)` — no
  horizontal shift any more (spec §1 explains the jump a position-anchored shift
  would cause). `drawNowPlayingFog` then derives `horizontalShiftPx = 0`.
- **Film once:** `palette = handover.paletteAt(arrival)?.blendedTo(
  VisualizerRampPalette, visualizerLight)`, `seconds = state.oilFilmSeconds +
  clocks.filmSeconds`, `level = state.oilFilmLevel`, `opacity = 1f`,
  `driftEnabled = rotationsEnabled` (`motion.sceneRenderPower().fogRotates`,
  as the panel used).
- **Disc twice** via `drawNowPlayingShimmer`: outgoing with `opacity =
  handover.outgoingDiscAlpha(arrival)`, incoming with
  `handover.incomingDiscAlpha(arrival)`; skip a draw whose alpha is `0f`;
  `elapsedSeconds = (state.shimmerElapsedSeconds + clocks.shimmerSeconds) mod
  SHIMMER_TURN_SECONDS`, `swell = state.fogLevel`, `coverDiameterDp =
  COVER_SIZE_DP.toFloat()`.

Placement: in `NowPlayingScene` (the composable with
`Box(Modifier.fillMaxSize().testTag("now-playing-scene"))`, ~line 376) put
`NowPlayingFogLayer(liveScene, motion, visualizerLight)` as the **first child**
of that `Box`, before `panels.forEach`. `NowPlayingScene` gains
`visualizerLight: Float` as a **required** parameter — no default (grill
decision: a default of `visualizerOpacity` would let a forgetful caller get the
fast 220 ms light with no compiler complaint). Update every call site: the
sheet (Task 5 passes the real value) and the compose-rule tests that compose
the scene (`NowPlayingSceneEngineTest.kt`, `DriveSceneComposeTest.kt`,
`NowPlayingPanelFrozenSceneIdentityTest.kt` — grep for `NowPlayingScene(` under
`app/src/test` to be sure), which pass the same value they pass for
`visualizerOpacity`.

Tests:

- `NowPlayingSceneVerificationTest.kt`: retarget `renderPlayedFog()` at
  `drawNowPlayingFogLayer` with a single-fog `FogHandover` at `arrival = 1f`
  and zero offsets — the two existing pixel tests must stay green unchanged in
  their assertions (the raster is the same picture). Add
  `a_half_arrived_fog_sits_between_its_ends_and_never_dips`: fogs from a black
  and a white cover (`prepareCoverFogBitmap` with solid bitmaps), a plain
  handover, render at `t = 0, 0.5, 1`; `meanFogRegionLuma(0.5)` lies between
  the two end lumas and is `>= min(end lumas) − 1` (8-bit rounding); the `t =
  1` raster equals the raster of a fresh single-fog state with the white fog
  (bit exact — the handover leaves no residue).
- Clock continuity at the draw level, same file: render at arrival 1 with
  state A (film at 100 s, offsets 0), then hand over to state B whose clocks
  read 3 s with `continuedFogClocks` offsets — the film raster with B is
  bit-identical to the film raster with A at the same level (`level` equal,
  `rotationsEnabled = false` keeps the drift deterministic).
- `NowPlayingGesturesTest.kt` gets the `FOG_CROSSFADE_MS` pin from Task 2.

### Task 5 — `visualizerLight`: the palette takes the slow clock

`NowPlayingSheet.kt`: next to `visualizerOpacity` (line ~137) add
`val visualizerLight = remember(visualizerPreference) { Animatable(if
(visualizerVisible.value) 1f else 0f) }` and extend the existing
`LaunchedEffect(visualizerVisible.value, motion.sceneAnimationsEnabled)` to
drive both: `visualizerOpacity` as today (220 ms), `visualizerLight` with
`tween(FOG_CROSSFADE_MS, easing = LinearEasing)` — run the two `animateTo`s
concurrently (`launch { }` inside the effect, or a second `LaunchedEffect`
with the same keys), snap both when animations are off. Pass
`visualizerLight = visualizerLight.value` into `NowPlayingScene` (line ~347)
→ the layer. Nothing else reads it; `barsOpacity` no longer touches the
palette (that call moved into the layer in Task 4 and reads `visualizerLight`).

Test: a pure-function pin is not possible for an `Animatable`; instead extend
the compose-rule tests that already compose the sheet —
`ComposeBehaviorTest.kt` composes `NowPlayingSheet(...)` three times, and
`NowPlayingGesturesTest.kt` holds the visualizer-toggle coverage and the
`VISUALIZER_CROSSFADE_MS` pin; `ArtistPhotoProgressBarTest.kt:291` shows the
`mainClock.autoAdvance = false` / `advanceTimeBy` pattern. Toggle the
visualizer on, advance 220 ms → the scene receives
`visualizerOpacity == 1f` while `visualizerLight < 1f`; advance to 1000 ms →
`visualizerLight == 1f`. If the sheet's test seam does not expose the scene
parameters, expose them through the existing test tag / a `@VisibleForTesting`
accessor in the same way the sheet's other animatables are observed; do not
skip the test.

### Task 6 — gates and the handoff

1. `scripts/check-android-suite.sh` → `verdict=fresh` and the test count above
   the floor plus the new tests (expect ≥ 334 + ~10). Filtered Gradle runs are
   fine while iterating; the final evidence is the whole suite through the
   script.
2. `npm --prefix android run lint`.
3. The six contract scripts listed in the shared context, each exit 0.
4. Commit in focused steps (one per task is fine). No agent attribution lines
   in commit messages.
5. Write nothing into the status block.

Device verification (screen recording of a next tap, frames at 0/250/500/750/
1000 ms, plus a control arm on `dev`) is **not** a Codex task — it needs the
device lock and runs in the review session after `/check`. Grill decision:
that measurement happens **before** landing, and the feature build stays
installed on the phone afterwards so Marvin can judge the feel himself before
the branch lands.

## Out of scope

Case B in the spec (window rebuild on a jump), any zoom/breathing, the
desktop, `SceneState`/`SceneDriver` (the clocks are continued by offset, not
moved).

## Parallelität

**Cut attempted; single strand chosen (confirmed in the grill, 2026-09-18).**
Further grill decisions folded into the tasks above: the first fog snaps
(Task 4), one constant `FOG_CROSSFADE_MS` for both slow lights (Tasks 2, 5),
`visualizerLight` is a required parameter (Task 4), Codex runs all six
contract scripts (Task 6), device measurement before landing (Task 6).

The only disjoint file group is Task 1 (`CoverFogBitmap.kt` + its test): it
touches nothing the other tasks touch, and Tasks 2–5 do not depend on it. It
could be strand B. But it is ~20 lines of change and one test; a second
worktree costs a second `check-android-suite.sh` run (host `.so` build +
bindings + full JVM suite, several minutes each), a second Gradle daemon on a
box the load governor already throttles, a second PR and a second landing.
The wall-clock saved is a few minutes of Codex time; the wall-clock spent is
more than that. Tasks 2–5 cannot be cut at all: 3, 4 and 5 all edit
`NowPlayingScene.kt` and 4 depends on 2's types.

- **Strand (single):** owns `android/app/src/main/java/io/github/marvinbaudach/reprise/{CoverFogBitmap,NowPlayingScene,NowPlayingSheet,NowPlayingFogHandover,NowPlayingFogLayer}.kt`,
  `android/app/src/test/java/io/github/marvinbaudach/reprise/{CoverFogBitmap*,NowPlayingFogHandover*,NowPlayingPanels*,NowPlayingSceneVerification*,NowPlayingSceneEngine*,NowPlayingGestures*,NowPlayingSheet*}Test.kt`,
  `docs/plans/android-fog-crossfade.md`. Tasks 1–6 in order.
- **Merge order:** n/a.
- **Post-merge cross-checks:** none needed for the cut; the device recording
  (see Task 6) is the post-code check and runs in the review session.

## Device verification (2026-09-24)

The measurement Task 6 demands before landing is done, on the 1080×2404 phone,
control arm on `dev` `4b75660d84` and feature arm on this branch's
`e957068817`. Same transition in both arms with shuffle off — „(I Used to Make
Out With) Medusa" (teal cover) → „(In)Human Scum" (warm red-brown cover),
started from the Titles list so the list is the queue.

Frames are extracted at 20 fps and **t0 is derived from the frames**: the first
frame whose cover region jumps against its predecessor. A fixed offset from the
tap would have been wrong by 50–100 ms, because `screenrecord` starts encoding
after the host launches it. Crossfade progress is the mean colour of a
background patch beside the cover, normalised: 0 = the outgoing track's fog,
1 = the incoming one's.

| ms after the track change | dev (control) | this branch |
|---|---|---|
| 100 | 0.85 | 0.13 |
| 250 | 0.88 | 0.33 |
| 500 | 1.03 | 0.54 |
| 750 | 1.02 | 0.76 |
| 1000 | 1.01 | **1.00** |

The control is 85–100 % switched 100 ms after the change; this branch climbs in
even ~0.05 steps per 50 ms and lands on 1.00 at exactly 1000 ms — the specified
linear crossfade. The rows before 100 ms are contaminated by the panel slide
(the patch lies in the outgoing cover's path) and say nothing about the fog.

Review finding 1's freeze — the handover holding steady on a null live fog —
was **not** exercised: the blur resolved before the panel went live, so the
curve is monotone with no plateau. Judging how that freeze feels is left to the
manual pass, which is why the feature build stays installed.

Harness and frames: `~/.cache/reprise-scratch/fog-device-run/`
(`fog-record2.sh`, `fog-analyse.py`, `control-arm.csv`, `feature-arm.csv`,
`fog-arms.png`).
