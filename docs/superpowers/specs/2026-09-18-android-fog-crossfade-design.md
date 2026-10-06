# Android: the fog turns instead of cutting

**Date:** 2026-09-18
**Scope:** Android now-playing scene only. Desktop is the reference, not a target.

## Problem

On a track change the now-playing background — the oil film plus the blurred
cover disc ("shimmer"), together the *fog* — reads as a hard cut. The fog is
drawn per panel (`NowPlayingPanelLayer`, `NowPlayingScene.kt`) and its opacity
is chained to the panel position: `nowPlayingGlowTransform` gives
`opacity = 1 − |delta|·1.1`, so the fade is complete at 91 % of the slide, and
the settle easing `CubicBezierEasing(0.22, 1.06, 0.32, 1)` covers that distance
in roughly the first 150–200 ms of the 480 ms settle. The background therefore
switches in ~200 ms, moves sideways while doing it, and the two layers dip in
brightness at the midpoint.

The desktop does the opposite: the cover picture changes in 250 ms
(`motion::STANDARD`), while the cloud background crossfades **stationary over
1000 ms, linear** (`cover_cloud.rs`, `COVER_FADE_S`). The background is four
times slower than the subject; that is the softness the user is asking for.

Case covered: the ordinary adjacent advance (next/previous, swipe, auto
advance). A jump that rebuilds the panel window (library tap, shuffle, queue
jump) is out of scope except for §5, which removes its fade-to-black.

## Design

### 1. One stationary fog layer: `NowPlayingFogLayer`

A new composable in its own file, placed in the scene `Box`
(`NowPlayingScene.kt`, the `Box` with `testTag("now-playing-scene")`) **before**
`panels.forEach`, so it is one full-size `Canvas` behind every panel.

`NowPlayingPanelLayer` stops drawing the fog: `drawPlayedNowPlayingFog` and
`drawPlayedNowPlayingShimmer` leave the panel, which then draws only cover and
bars. `nowPlayingGlowTransform`, `NowPlayingGlowTransform` and
`GLOW_TRANSLATION_FACTOR` are removed — the fog was their only consumer.

The layer is **fully stationary**, including horizontally. Today the film
travels with the cover during a drag (`NowPlayingFog.kt`, `horizontalShiftPx`);
that made sense while every panel owned its own fog. With one shared layer any
position-anchored shift jumps by `0.23 · width` at the moment the live index
changes (the outgoing panel's translation is `−d·w`, the incoming one's
`(1−d)·w`), and there is no anchor that is continuous through both the commit
and the end of the fade. So the light stays put under a sliding cover and
turns over the next second — which is exactly the desktop behaviour.
The layer passes `center.x = width / 2`, so `drawNowPlayingFog` derives
`horizontalShiftPx = 0`; the shift plumbing and its comment stay for the
renderer tests that exercise it.

The live panel publishes what the layer needs through the existing
`LiveSceneHandle` (today it carries only `engine`):

- `fog: CoverFogBitmap?` — the live panel's prepared fog,
- `state: SceneState?` — the live panel's scene state (levels and clocks),
- `drawRevision: Int` — the live panel's frame revision.

The panel writes them in a `SideEffect`; the layer reads them inside its draw
lambda. The revision is the layer's invalidation signal, exactly as
`observeSceneFrame(drawRevision)` is for the panel canvases today. The
`arrival` animatable value is also read in the draw lambda and invalidates on
its own.

### 2. The handover: `fogHandover`, a pure function

Layer state:

```
outgoing: CoverFogBitmap?      // the fog on its way out (null at first start)
incoming: CoverFogBitmap?      // the fog on its way in
arrival:  Animatable(1f)       // 0 → 1 over the fade; 1 at rest
outgoingPalette: OilFilmPalette?   // see Restart
outgoingAlpha: Float           // see Restart; 1 after a plain handover
```

The rule fires when the live fog **object** changes — not only when the track
id changes, so it also fires when a cover finishes preparing late:

- **Handover** (`arrival == 1`): `outgoing = incoming`, `outgoingPalette =
  incoming.palette`, `outgoingAlpha = 1`, `incoming = new`, then `arrival`
  animates 0 → 1 over `FOG_CROSSFADE_MS = 1000`, **linear**.
- **Restart** (a change while `arrival = t < 1`): the mixture on screen at `t`
  becomes the new outgoing. For the film that is
  `outgoingPalette = outgoingPalette.blendedTo(incoming.palette, t)`; for the
  disc the current `incoming` becomes `outgoing` with `outgoingAlpha = t` (the
  older disc is dropped — never three layers). Then `incoming = new` and
  `arrival` restarts at 0.
