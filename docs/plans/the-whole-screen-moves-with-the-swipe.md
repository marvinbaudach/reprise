---
slug: the-whole-screen-moves-with-the-swipe
worktree:
branch:
phase: planned
codex_session:
created: 2026-08-31
strands: a,b
merge_order: a,b
---
# The whole screen moves with the swipe

Mother plan. The work lives in `-a.md` (neighbour data) and `-b.md` (the whole
Compose surface). Read this file first: it carries every number, every decision
and the checks no strand can make alone.

## What the user reported

> "man sieht nicht dass man gerade zum nächsten song geswipet hat"
> "wäre es nicht besser auch titel und seek mit der karte zu werfen? so sieht
> die animation nur aufs coverbox beschränkt komisch aus"

Followed by a full design, delivered as a prose spec plus an interactive
prototype. Both live in the Claude Design project
`bd4103fc-2fae-465b-8fdf-afb8268f143a` (`prompt-swipe-animationen.md`,
`Player Swipe.dc.html`).

**Codex cannot reach that project.** The claude-design MCP is main-thread only
and is not available inside a worktree run. Every number, colour and formula the
implementation needs has therefore been transcribed into this file. If something
is missing here, it is missing — do not assume it can be looked up.

Android only. There is no swipe or carousel call site under
`crates/reprise-gnome/src`; the gesture exists solely in the Compose app.

## What this supersedes

