---
slug: the-throw-and-the-plate-land-together
worktree: /home/marvin/Projects/reprise-the-throw-and-the-plate-land-together
branch: feature/the-throw-and-the-plate-land-together
phase: superseded
codex_session:
created: 2026-08-31
---
# The throw and the plate land together

> **Superseded since 2026-09-01, and parked unmerged.** The ten commits of
> `feature/the-throw-and-the-plate-land-together` were written, tested (542
> Android tests) and measured on the device — the run log is
> `the-throw-and-the-plate-land-together.device-arms.md`, and it says the fix
> works. They are not landing. `the-whole-screen-moves-with-the-swipe.md`
> reached `origin/dev` in the meantime and deletes the very mechanism this
> plan repairs: *"The latch it tried to repair is deleted: one continuous
> position replaces the offset-plus-latch model, and a displaced neighbour
> showing the next track is no longer a defect but the point."* That plan also
> lists this one's own base, `feature/the-neighbour-plate-obeys-the-visualizer`,
> among three branches "all unmerged and all abandoned".
>
> Both defects this plan names are already answered there, not by parameters
> but by shape:
>
> - **F-LATCH** — a continuous `pos = index * screenWidth + dragDelta` has no
>   instant at which content and offset can disagree, so there is nothing to
>   latch.
> - **F-PLATE** — answered by decision, not by repair, and the decision goes
>   the other way. `the-whole-screen-moves-with-the-swipe.md` specifies the
>   neighbour in visualizer mode as showing **its own album cover**, blurred
>   and desaturated, fading out as the bars rise:
>   `neighbourCoverOpacity = 1 - near^1.6`. It records this as what the user
>   saw and approved (`scaffold: 'cover'`, *"Statt eines Balken-Skeletts zeigt
>   er im Stillstand sein normales Albumcover"*), correcting its own first
>   draft which had damped bars and no cover. Strand -b implements exactly
>   that: `coverOpacity = 1f - visualizerOpacity * near.pow(1.6f)`
>   (`NowPlayingScene.kt:333-337`), identical to the spec at
>   `visualizerOpacity = 1`.
>
>   So D1's visualizer sentence — *no full-strength album artwork anywhere in
>   the card band* — is **superseded by a design decision**, not satisfied.
>   Anyone pointing this log's 0.015 brightness gate at the redesign will get
>   a failure, and it will be the gate that is wrong. That warning is the
>   single most useful thing this document can hand forward.
>
> The D6 note below predicted exactly this and was right: *"A latch built on
> `horizontalOffsetPx` does not survive it."*
>
> What survives is the measurement, and it is the reason this pair of documents
> was lifted onto `dev` instead of dying with the branch. See the device-arms
> log's closing section for the arms the redesign inherits — including a
> previous-direction regression that this branch found and that nothing in the
> 542-test suite catches.

> Supersedes `android-swipe-discards-the-whole-card.md` (PR #771, after-arm
> failed) and `the-card-leaves-with-its-own-cover.md` (never implemented), and
> absorbs `android-swipe-visualizer-gap.HANDOFF.md`. It does **not** supersede
> `the-whole-screen-moves-with-the-swipe.draft.md`, which is a separate feature.

## Why

Three things the user said, over two sessions:

> "Swipe für den aktuellen Song bei visualization ist nicht korrekt. Man sieht
> kurz das Cover des nächsten Songs. Eigentlich sollte die gesamte Karte
> weggeworfen werden"

> "dann siehst du das cover des nächsten songs … ist doch scheiße"

> "man sieht nicht dass man gerade zum nächsten song geswipet hat"

The third one is the reason this plan does not simply hide things during the
swipe. Whatever is fixed here must leave the gesture **more** legible, not less.

## The reconciliation — two defects, one report

Two sessions each measured this bug, reached a confident diagnosis, and were
each half right. They were not looking at the same screen.

The card has two display modes and the tap-to-toggle switches between them
(`NowPlayingSheet.kt:159-163`). The symptom is different in each:

| | **F-LATCH** (cover mode) | **F-PLATE** (visualizer mode) |
|---|---|---|
| what is seen | the displaced centre card repaints with the *next* track's cover | a full-strength album cover slides in beside the dark spectrum plate |
| when | at commit, for the length of the spring | the **whole drag**, from the first pixel |
| cause | `controls.next()` and the offset animation change at two different instants; the content swap is not gated on the offset | neighbours are drawn with **no `opacity` argument** while the centre cover is damped by `1f - visualizerOpacity` |
| in `origin/dev` | present | present — `NowPlayingScene.kt:211-228` vs. `:234` |
| does the latch fix it | yes | **no** — the identity being drawn is entirely correct |

The handover's conclusion that PR #771's after-arm failed "because the preview
slot was never latched" is wrong in its mechanism. #771 was measured in cover
mode, where a neighbour preview showing the next cover is what a preview is
*for*. Its residual 265 ms was largely correct behaviour counted as a defect —
which is why no amount of latching would have driven it to zero. The
unfalsifiable criterion, not the code, is what failed.

## What is already built

Neither branch is merged. They touch disjoint files and do not conflict.

| | **A** — `feature/android-swipe-discards-the-whole-card` | **B** — `feature/the-neighbour-plate-obeys-the-visualizer` |
|---|---|---|
| PR | #771, **must not merge as it stands** | none |
| files | `NowPlayingSheet.kt`, `NowPlayingGestures.kt` | `NowPlayingScene.kt` |
| fixes | F-LATCH — `outgoingTrack` latch, exit to `±swipeWidthPx` | F-PLATE — `drawPlayedNeighbourCover` with `opacity = 1f - visualizerOpacity`; plus a Tinder throw (tilt, fade, grow-in) |
| tests | 5 (`NowPlayingSwipeTransitionTest`) | 13 (`NowPlayingSceneVerificationTest`) |
| touches the other's files | no | no |

B already carries the F1 decision the user made from three options: in
visualizer mode the swipe moves between dark plates. It has never been measured
on the device.

## Decisions

### D1 — The invariant is stated per slot and per mode

One sentence per mode, each falsifiable on its own:

- **Cover mode.** For every frame of the gesture, the **leftmost visible card**
  shows the artwork of the track that was playing when the gesture began, until
  its right edge leaves the screen. The card that enters from the right may show
  the next track's artwork from its first frame.

  This wording only holds if the exit actually clears the screen. A's exit target
  is `widthPx = size.width` of the gesture node (`NowPlayingGestures.kt:111`) —
  the same width the scene uses to place the neighbour
  (`NowPlayingScene.kt:213,222`). The card's right edge rests at ~870 of 1080, so
  the invariant needs `size.width > 870`, i.e. the gesture node must span the
  scene and not the card. **Assert it in a test (T5), do not assume it**: an exit
  to only the card's own width would leave the right edge at 208 when
  `snapTo(0f)` fires, breaking D1 on the last frame.
- **Visualizer mode.** For every frame of the drag, **no full-strength album
  artwork appears anywhere in the card band**. Both the centre and the
  neighbours are damped by `1f - visualizerOpacity`.

The old formulation — "a nonzero offset and a track flip never in the same
frame" — is dropped. It cannot distinguish the outgoing card from the preview,
and that is precisely what made the 265 ms unreadable.

### D2 — The preview may show the next cover (user decision)

Asked and answered: the neighbour plate keeps showing the next track's artwork
in cover mode while the committed card flies out. It is the only thing on screen
that says *which way the swipe went*, and the user's third quote asks for more of
that, not less. Rejected: suppressing it until the swap lands (an empty throw);
damping it like the visualizer plate (dims exactly what should be visible).

Consequence: **the target is 0 ms for D1's wording, not for "no next cover
anywhere on screen".** These are different measurements and only the first one
is achievable.

### D3 — B is the base, A's latch is ported onto it (user decision)

B carries F-PLATE plus the throw visuals plus 13 tests and touches none of A's
files. A's latch is three files' worth of mechanical transplant: the
`outgoingTrack` / `displayedTrack` pair, the `onSettle(decision, widthPx)`
signature, and the exit-then-snap sequence. Its 5 tests come with it.

PR #771 is **closed, not merged**, with a comment pointing here. Its commits
survive inside this branch.

### D4 — The snap and the latch release happen in the same frame as the swap

A's exit animates to `±swipeWidthPx`, then `snapTo(0f)`, then
`outgoingTrack = null`. If the new track has not arrived by then, the centre
card repaints the *old* track at rest for a frame — the same class of defect,
mirrored. A's existing test
`aCommittedSwipeThatNeverReceivesANewTrackReturnsToCentre` pins the timeout path;
this plan additionally requires that on the normal path the snap, the release and
the `track.id` change are observable in one frame, never spread over two.

### D5 — The throw and the full-width exit have never run together

B's `cardThrowFraction`, `cardTiltDegrees`, `cardExitOpacity` and
`neighbourEntryScale` were written against a drag that returns to centre. A's
exit drives `horizontalOffsetPx` all the way to `±swipeWidthPx`, an input range
B's helpers have never seen: at full width the outgoing card sits at minimum
opacity and maximum tilt, and the neighbour at full scale and centred — one frame
before the snap. This composition is the integration risk and gets its own tests
(T5), not just a device look.

### D6 — Deliberately not in scope

- **F3, the incoming spectrum ramps from zero over ~0.5 s.** Real, documented in
  the handoff, and independent: it has its own cause, its own fix (prefetch) and
  its own measurement. Bundling it would make the after-arm unfalsifiable for the
  second time. Carried forward as its own note — see "What this leaves open".
- **`the-whole-screen-moves-with-the-swipe.draft.md`**, the redesign where title
  and seek fly with the card. Separate feature, still open. One thing to know
  now: that draft replaces the `horizontalOffsetPx` model with
  `pos = index * screenWidth + dragDelta`. **A latch built on `horizontalOffsetPx`
  does not survive it** — D1's invariant would have to be re-expressed against
  `pos`. Known now rather than discovered twice.
- `onCoverBounds` still reports an axis-aligned rect while the card is tilted
  mid-drag. Belongs to whoever owns tap-to-toggle.
- Gesture physics, thresholds, and the previous/next asymmetry.

## Verification

### One protocol, pinned

The two existing documents record two different gestures. Arms measured with
different gestures do not compare. **This one, and only this one:**

```
adb shell settings get global window_animation_scale     # precondition: 1.0
adb shell screenrecord --bit-rate 20000000 --time-limit 8 /sdcard/arm.mp4
adb shell input swipe 700 770 260 770 250
```

The wide gesture from the findings file (`input swipe 850 1000 120 1000 260`) is
**rejected**: it leaves the card's rest bounds and is caught by the tab pager,
which jumps to the Queue tab.

Card rest bounds on the reference device: **left 208, right ~870, centre 540,
width ~662** of 1080. The earlier `right=785 / width=577` was measurement error
and is superseded. **The left edge is the metric** — right-edge detection is
fooled by album-art content.

Frames: 120 fps, cropped to the card band, every 6th frame kept (~50 ms per
tile).

**The counting rule, because this is where the last after-arm became
unreadable.** Two things break a fixed-crop RMSE against an absolute reference:
the leftmost card moves every frame, so a fixed crop cannot attribute content to
a slot; and B renders the outgoing card **tilted** and faded to
`CARD_EXIT_MIN_OPACITY = 0.55` while the incoming one arrives at
`CARD_ENTRY_MIN_SCALE = 0.9`. A full-strength reference of cover X will not match
a tilted 55%-opacity render of cover X, and B has never been measured on a device
— no existing protocol was built for this.

So the rule is **relative, not absolute**: locate the two card quads per frame,
and for each one decide *which of the two candidate references it is closer to* —
X (outgoing) or Y (incoming). The verdict is an assignment, not a threshold. A
frame violates D1 when the leftmost quad is assigned Y. Fix the reduction before
T7 and state it in the run log; deciding it while looking at frames is how the
retracted false positive happened. Any frame that decides something is re-checked against a
**full-resolution** screenshot — a mis-scaled crop already produced one false
positive that had to be retracted.

### The arms

| arm | mode | asserts | target |
|---|---|---|---|
| **V1** control, before | cover | D1 cover-mode wording | fails on `origin/dev` |
| **V2** control, before | visualizer | D1 visualizer wording | fails on `origin/dev` |
| **V3** after | cover | leftmost card holds the outgoing cover until off-screen | **0 frames violating** |
| **V4** after | visualizer | no full-strength artwork in the card band during the drag | **0 frames violating** |
| **V5a** discriminator | cover | animations off → the offset never leaves 0, so F-LATCH's window is structurally impossible | if it survives, the F-LATCH diagnosis is wrong and the plan stops |
| **V5b** discriminator | visualizer | **mode, not motion**: hold the same drag at rest, visualizer on vs. off. Artwork at full strength with the visualizer on, damped after the fix | F-PLATE is a drag-time defect; `window_animation_scale=0` does nothing to a finger-driven offset and must **not** be used as its discriminator |
| **V6** regression | both | rejected swipe (below threshold) returns to rest, no content change; both directions | unchanged |

Two traps that cost earlier sessions real time:

- The card shows the **visualizer by default**. Tap it to reach cover mode, and
  confirm the mode by screenshot before recording. Half the confusion in this
  bug's history is an unrecorded mode.
- Keep the gesture inside the rest bounds, as pinned above.

Also unverified from the previous session and owed here: **swipe-right to
previous**, and **two rapid swipes in a row**.

### Tests

Device measurement is the verdict, but it is not the gate. The gate is the
Android suite, run whole — 91 suites, 527 tests was the last known-good count.

## Tasks

- **T1** — Create the worktree from
  `feature/the-neighbour-plate-obeys-the-visualizer`. That branch is 5 commits
  ahead of `origin/dev` **and 5 behind** — rebase it first, and drop its
  `0.1.75` version bump (the landing run owns that).
- **T2** — Port A's latch: `outgoingTrack` / `displayedTrack` in
  `NowPlayingSheet.kt`, the `onSettle(decision, widthPx)` signature and the
  bounds check in `NowPlayingGestures.kt`, the exit-then-snap sequence. Bring
  `NowPlayingSwipeTransitionTest`'s 5 tests across unchanged.
- **T3** — Make the snap, the latch release and the `track.id` change observable
  in one frame (D4). Test.
- **T4** — Reconcile the two: B's throw helpers must be correct across the full
  `0 … ±swipeWidthPx` range that A's exit now drives (D5).
- **T5** — Tests for the composition (D5): outgoing card's opacity and tilt at
  full-width exit; neighbour scale at full width; that the exit width exceeds the
  card's right edge (D1); and that the frame after the snap shows the new track at
  rest, undamped and untilted.
- **T6** — Run the whole Android suite.
- **T7** — Device arms V1–V6, in that order. V5 first if anything looks off.

## Risks

- **The reference device locked mid-session last time** and no credential was
  available; guessing a PIN was refused. Confirm device access before starting
  an arm, not during one.
- The debug APK needs a fresh arm64 `libreprise_android_ffi.so` or the app dies
  on launch with `UnsatisfiedLinkError`:
  `scripts/android-build.sh ANDROID_TARGET=aarch64-linux-android ANDROID_ABI=arm64-v8a`.
- `settings put system screen_off_timeout 1800000` was left at 30 min by the
  previous session; the original value is unknown and was not guessed.
- Codex has written `phase: shipped` into plans on its own. That line belongs to
  `land.sh`. Reset it after a run and say so in the prompt.

## What this leaves open

- **F3** — the incoming card's spectrum ramps from near zero over ~0.5 s instead
  of arriving populated. Deferred by D6, not dismissed.
- One unexplained observation from the before-arm recording: the track advanced
  **again on its own** at t≈5.5–6.8 s with no further input. Possibly a duplicate
  synthetic touch from `input swipe`, possibly real. Nobody has looked.

## Parallelität

One worktree, one branch. The showroom session works in `showroom/` and does not
touch `android/`.
