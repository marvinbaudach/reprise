---
slug: android-desktop-visualizer
worktree:
branch:
phase: planned
codex_session:
created: 2026-08-10
---
# Android: the desktop visualizer, in the cover's place

Status: **planned — waits for `android-play-view-gestures` to land**
Base: `origin/dev` at `6c0646343a` plus the play-view rebuild, which owns
`NowPlayingScene.kt` and `appearance.rs` until it merges.
As of: 2026-08-10

## Product goal

The visualizer the desktop draws — `reprise-core::visuals`, bars mode — runs on
the phone too, in the square the cover occupies, and a tap switches between the
two. The choice is remembered.

This is not the burst that the play-view rebuild removes. The burst was a
mobile-only invention; this is the shared engine, so the two front ends show
the same thing and stay that way by construction.

## Decisions taken with the user (2026-08-10)

1. **The bands come from the precomputed spectrogram** — the same analysis the
   cover fog already runs on, played back against the position. No microphone
   permission, no `audiofx.Visualizer`. Tracks without an analysis show a
   resting visualizer; that is the accepted price.
2. **A tap on the cover switches**, and the choice persists across tracks and
   restarts.
3. **The visualizer takes the cover's square**, same size, same corners. The
   fog stays behind it — it is made from the cover, so the picture becomes
   coloured haze from the album with the bands in front of it. That is a view
   the desktop does not have.
4. **The double tap keeps its ∓10 s.** A tap therefore resolves about 250 ms
   late, because Compose has to wait out the double-tap window. Switching is
   rare, seeking is not.
5. **Kotlin draws only what the engine says.** No bars reimplemented in Kotlin
   — the look lives in Rust, or the two front ends drift apart.

## What already exists

`VisualEngine::ingest(&SpectrumFrame)` → `tick()` → `scene(w, h) -> Scene`, with
`Scene` a list of `Shape { geom: Polyline | Rect | RadialGlow, fill, width,
glow, dash }` in resolution-independent coordinates. GNOME renders it with
Cairo in `crates/reprise-gnome/src/ui/now_playing/song_visualizer.rs`. One mode
exists: bars. `SPECTRUM_BAND_COUNT` is 64; the mobile spectrogram carries 24.

# Packages

## Wave A — Rust, both independent

### V1 — spectrogram bands become a spectrum frame

New in `reprise-core`, next to the visuals module: a conversion from the
analysis' 24 bands to the engine's 64, plus the `BassPressure` the engine
expects. Interpolate across the band index — the 24 are already logarithmically
spaced — and do **not** apply another gain or smoothing: `from_cava_bars`
documents that it wraps an already-smoothed frame, and a second filter would
make the phone lag behind the desktop.

Tests: 24 constant bands give 64 constant ones; a monotone ramp stays monotone;
the edges do not clip or wrap; no NaN or infinity leaves the function for any
input, including an empty slice.

### V2 — the engine crosses the FFI

New `crates/reprise-android-ffi/src/visualizer.rs`: a UniFFI object holding one
`VisualEngine`, with `set_accent`, `set_playing`, `note_track_changed`,
`ingest_bands`, `tick` and `scene(width, height)`.

`scene` returns **one flat `Vec<f32>`**, not nested records: at 60 frames a
second with up to 64 bars, a `Vec<Shape>` with a `Vec` per shape would allocate
hundreds of times per frame across the boundary. Layout, one record per shape,
documented in the module and pinned by a test on both sides:

```
[ kind, r, g, b, a, width, glow, pointCount, x1, y1, x2, y2, … ]
  kind: 0 = rect (4 values: x, y, w, h), 1 = polyline, 2 = radial glow (3: cx, cy, r)
```

Tests: every shape the bars mode can emit survives an encode/decode round trip;
the buffer satisfies `Scene::is_finite_and_sane` for a range of sizes; a scene
requested before any ingest is empty rather than garbage.

## Wave B — Kotlin, V3 then V4

### V3 — the renderer

`VisualizerScene.kt`: decode the buffer and draw it — rect, polyline, radial
glow, and `glow` as the wide translucent under-stroke at three times the width
that `scene.rs` documents renderers should fake.

`NowPlayingVisualizer.kt`: the composable in the cover's square. It runs on the
same frame driver as the fog and obeys the same power gates — no frames with
the screen off or animations disabled. The accent comes from the cover's
ambient colours (`ArtworkVisual.ambientColors`), falling back to the theme
accent when a track has no artwork.

Tests: a Robolectric pixel test in the shape of the old `NowPlayingBurstPixels`
one — bars visible under a signal, still under silence, nothing drawn without
frames.

### V4 — the switch

Bring the persisted setting back to the FFI, renamed to what it now means:
`AndroidNowPlayingSurface { COVER, VISUALIZER }` under a `now_playing_surface`
key. The old `now_playing_view` row the play-view rebuild orphans stays where
it is; do not migrate it, it means something else.

A tap on the cover square cross-fades to the visualizer over 220 ms and back.
The fog does not fade — it belongs to neither and stays.

Tests: the tap switches; the choice survives a restart; the double tap still
seeks; the switch does not disturb the fog's rotation.

## Verification

`JAVA_HOME=/usr/lib/jvm/java-21-openjdk`, delete
`android/app/build/test-results/testDebugUnitTest` before the run and check the
XMLs are fresh, test *count* and *suite* count over the colour of the line —
the same rules the play-view rebuild runs under.

Visual acceptance on the emulator, then on the device: an analysed track
switching to bars and back, an unanalysed one showing the resting state, and
the fog still turning behind both.

## Out of scope

- More modes than bars. The desktop has one; when it gains more, this follows.
- Live FFT from the device output. Considered and declined with the permission
  it would cost.
- Showing the visualizer anywhere but the play view.

## Risks

- **Per-frame FFI cost.** If the flat buffer still costs too much, drop the tick
  rate to 30 fps before touching the format — the analysis itself only carries
  about 20 frames a second, so nothing is lost.
- **A wide-window mode inside a square.** The scene is resolution-independent,
  but bars laid out for a desktop window may look cramped at 272 dp. This needs
  a look before it needs a fix.
- **The engine's state is per track.** `note_track_changed` has to fire on
  every switch, or the bars carry the previous song's envelopes into the new
  one — the same class of bug the fog's rotation just produced.