- **No animations** (`motion.sceneAnimationsEnabled == false`): `snapTo(1f)`,
  the same gate `NowPlayingSheet.kt` applies to `visualizerOpacity`.

Linear on purpose: the two disc layers are painted one over the other and
must sum to 1 at the midpoint, otherwise the light dips. This is the reasoning
`cover_cloud.rs::cover_fade` documents; it is carried over verbatim.

`fogHandover(previous: FogHandoverState, liveFog: CoverFogBitmap?, arrival:
Float, animationsEnabled: Boolean): FogHandoverState` lives beside
`nowPlayingVisualBlend` and returns a new value — no mutation inside the
canvas, house idiom.

Drawing per frame:

- **Oil film, once**, with `palette = outgoingPalette.blendedTo(incoming.palette,
  t)`, then `.blendedTo(VisualizerRampPalette, visualizerLight)` (§4). One film
  pass, so no second shader cost and no brightness dip.
- **Shimmer disc, twice**: `outgoing` at `opacity · outgoingAlpha · (1 − t)`,
  `incoming` at `opacity · t`. Both are translucent
  (`NowPlayingShimmerSpec.OVER_FOG_SCALE`); two draws are cheap.
- `opacity` is the layer's own presence (1 while the sheet is open); the old
  per-panel glow opacity no longer exists.

### 3. The clocks keep running

`oilFilmSeconds` and `shimmerElapsedSeconds` live in the per-panel
`SceneState`. If the layer simply read them from the new live state at the
handover, the film composition would jump in that frame — the very jolt this
design removes. The layer therefore keeps an **offset** per clock:

- at the handover, `filmOffset = shownFilmSeconds − new.oilFilmSeconds` and
  `shimmerOffset = (shownShimmerSeconds − new.shimmerElapsedSeconds) mod
  SHIMMER_TURN_SECONDS`;
- it draws `new.oilFilmSeconds + filmOffset` and
  `(new.shimmerElapsedSeconds + shimmerOffset) mod SHIMMER_TURN_SECONDS`.

No new clock, no change to `SceneState` or `SceneDriver`. The levels
(`oilFilmLevel`, `fogLevel`) are read unsmoothed from the live state — their
envelopes are slow by construction.

### 4. Cover ↔ visualizer: the same rhythm

`visualizerOpacity` (220 ms, `VISUALIZER_CROSSFADE_MS`) keeps driving the cover
and the bars. The film palette no longer follows that opacity: the sheet gets a
second animatable `visualizerLight` next to `visualizerOpacity`, same target,
**1000 ms linear** (`FOG_CROSSFADE_MS`), same `snapTo` gate. The layer receives
both; `drawPlayedNowPlayingFog` blends towards `VisualizerRampPalette` with
`visualizerLight` instead of `barsOpacity`. Subject fast, light slow — on both
transitions.

### 5. No fade to black

`rememberCoverFogBitmap` currently resets its state to `null` (or the cache hit)
whenever the artwork changes, and draws nothing until the blur has finished on
`Dispatchers.Default`. It keeps the last finished fog instead until the new
one is ready; only the very first fog of a composition is `null`. This is right
regardless of the case above and costs nothing.

## Testing

- `fogHandover` unit tests: plain handover; restart mid-fade (the new
  outgoing palette equals the mixture at `t`, the new outgoing alpha equals
  `t`); snap when animations are off; clock continuity (the shown seconds are
  identical in the frame before and after a handover).
- Renderer test of the layer in the style of the existing
  `drawPlayedNowPlayingFog` pixel verification: at `t = 0.5` the brightness at
  the fog centre lies between the two end states and never below the minimum
  of both (no dip).
- `rememberCoverFogBitmap`: returns the previous fog while the new one is
  computing, the new one afterwards.
- Panel layer: no fog draw calls remain in `NowPlayingPanelLayer`; the scene
  has exactly one fog canvas.
- On the device (device lock held): screen recording of a next tap; frames at
  0/250/500/750/1000 ms show the fog colour moving monotonically towards the new
  cover while the cover disc has arrived by 480 ms. A control arm without the
  change shows the ~200 ms cut.

## Out of scope

- Case B (window rebuild on a jump): only the fade-to-black goes away via §5;
  the crossfade there starts only once both fogs exist.
- Zoom, breathing, colour-temperature morphs.
- Desktop.