PR [#771](https://github.com/marvinbaudach/reprise/pull/771) and three branches,
all unmerged and all abandoned:

- `feature/android-swipe-discards-the-whole-card`
- `feature/the-card-leaves-with-its-own-cover`
- `feature/the-neighbour-plate-obeys-the-visualizer`

#771's on-device after-arm failed: the cover window shrank from 634 ms to
265 ms instead of reaching 0. The cause was a slot its plan put out of scope —
the neighbour preview at `NowPlayingScene.kt:210-228` has its own binding and
was never latched, so a second card slid in from the right already carrying the
incoming cover. This plan does not patch that. The latch it tried to repair is
deleted: one continuous position replaces the offset-plus-latch model, and a
displaced neighbour showing the next track is no longer a defect but the point.

**Close #771 unmerged.** Its knowledge is in this file; its code is superseded in
shape, not merely in parameters.

## Why the current code cannot express this

`horizontalOffsetPx` is read only inside the `Canvas` draw block of
`NowPlayingScene.kt` (lines 154, 171, 175, 210, 220). `translationX` appears
nowhere for the card. `PlayedHeader`, `SceneTitle`, `SceneProgress` and
`SceneTransport` (`NowPlayingScene.kt:266-289`) are absolutely positioned
children of a `BoxWithConstraints` — `.align(...)` plus `.offset(y = ...)`, no
horizontal offset anywhere. A canvas `rotate`/translate cannot move a Compose
child, which is why every previous attempt was cover-only.

The model is also the wrong shape: a single `horizontalOffset` `Animatable` plus
an `outgoingTrack` latch. Two CRITICAL defects were found in that latch — a card
that can stay stuck off-screen when `next()` does not change the track id, and a
second touch cancelling the exit coroutine before the transport call. Both are
deleted rather than repaired.

## The model

One state value drives everything:

```
pos  = index * screenWidth + dragDelta        // pixels, absolute across the queue
f    = pos / screenWidth                      // fractional index
dist = min(1.6, |i - f|)                      // per panel i
near = max(0, 1 - min(1, |i - f|))            // per panel i
dev  = pos - index * screenWidth              // signed deviation from the current index
off  = min(1, |f - index|)
```

`index` is the **absolute queue position**, and it already exists:
`PlaybackUiState.kt:11` carries `currentIndex: Int?`. It is currently never read
by the now-playing screen. Strand A plumbs it through; strand B consumes it.

Every layer derives its transform from `pos` with its own factor. **No layer runs
an animation of its own.** While the finger is down there is no transition at
all — the layers follow the finger 1:1. On release, one settle animation moves
`pos` to `target * screenWidth`.

Panel `i` sits at `i * screenWidth - pos`. This is the only positioning rule;
everything below is a factor on that displacement.

### Per-panel scalars

| Quantity | Formula | At rest | At distance 1 |
|---|---|---|---|
| box scale | `1 - dist * 0.13` | 1.0 | 0.87 |
| box rotation | `clamp(i - f, -1, 1) * -3.5°` | 0° | ∓3.5° |
| box opacity | `max(0, 1 - dist * 0.75)` | 1.0 | 0.25 |
| title opacity | `max(0, 1 - dist * 1.35)` | 1.0 | 0 at dist 0.74 |
| blur | `(1 - near) * 5` px | 0 | 5 |
| saturation | `0.4 + near * 0.6` | 1.0 | 0.4 |
| glow opacity | `max(0, 1 - |i - f| * 1.1)` | 1.0 | 0 at dist 0.91 |

Box opacity reaches exactly 0 at `dist = 1.333`. This is the fact that settles
the window size: a panel two steps out is never visible. See "The window".

### Layer translations

| Layer | Translation |
|---|---|
| ambient glow / fog | `(i - f) * screenWidth * 0.23` — per panel, see below |
| cover / visualizer box | `i * screenWidth - pos` (factor 1.0) |
| title + artist | panel width `screenWidth * 1.282`, row translated `-pos * 1.282 - (1.282 - 1) * screenWidth / 2` |
| waveform + times | does not travel: `-off * 70` px, opacity `1 - off * 0.9`, `scaleX 1 - off * 0.06` |

The faster title layer is the effect that makes the swipe legible. It is not
decoration; it is the reason the change is visible at all.

The design fixes the title panel at 500 px against a 390 px screen. `1.282` is
`500 / 390`; keep it as a named ratio applied to the real screen width, not as a
hard 500 px.

### The glow is per panel, not one layer

**Correction against the first draft.** The prototype renders one glow *per
track*, each with its own hue, translated by its own displacement and faded by
its own distance. A single background layer translated by `pos * 0.23` would lose
the per-track colour entirely.

The user's decision — "the prose factor `0.23`, not the prototype's
`(i - f) * 90 * 0.23`" — resolves inside that per-panel structure: the prototype
scaled `0.23` against a literal 90 px instead of the screen width, which is why
it moved ≈21 px per step where the prose asks for ≈90. The corrected formula is
the panel's own displacement times the factor:

```
glowTranslateX = (i - f) * screenWidth * 0.23        // ≈ 90 px per step at 390 px
glowOpacity    = max(0, 1 - |i - f| * 1.1)
```

`0.23` must be a single named constant so the glow can be retuned in one edit.
The user chose this knowing the glow moves ≈4× further than the prototype they
saw.

## Commit rule

- Commit when `|dev| > 0.22 * screenWidth` **or** velocity `> 0.55 px/ms`.
- Otherwise spring back to the current index.
- Velocity is `|d| / max(60ms, Δt)`. The 60 ms floor is deliberate: it defuses
  very short flicks. Keep it.
- Settle: `480 ms`, `cubic-bezier(.22, 1.06, .32, 1)` — slight overshoot.
  Opacity settles at `420 ms` ease. During the drag: **no transition at all.**
- At the ends of the list: rubber band, movement damped to `0.3`, no hard stop.
  Beyond the first panel `pos *= 0.3`; beyond the last
  `pos = max + (pos - max) * 0.3`.

Today's thresholds are replaced:

- `TRACK_DISTANCE_FRACTION = 0.25f` (`PlayGestureState.kt`) → `0.22`.
- `TRACK_FLING_DP_PER_SECOND = 800` (`NowPlayingGestures.kt:148`) → the design's
  `0.55 px/ms`.

**These are different units.** `0.55 px/ms` is 550 **physical px per second**;
the existing constant is in **dp per second**. The conversion must be explicit
and the surviving constant must be named in one unit only. Do not leave a
constant whose name says dp and whose value is px.

## The four confirmation cues

All four fire together, and **only on a real track change** — never on first
render.

Note a divergence inside the design itself: the prototype gates the sweep and the
ring behind a `moved` flag but leaves the waveform build ungated, so it also runs
on first paint. The prose is explicit that all four are change-only. **The prose
wins**; gate all four.

The prototype forces an animation restart by alternating two identical CSS names
(`waveA`/`waveB`, `ringA`/`ringB`, `sweepA`/`sweepB`). In Compose the equivalent
is keying the animation on the track id. Precedent that already skips the first
composition: `CoverFogBitmap.kt:120-139` guards its crossfade with
`if (hadCurrent)` and `snapTo`s otherwise.

1. **Waveform build** — bars rise from `scaleY 0.1` / opacity `0.1` to full,
   `560 ms`, `5 ms` stagger per bar, left to right,
   `cubic-bezier(.22, 1, .36, 1)`.
2. **Accent line at the top edge** — 2 px tall, gradient
   `transparent → accent → transparent`, `scaleX = min(1, |dev| / (screenWidth * 0.22))`,
   transform origin **right** when dragging forward (`dev > 0`), **left** when
   back, opacity 1 only while the finger is down. This is a *pre*-indicator: it
   reaches full scale exactly at the commit threshold and so promises the swipe
   will take. A mismatch between this constant and the commit constant lies to
   the user — they must be the same constant, not two copies.
3. **Sweep** — a thin `accent-200` gradient line runs once across the top edge on
   commit: `translateX -100% → 100%`, opacity `0 → 1 at 25% → 0`, `620 ms`,
   ease-out.
4. **Play-button pulse** — a 1 px accent ring, radius 26, scales `0.9 → 1.9` and
   fades `0.5 → 0`, `620 ms`, ease-out. Plus a light haptic on commit.

**Haptics (settled).** Reuse the existing abstraction in `QueueHaptics.kt` rather
than adding a second path: it already runs a real `Vibrator` with
`VibrationEffect.createWaveform`, falls back to `LocalHapticFeedback`, and
consults `Settings.System.HAPTIC_FEEDBACK_ENABLED` itself (`:86-92`).
`android.permission.VIBRATE` is declared (`AndroidManifest.xml:29`). Add a
`commit()` pulse: `longArrayOf(0, 12)` with `HapticFeedbackType.TextHandleMove`
as the fallback — between `crossedBoundary()` (8) and `dropped()` (14).

## The box and its neighbours

The box shows album art **or** the visualizer. Both modes ride exactly the same
`pos` track and the same factors.

- **Cover mode** — the neighbour's artwork arrives at 5 px blur and 0.4
  saturation and sharpens as it approaches.
- **Visualizer mode** — the neighbour cannot have live audio. It shows **its own
  album cover**, blurred and desaturated, which fades out while the bars rise:

  ```
  neighbourCoverOpacity = 1 - near^1.6
  neighbourBarOpacity   = near^1.4
  neighbourBarHeight    = h * (0.3 + near * 0.7)
  ```

  At the centre the live visualizer takes over continuously. No jump, no flash.

**Correction against the first draft**, which described damped bars with no cover
underneath. That is the prototype's `scaffold: 'peaks'` variant. The default the
user saw and approved is `scaffold: 'cover'`, and the prose says so explicitly:
"Statt eines Balken-Skeletts zeigt er im Stillstand sein normales Albumcover
(5 px Blur, 0,4 Sättigung)."

This means a neighbour in visualizer mode needs **both** its artwork and its
spectrogram prefetched.

**No neutral scaffold plate.** `NowPlayingNeighbourScaffold` is not carried over.

## The window

**Render ±1, prefetch ±2.**

Rendering follows from the design's own opacity formula: box opacity is
`max(0, 1 - dist * 0.75)` and reaches 0 at `dist = 1.333`, so a panel two steps
out can never be seen. Rendering it costs a composition and buys nothing.

Prefetching one further is not an optimisation. After a commit the panel that was
`index + 1` becomes the centre and a *new* `index + 2` becomes the neighbour. If
the window were only ±1, two quick swipes in a row would show an empty neighbour
— which is a variant of the very bug this work exists to fix.

Cost: 2 artworks and 4 analysis fetches held warm instead of 1 and 2.

## Fog and shimmer

Today the scene draws a **pair** of fogs and a **pair** of shimmers — `previous`
and `current`, crossfaded by `fog.fraction` (`NowPlayingScene.kt:180-209`) — and
shifts them horizontally by `FOG_SWIPE_DISTANCE_FACTOR = 0.35f`
(`NowPlayingScene.kt:605`, applied at `:175`).

That whole construction is replaced by the spatial rule. **One fog per panel**,
translated by `(i - f) * screenWidth * 0.23`, faded by
`max(0, 1 - |i - f| * 1.1)`. The `previous`/`current` pair and its `fraction`
crossfade are deleted; distance *is* the crossfade now. `FOG_SWIPE_DISTANCE_FACTOR`
is deleted with them.

The shimmer is anchored to its fog's centre already
(`NowPlayingShimmer.kt:71`, `drawNowPlayingShimmer(fog, center, …)`), so it
travels with its panel and takes the same opacity. No separate treatment.

Per-track fog identity is preserved and needs no new mechanism: the palette
already comes from the artwork, blended toward `VisualizerRampPalette` by
`visualizerOpacity`. The design's "hue per track" *is* this palette. Do not add a
hue rotation.

Definitions, for the record: `drawPlayedNowPlayingFog` (`NowPlayingScene.kt:336`)
→ `drawNowPlayingFog(palette, center, seconds, level, opacity, driftEnabled)`
(`NowPlayingFog.kt:64`); `drawPlayedNowPlayingShimmer` (`:358`) →
`drawNowPlayingShimmer(fog, center, coverDiameterDp, elapsedSeconds, swell, opacity, rotationsEnabled, alphaScale)`
(`NowPlayingShimmer.kt:71`).

## The seek track

**Adopt the design's marker and keep the alpha encoding.** The design has both:
played bars are tinted (`color-mix(accent 45 + h·0.5%, neutral-800)`) while
remaining bars sit at `neutral-800`, *and* a 3 px `accent-200` marker with
`box-shadow: 0 0 12px accent` stands at `playedPct`.

The app already has the tint's equivalent — `PLAYED_ALPHA = 0.96f` vs
`REMAINING_ALPHA = 0.34f` (`SpectralSeekTrack.kt:33-34`, applied at `:69-76`).
What it lacks is the marker, and what it has instead is Material3's stock Slider
thumb: `NowPlayingSheet.kt:363-376` passes `track = { SpectralSeekTrack(...) }`
and no `thumb =` at all.

So: pass `thumb = {}` and draw the marker inside `SpectralSeekTrack`, which
already receives `displayed` and `durationMs`. **The `Slider` itself stays** — it
carries the drag-to-seek input, `onValueChangeFinished`, the `enabled` gate and
the accessibility semantics. Removing it would silently drop TalkBack support.

## Auto-advance and a queue that moves

**Auto-advance uses the same settle as a commit.** A track ending on its own is
not visually a different event from pressing next — the prose already requires
that the previous/next buttons take the same path as the swipe, and end-of-track
belongs on that path too. `pos` animates to `index * screenWidth` with the same
480 ms curve.

There is no way to distinguish an automatic advance from a user-initiated one at
the Compose layer, and none is needed: both arrive as a new `currentTrackId` in
the same `LibraryPlayback` snapshot (`PlaybackUiState.kt:23-28`). Do not invent
an event flag.

**If it arrives during a drag, re-anchor.** The commit rule reads
`dev = pos - index * screenWidth`. When `index` moves under an active drag, `dev`
jumps by almost a full screen width and a small forward drag would read as a
large backward one and commit the wrong way. So on an external index change while
dragging, reset the drag origin against the new index so `dev` restarts at 0; the
finger keeps adding delta from there.

There is **no such guard today** — nothing resets the offset animatables when the
track changes externally, in gestures or in the sheet. This is new work, not a
regression to preserve.

**A queue edit that does not change the current track re-seats silently.**
Reorder, removal or enqueue can move the cursor without changing what is playing.
When `currentTrackId` is unchanged, set `index` and reload the panels with no
motion at all: the user did not ask for movement and a player that slides because
something was enqueued elsewhere reads as a bug. Only a changed `currentTrackId`
runs the auto-advance path.

Precedent for the reload guard: `NowPlayingQueue.kt:47,64-66` already uses a
`generation` counter to discard answers that a mid-flight edit invalidated. Reuse
that shape rather than inventing one.

## Known defect to fix on the way

`drawPlayedCover` (`NowPlayingScene.kt:565-595`) draws the cover shadow at
`shadow?.let { drawCoverShadow(it, rect) }` **before** the `if (opacity <= 0f) return`
guard, and `drawCoverShadow` takes no alpha at all. Any damped or faded cover
therefore keeps a full-strength shadow — a dark silhouette with nothing inside
it. With per-panel opacity going to 0.25 and below, this becomes visible on
every swipe rather than only in the visualizer case where it was first found.

## Design tokens

Transcribed from the design system `nocturne-8bc6e4d1-56d3-4ff1-a399-897a2df571af`,
because Codex cannot read it. The app has its own `NocturneTheme.kt` — **expect a
reconciliation, not an import.** Where the app already has an equivalent token,
keep the app's; use these only for what the app lacks.

```
--color-bg          #161826      --color-surface     #232532
--color-text        #e9e9ed      --color-accent      #9184d9

--color-accent-100  #f5f4ff      --color-accent-500  #968ae0
--color-accent-200  #e7e5fe      --color-accent-600  #796cbf
--color-accent-300  #d2cefd      --color-accent-700  #5d5294
--color-accent-400  #b5abfc      --color-accent-800  #423a6a

--color-neutral-100 #f3f5fe      --color-neutral-500 #9397ab
--color-neutral-200 #e4e7f5      --color-neutral-800 #3f424d
--color-neutral-300 #cfd3e5      --color-neutral-900 #292b31

--font-heading      Inter, weight 500, letter-spacing -0.01em
--space-2 5.6px   --space-4 11.2px   --space-6 16.8px
--shadow-lg   0 0 0 1px #9397ab, 0 16px 40px rgba(0,0,0,0.65)
```

Geometry from the prototype, at a 390 × 844 frame: box 262 × 262, radius 26,
cover row height 386, title row height 92, waveform height 66, play button
78 × 78 with radius 26, top accent line 2 px.

The glow is two radial gradients — `radial-gradient(42% 52% at 34% 42%, accent-700, transparent 72%)`
and `radial-gradient(38% 46% at 72% 26%, accent-800, transparent 74%)` — in a box
at `left -25%, top -8%, width 150%, height 62%`, blurred 46 px, under a scrim
`linear-gradient(180deg, transparent 0%, bg@55% at 58%, bg 82%)`. Reconcile
against the app's existing fog rather than reproducing this literally; it is
recorded so the intent is not lost.

## What is explicitly not wanted

- No pagination dots — meaningless for large queues.
- No fade-out/fade-in as the track change; the movement carries the information.
- No animation that does not mirror the finger directly. Nothing of the shape
  "swipe detected → play an animation".
- Previous/next buttons trigger the *same* commit animation as the swipe, not a
  second code path.

## Settled in the grill (2026-08-31)

**G1 — the left panel follows queue order, not playback history.**
`previous()` today calls `previous_from_history()` (`playback_session.rs:657` →
`history.rs:146`). The design's spatial model needs left/right to be reversible,
so the player navigates `queue[cursor - 1]` instead. The building blocks exist
and need no new data structure: `queue.rs:605 current_order_position`,
`:612 id_at_order_position`, `:622 jump_to_order_position`, `:672 ids_in_order`.
`upcoming_tracks` (`queue_boundary.rs:56`) is the same loop, only clamped forward
from `queue_window_start` (`queue_boundary.rs:283`) — a symmetric window around
the cursor reuses it.

Mitigating fact found while grilling: with shuffle on, `set_shuffle` reorders the
queue itself and the cursor walks that shuffled order, so `queue[cursor - 1]` is
normally the track just heard anyway. The two semantics diverge only after a jump
(playing a track directly, then going back).

**G2 — the switch is global on Android, GNOME keeps history.**
Every Android surface gets one meaning of "previous": the player swipe and
button, plus `CoreControlledPlayer.kt:46,50` (`seekToPrevious` /
`seekToPreviousMediaItem`), which is what the media notification, Bluetooth and
headphone buttons, the lock screen and Android Auto call. Also `DockMode.kt:149`.
`crates/reprise-gnome/**` is deliberately **not** touched, so the two frontends
diverge on this point until someone decides otherwise. That divergence is
accepted, not overlooked.

**G3 — one Compose element per panel, not one canvas for everything.**
Each panel is its own `Box` carrying `graphicsLayer` (scale, rotation, alpha),
`Modifier.blur` and a saturation `ColorFilter`, with the cover or visualizer
drawn inside it. This is the larger restructure — cover, visualizer and fog draw
routines must be re-hosted per panel — but it is the only shape that yields the
design's blur and saturation without hand-rolling them.

**G4 — render ±1, prefetch ±2.** See "The window".

**G5 — adopt the seek marker, keep the alpha encoding, keep the Slider.**
See "The seek track".

**G6 — the neighbour in visualizer mode shows its cover crossfading into bars.**
See "The box and its neighbours".

**G7 — fog and shimmer become one per-panel layer at factor 0.23.**
See "Fog and shimmer".

**G8 — auto-advance settles like a commit and re-anchors mid-drag; a queue edit
that leaves the current track alone re-seats silently.** See "Auto-advance".

**G9 — haptics reuse `QueueHaptics`.** See "The four confirmation cues".

**Decided without asking, both with existing precedent in the codebase:**

- *Blur is API 31+ and `minSdk` is 26* (`android/app/build.gradle.kts:56`;
  `targetSdk = 37` at `:57`). `Modifier.blur` is silently inert below 31;
  `OilFilmPalette.kt:58` already records this boundary and `AmbientSurface.kt:178`
  already lives with it. Accepted: on API 26-30 neighbours stay sharp but are
  still scaled, desaturated and faded, which carries the "not current" signal on
  its own. Saturation works everywhere — `ColorFilter.colorMatrix` is not a
  `RenderEffect`.
- *Reduced motion stays as today:* immediate track change, no parallax, no cues.

## The cut

**Two strands, not three.** The draft proposed a third strand for the four cues.
The mandatory disjointness check killed it: every cue is inserted into a file the
motion strand owns — the marker needs `thumb = {}` in `NowPlayingSheet.kt`, the
accent line and sweep sit at the top edge of that same layout, the ring sits on
the play button, the haptic hangs off the commit in `PlayGestureState`. Two of
them also read live values from the motion strand (`dev` for the accent line, the
commit event for sweep, ring and haptic), and the accent line must share the
`0.22` constant *literally* or it lies to the user. A "disjoint" strand with four
seams and two shared constants is not disjoint.

**Strand A — the neighbour data (`-a.md`).** Rust, FFI and the Kotlin data layer.
Makes a symmetric neighbour window, an absolute index and warm analysis data
available, so the surface strand has something to render on both sides.

**Strand B — the whole Compose surface (`-b.md`).** The `pos` model, the per-panel
hosting, fog and shimmer, the seek track, and all four cues.

**Merge order: A, B.** B cannot render a previous panel without A's window, and
cannot seat `pos` without A's `currentIndex`.

B is large — roughly eleven files. If one Codex run does not carry it, split it
**sequentially inside the same worktree**: B1 the motion model and per-panel
hosting, B2 the four cues and the seek marker. Do not split it into parallel
strands; that is the cut this grill already rejected.

## Post-merge cross-checks

None of these can run inside a single strand.

- The neighbour's cover *and* its static visualizer both have data when it
  scrolls in — A's ±2 prefetch against B's panel rendering, in both box modes.
- The accent line reaches full scale exactly at the commit threshold, because it
  reads the same constant B commits on. Assert the constant is shared, not equal.
- The waveform build fires exactly once per real track change and not on first
  render.
- Rest state is bit-exact: at `dev == 0` no rotation matrix is applied, opacity
  is a bit-exact `1f`, and translation is exactly `0f`. Asserted, not assumed.
- Two quick swipes in a row show a populated neighbour both times — the ±2
  prefetch's reason for existing.
- The on-device arm, with the animation scale asserted at 1.0, against a control
  recording from the same worktree with only the motion changes reverted.

## Verification

**The reduced-motion test the draft claimed exists does not exist.**
`reducedMotionAdvancesImmediatelyBecauseThereIsNoExitWindow` is nowhere in the
tree. The nearest thing is `ScenePowerGateTest.kt:9-22`
(`animations_off_suppresses_fog_rotation_without_stopping_scene_frames`), which
asserts `assertFalse(power.fogRotates)` and `assertTrue(controller.sceneFramesAllowed)`.
So reduced motion is **not** covered today, and a test for it is work this plan
owes, not coverage it inherits.

Note also what the flag means: `sceneAnimationsEnabled` (`AmbientSurface.kt:46-47`)
is `attachedSurfaces > 0 && resumed && screenInteractive && systemAnimationsEnabled`,
where the last term comes from `Settings.Global.ANIMATOR_DURATION_SCALE`
(`AmbientRuntime.kt:64`). It is a power gate that happens to include the system
animation scale — not a user-facing reduced-motion preference.

With it false, the whole parallax and all four cues must degrade to an immediate
track change with no motion.

**Device measurement has a precondition that has already cost this work once:**
the Pixel's three animation scales must be `1.0`. At `0` the app correctly
honours reduced motion and every recording — control arm and fix arm alike —
captures an empty animation, which reads as "no change" while nothing was
measured at all. The measuring script must check the scale before it records and
refuse otherwise.

## Traps paid for already

- **The device's animation scales must be `1.0`** before any on-device judgement.
  Check, do not assume; anything can set them back. Memory:
  `animation-scale-zero-voids-every-android-motion-test`.
- **Never `pgrep`/`pkill` by name here.** It matches your own command line, and
  parallel sessions run their own Codex in other worktrees. Identify processes
  through `/proc/*/exe` and check `/proc/*/cwd` before killing.
- **`git stash` is repo-global.** With this many worktrees it is never the right
  tool. Save a patch to the scratchpad instead.
- **Display tests run one test per process.** `scripts/check-display-tests.sh`
  gives each its own `--exact`, XDG roots, D-Bus session and X server. Running
  the group in one process aborts for environment reasons that say nothing about
  the code.
- **Codex writes `phase: shipped` into the plan on its own.** That line belongs
  to `land.sh`. Reset it after a run and say so in the prompt.
- **The Pixel 10 Pro XL (`59100DLCQ006SB`) currently runs a debug build 0.1.74
  from the abandoned throw branch.** Reinstall before judging anything. The debug
  APK needs a fresh arm64 `libreprise_android_ffi.so` or the app dies on launch
  with `UnsatisfiedLinkError`.
- The card's rest bounds on that device: left 208, right ≈870, centre 540, width
  ≈662 of 1080. The left edge is the reliable signal; right-edge detection is
  fooled by album-art content.
- A wider swipe than `input swipe 700 770 260 770 250` is caught by the tab pager
  and jumps to the Queue tab.
