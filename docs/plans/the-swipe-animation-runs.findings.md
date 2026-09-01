# The swipe animation on the device — measured 2026-09-01

Follow-up to `the-swipe-animation-is-still-broken.HANDOVER.md`, which asked the
next session to measure the animation *as motion* rather than as endpoints, and
named "hard cut or real tween?" as the one question to settle.

It is a hard cut. The cause is a state-update race, found by instrumenting the
settle path on the device. Two defects, both fixed here.

## 0. The phone did not have the redesign on it

| | value |
|---|---|
| installed on the phone | `versionCode=74`, `versionName=0.1.74`, installed 2026-09-01 21:19:30 |
| the redesign (`1f8eacadc8`, #796) | `versionCode=83`, `versionName=0.1.83` |

Measured on the running 0.1.74: during a swipe the cover band translates
(275 → 616 px) while the **title band translates 0 px**. "The whole screen moves
with the swipe" is not in that build at all.

This does not establish *when* the user looked — only that the build sitting on
the phone cannot show the redesign.

## 1. Defect A — every title sat 152 px left of centre, at rest

`nowPlayingTitleTranslation` (`NowPlayingScene.kt:130`) was

    -positionPx * TITLE_PANEL_WIDTH_RATIO - (TITLE_PANEL_WIDTH_RATIO - 1f) * widthPx / 2f

At rest `positionPx` is 0 for the current panel, so the constant term stands
alone: `-0.141 * 1080 = -152.3 px`.

| build | title centre | artist centre |
|---|---|---|
| 0.1.74 | 540 | 540 |
| 0.1.83 as landed | **389** (clipped at x = 0) | **388** |
| 0.1.83, constant term removed | **541** | **540** |

An on-device A/B, not an argument: the term compensates for nothing. The
container it was meant to re-centre (`requiredWidth(maxWidth * 1.282)`) already
centres its text on the screen centre.

It also broke the panel symmetry — with it the left neighbour sat at `-1.423 w`
and the right neighbour at `+1.141 w`. A correct offset is odd in `positionPx`.

`NowPlayingPanelsTest.kt:113` only ever asserted `positionPx = 400f`, so the
resting case — the one a user looks at almost all the time — was never covered.
The rest case and the symmetry are now both pinned.

## 2. Defect B — the 480 ms settle was cancelled before it could render

### What it looked like

Release build, 120 fps capture, one `input swipe 860 920 220 920 250`:

`0 ms` card centred → `80–160 ms` card and title track the finger, title at
1.28× the cover (the specified 1.282 — the parallax itself was always correct) →
`235 ms` neighbour appears → **`250–266 ms` the outgoing track is back, fully
centred** → `275 ms` hard cut to the new track → still from `290 ms`.

Frame-to-frame measurement, release build, all motion after the finger lifts:

| | 0.1.74 | 0.1.83 |
|---|---|---|
| frames carrying motion | 27 | 8–17 |
| motion after release | ~230 ms | **~35 ms** |
| longest hold inside the gesture | 88 ms | 100–174 ms |

A 480 ms tween at 120 fps owes about 58 frames. It produced two.

### Why

`adb logcat` from an instrumented build, one swipe:

    settle begin decision=NEXT cur=0 target=1 changes=true anim=true w=1080.0 pos=581.1 targetPx=1080.0
    transport returned +18ms latestIndex=0 pos=581.1
    reconcile action=SNAP    track=396 cur=1 settling=1 pos=581.1 target=1080.0
    reconcile action=ANIMATE track=767 cur=1 settling=null pos=1080.0 target=1080.0

`settle done` never logs — the settle coroutine is cancelled part-way.

`NowPlayingPositionReconciler.update` guarded a running settle with

    if (previousTrackId != trackId && settlingTargetIndex == index) CONTINUE_SETTLE

The transport does not publish the index and the track in one step. Here the
index went 0 → 1 while `track.id` was still 396, so `previousTrackId != trackId`
was false, the guard did not fire, and the reconciler fell through to `SNAP`.
`Animatable.snapTo` cancels a running `animateTo` through the same mutex, so the
snap killed the settle and jumped straight to the target — the cut.

The mirrored ordering explains the rest of what the filmstrip shows: a new track
id arriving while the index still reads the one the swipe left makes the
reconciler `ANIMATE` back to that old anchor, which is the outgoing card
returning to the centre mid-exit.

**That guard was necessary but not sufficient.** With it in place the settle
still died, and a second probe run said why:

    BEGIN d=NEXT cur=0 tgt=1 anim=true pos=2774.4 tgtPx=1080.0
    reconcile=SNAP track=124 cur=3 settling=1 pos=2774.4 tgt=3240.0
    ABORT +128ms pos=2774.4 MutationInterruptedException

`cur=0` while the panel sits at 2774 px (anchor 2160, index 2) and the queue is
on index 3. The index in the settle closure is frozen at the value of the very
first composition.

`NowPlayingGestures.kt:123` was `pointerInput(animationsEnabled)`. A
`pointerInput` block is rebuilt only when its key changes, so the block — and
every callback it captured — lives as long as `animationsEnabled` holds still.
The module already reads `currentIndex`, `firstIndex`, `lastIndex` and
`positionPx` through `rememberUpdatedState`, which is why the *gesture* anchored
correctly at 2160; the *callbacks* were not, so `onSettle`, and through it
`settleTrack` with its captured `currentIndex`, aged with the block. Every
settle after the first track change therefore aimed at a panel the queue had
long left, and the reconciler cut off a settle that was genuinely heading the
wrong way. Its `SNAP` was the symptom; the stale closure was the cause.

**Fix, part one:** the six gesture callbacks now go through
`rememberUpdatedState`, like the values beside them already did.

**Fix, part two:** the reconciler guard now recognises both windows — a settle keeps ownership of the
position while the index has either arrived at its target or not moved at all
since the last update. A settle whose target the world has overtaken is
deliberately still handed over. Both orderings are pinned as tests.

### Verified on the device, release build, same gesture

    BEGIN d=NEXT cur=1 tgt=2 anim=true pos=1684.2 tgtPx=2160.0
    reconcile=CONTINUE_SETTLE track=767 cur=2 settling=2 pos=1684.2
    reconcile=CONTINUE_SETTLE track=124 cur=2 settling=2 pos=2150.4
    DONE +557ms pos=2160.0 latest=2

No abort, no trailing snap, and the second reconcile catches the animation at
2150 of 2160 px — it is genuinely in flight. 557 ms is the 480 ms tween plus the
transport call and scheduling.

Frame-to-frame, release build, before and after:

| | before | after |
|---|---|---|
| frames carrying motion | 8–17 | **31** |
| span of motion | 256–274 ms | **550 ms** |
| motion after the finger lifts | one 93 px jump | **a decelerating ramp** |

The tail is the tween's ease-out, one frame every 8.5 ms:
61, 26, 24, 22, 20, 18, 16, 14, 13, 11, 10, 9, 8, 7, 6, 5, 5, 4, 4, 3, 3, 2, 2,
2, 1, 1, 1, 0 — coming to rest at 550 ms, 1004 px travelled. The title rides it
at 1.25–1.31×, the specified 1.282.

The filmstrip no longer shows the outgoing card returning to the centre: the old
track slides out, the new one slides in and settles.

## 3. A frame-rate finding that did not survive its control arm

An earlier pass measured the redesign at ~40 fps with every frame over the
16.6 ms budget and reported it as a defect. That was a **debug** APK measured
against the user's **release** 0.1.74. Building the redesign as a release APK
and repeating the same three runs:

| | debug 0.1.83 | release 0.1.83 |
|---|---|---|
| frames per swipe | 15–27 | **55–69** |
| median vsync interval | 25–33 ms | **8.33 ms** |
| gaps > 20 ms per run | 13–16 | **0–1** |
| idle, playing | 25 ms cold / 8.3 ms warm | **8.33 ms, 0/120 over budget** |

There is no frame-rate defect. The stutter numbers were the debug build and the
JIT warming up. Recorded here because the retracted claim is as much a part of
the measurement as the surviving one.

## 4. Gates

`scripts/check-android-suite.sh` passes with no failures. A bare
`./gradlew :app:testDebugUnitTest` is not a substitute — in a fresh worktree it
fails every Robolectric class with `ExceptionInInitializerError at
NativeLibrary.java:325`, on unmodified HEAD as much as with these changes, since
only the script builds the host `libreprise_android_ffi.so` and puts it on
`LD_LIBRARY_PATH`.

## 5. Still open

The incoming track shows a **placeholder note instead of its cover** for at
least 600 ms after the swipe, on both the neighbour panel during the drag and
the new current panel after the commit. The design has the neighbour carrying
its own artwork throughout. Not diagnosed here; it is a prefetch question, not a
motion one.

## Instruments, for the next session

Under `$SCRATCH` and worth keeping:

- `motion.py` — frame-to-frame alignment of the cover and title bands with true
  PTS. Consecutive frames always show nearly the same content, so this stays
  well posed across a track change, where alignment against a resting reference
  stops meaning anything. Guard against the degenerate case: identical frames
  make every shift equally good and `argmin` then reports the extreme.
- `fs2.py` / `fs3.py` — `dumpsys gfxinfo … framestats` reduced to vsync
  intervals and the per-phase breakdown. Independent of the recorder.
- `run-swipe.sh`, `extract.sh` — capture and PTS-preserving frame extraction.

Three traps that cost time here:

- `screenrecord` at 24 Mbit/s is itself load; take frame-pacing numbers from
  `framestats` without the recorder running.
- A `uiautomator dump` loop during a capture steals enough CPU to halve the
  frame rate. Do not poll the UI while measuring motion.
- The screen sleeping mid-capture looks exactly like a black animation.
  `adb shell svc power stayon usb` first.

## 2026-09-02 — the cover hypothesis in the handoff is refuted

The handoff proposed that the prefetch warms `AndroidArtworkSize.LIST` while the
panels request `NOW_PLAYING`, and that those separate shelves make the prefetch
unable to warm what the panels read. **That is wrong**, on four pieces of
primary evidence:

- `ArtworkCache.kt:105-120` — `seedArtwork` explicitly bridges sizes. After the
  exact-size lookup misses it scans *every* shelf for a `TrackArtworkKey` whose
  identity matches the request, and returns that visual.
- `ArtworkCache.kt:208-209` — `matchesIdentity` compares `trackUri` and `kind`
  only. Size is deliberately not part of it, so a LIST entry answers a
  NOW_PLAYING seed.
- `ArtworkCacheTest.kt:41-49` — an existing, green test already asserts exactly
  that: `putArtwork(request("direct", LIST))` then `seedArtwork(request("direct"))`
  at NOW_PLAYING returns the same visual.
- `MobileSurfaceViewModel.kt:288-290` — the prefetch's own comment documents the
  LIST shelf plus cross-size seed as the intended design, not an oversight.

### What the symptom actually is

The note is **not** `CoverPlaceholder` (`LibraryFrame.kt:358`). `seedVisual`
(`TrackCover.kt:283`) can never return null — it falls back to
`generatedVisual(request, resolved = false)`, and `fallbackCoverBitmap`
(`FallbackCover.kt:35`) draws a gradient with `drawRestrainedNote` on it. So the
panel is showing the *generated* cover, which is what a cold seed is supposed to
produce.

Both symptoms the handoff lists — the neighbour during the drag and the ≥1.5 s
on the new current panel — are the same single mechanism: a cold seed followed
by the async `loadVisual`. They are not two defects.

The open question is therefore narrower than the handoff states: **why is the
LIST shelf cold for the incoming track?**

### Two findings that are real but are NOT this bug

- The prefetch window is **not** off by one, though it reads that way. A first
  pass here recorded `ANALYSIS_PREFETCH_OFFSET = -3` / `LIMIT = 5` as −3…+1
  against a doc comment promising −2…+2. That was a misreading:
  `queue_boundary.rs:56-75` starts the window at `queue_window_start`, the track
  *after* the current one, so `(-3, 5)` is current−2…+2 exactly as documented,
  and the panels' own `(-2, 3)` (`NowPlayingGestures.kt:94`) is current−1…+1,
  fully inside it. Do not "fix" these constants.
- `NOW_PLAYING_ARTWORK_CAPACITY = 3` (`ArtworkCache.kt:12`) while `putGenerated`
  (`ArtworkCache.kt:182`) writes into the same shelf as resolved covers. Three
  panels can insert up to six entries per track change into three slots. That
  thrashes, but evicting a NOW_PLAYING entry only costs a re-decode, because the
  cross-size `TrackArtworkKey` match still finds the LIST entry ahead of any
  generated key. A performance defect, not the note.

### 2026-09-02 — four distinct symptoms, reported live from the device

The user watched the fixed build by hand and reported more than the handoff had:

1. The neighbour panel shows the generated fallback cover instead of the real one.
2. **The real next cover flashes up and then disappears again.** This is the
   most informative of the four: a cold cache cannot flash a real cover, so the
   artwork *is* loaded at that moment and something discards it afterwards. It
   moves the suspicion off the prefetch and onto the delivery path.
3. A white line at the top edge keeps moving after the cover has already landed.
4. With the visualiser on, the neighbour panel shows a cover rather than the
   visualiser. This one is by design, not a defect: `NowPlayingScene.kt:344`
   passes `live = panel.index == currentIndex`, so the scene engine runs only in
   the current panel. It still contradicts what the user expects to see.

### A structural claim that turned out to be wrong

An earlier pass here read `withCurrentPanel` (`NowPlayingGestures.kt:46-62`)
dropping the forward panel and keeping `firstIndex`/`lastIndex` unchanged as a
defect. It is not: `NowPlayingPanelsTest.kt:49`
(`a_known_index_change_replaces_the_centre_without_discarding_its_neighbour`)
covers exactly this, and the forward panel genuinely cannot exist yet — the old
window only held three tracks. Do not "fix" this.

### The mechanism still to be measured

`rememberTrackArtworkVisual` (`TrackCover.kt:275-290`) holds its visual in
`remember(request, artwork)` and starts the load in a `DisposableEffect` whose
`onDispose` calls `gate.invalidate(admitted)`. When the panel window churns —
`withCurrentPanel` first, the async `loadUpcomingTracks` reload second — a panel
subtree can be disposed and rebuilt. That discards both the delivered visual and
the in-flight decode, which would produce exactly symptom 2 followed by
symptom 1. **Not yet measured. Needs frames, not argument.**
