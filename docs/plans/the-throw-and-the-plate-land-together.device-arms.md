# Device arms — the throw and the plate land together

> **The branch these arms measured is parked, unmerged, since 2026-09-01.**
> `the-whole-screen-moves-with-the-swipe.md` landed on `origin/dev` and deletes
> the latch this branch built; see the banner on
> `the-throw-and-the-plate-land-together.md`. This log was lifted onto `dev`
> anyway, because the harness it describes is the only thing that can measure
> the redesign, and because one of its arms found a defect that is still
> unfixed and still unmeasured on the new model. What carries over is at the
> bottom: "What the redesign inherits".

Run log for the device measurement the plan names as its verdict. Written as
the run goes, so the reduction is pinned before the frames are counted.

Branch `feature/the-throw-and-the-plate-land-together`, worktree
`/home/marvin/Projects/reprise-the-throw-and-the-plate-land-together`.
Artefacts: `/home/marvin/.cache/reprise-swipe-arms/` (real disk, not tmpfs —
the previous session lost its scratch mid-run).

## Substitutions, and why

**The control is the branch's merge-base `346fb33788`, not `origin/dev`.** The
plan names `origin/dev`, and that was right when it was written. Since then dev
has moved through `crates/reprise-android-ffi/src/playback_session.rs` and
`playback_session/queue_boundary.rs` — the swipe's own neighbour logic; a
`origin/dev` build does not even compile against this branch's FFI
(`Unresolved reference 'previousInQueueOrder'`). A dev-based control would
differ from the fix build by dev's queue work *plus* the ten commits under
test, so "defect reproduces / defect gone" would not attribute to either.
The merge-base differs by exactly the ten commits.

Consequence for landing, not for measuring: dev has advanced into the same
area, so the branch needs a rebase before it lands and the rebase may interact
with the latch.

**Both APKs are debug builds of the same package.** The branch touches no Rust,
so the native `.so` and the UniFFI bindings are built once and shared; only the
Gradle build differs.

| | sha256 (first 16) | tree |
|---|---|---|
| control | `115e49983d957efb` | `346fb33788` |
| fix | `b8c63753c60f55fd` | `1c088a53e1` |

The installed build is identified per arm by `sha256sum` of the on-device APK,
recorded in each arm's `build-sha.txt`. Version codes cannot tell the two
apart (both 75), so the checksum is the marker.

## The device, and what had to be done to it

Pixel 10 Pro XL `59100DLCQ006SB`, 1080×2404, density 390. All three animation
scales read `1.0` before anything was touched.

The app already on the phone (`io.github.marvinbaudach.reprise`, 0.1.74,
debug) was signed with a debug keystore that no longer exists on this machine —
none of the eight `debug.keystore` files found here matches its certificate —
so the measurement APKs could not be installed over it
(`INSTALL_FAILED_UPDATE_INCOMPATIBLE`). With the user's agreement the package
was uninstalled and reinstalled. Nothing is lost:

- `files/` and `shared_prefs/` were pulled with `run-as` before the uninstall
  (`appdata.tar`) and restored the same way afterwards;
- the original APK was pulled first (`user-0.1.74.apk`), so the user's own
  build goes back at the end;
- the music itself is not in app storage. The library is a SAF tree
  (`content://…/tree/primary%3AMusic%2FReprise`) and that grant *is* revoked by
  an uninstall — it has to be re-picked in the system dialog after each
  install, after which the app rescans (750 titles, about a minute).

## The protocol

One gesture, the plan's: `adb shell input swipe 700 770 260 770 250` towards
the next track, mirrored for the previous. The wide gesture from the old
findings file stays rejected. Card rest bounds re-measured on the control
build and unchanged from the plan: **left 208, right 870, top 591, bottom
1253** — a 662 px square.

`run-arm.sh <name> <left|right> <cover|visualizer>` refuses to record unless

1. the Now Playing sheet is open (`now-playing-seek` in the UI dump),
2. the card is **settled** — two captures 0.6 s apart differ by less than 1.5
   grey levels inside the card,
3. the card is in the mode the arm claims. The card shows the **visualizer**
   by default and cover art after a tap; the mode is decided by the fraction
   of near-black pixels in the card square and the `pre.png` is kept for the
   eye. Measured on this device: covers 0.11 to 0.40, visualizer plates 0.59
   (dense bright bars) to 0.91 — the threshold is 0.50. An earlier 0.70 was
   raised out by a plate that was too bright for it.

The first attempt at V1 was thrown away by check 2 and 3 together: it recorded
a card mid-animation, in visualizer mode, for an arm that claimed cover mode.

Capture is `screenrecord --bit-rate 20000000 --time-limit 6`. screenrecord
drops duplicate frames, so a 6 s capture is a few hundred frames, at ~140 fps
through the gesture and sparse outside it. **Every frame is kept** — the plan's
"every 6th frame" assumed a fixed 120 fps and would have thrown the gesture
away.

## The reduction, fixed before any frame was counted

The rule is the plan's: an assignment, not a threshold. Two shapes were tried
and rejected first, both on evidence:

1. **Column segmentation alone** merged two touching cards into one span.
2. **A sliding window with an absolute presence threshold** fired on
   mostly-off-screen windows — on a rest frame holding a single card it
   reported a second card at `left = -375` and called the frame a violation.

What survives: cards are located geometrically (per-column vertical structure;
a cover carries it, the background gradient does not), a span too wide for one
card is split at its quietest interior column, and each located card is then
assigned to whichever reference it is *closer* to by ZNCC — invariant to any
affine change of intensity, which is what the 0.55 exit opacity and the 0.9
entry scale amount to. A card clipped by the screen edge is compared only
against the part of the reference it can still show.

**Both references come from the arm's own rest frames**, so nothing is ever
compared across builds, and `arm-report.py` aborts if the two covers are not
separable (`ZNCC(X,Y) > 0.5`) rather than producing frame counts that are coin
flips.

`validate.py` renders the two references at the tilts, opacities and scales the
app actually uses and requires the reduction to place all of them correctly
before an arm is counted — including the two shapes that decide the verdict:
the outgoing card leftmost (good) and the next cover leftmost (the defect),
both with the cards touching and with the leftmost card nearly off screen.
15 of 15 on the V1 pair.

Rest is not a constant: the structure threshold trims a dark cover's own
border, so each cover has its own resting left edge. Both are taken from the
arm's own rest frames and a frame counts as part of the gesture when the
leftmost card sits at neither.

## Arms

### The visualizer criterion, and why correlation could not carry it

Matching covers by correlation works in cover mode and fails in visualizer
mode, and the failure is instructive. The incoming neighbour never gets more
than about a third of its width on screen during this gesture, and a narrow
strip of a dark cover's edge is a smooth gradient: it correlates at **+0.96**
with the plain background gradient — with per-column normalisation as well as
without — and its contrast ratio against the reference is 1.1. A frame whose
full-resolution crop, brightened five times, holds no artwork whatsoever was
reported as full-strength artwork by that reduction. It was caught by the
plan's own rule: any frame that decides something is re-checked against a
full-resolution screenshot.

So the visualizer arms are reduced on brightness, which is what D1's wording
actually says. The neighbour arrives from the side the gesture points at:

| | |
|---|---|
| region | the card band, columns outside the resting card box on the entry side (left swipe: x ≥ 871) |
| metric | fraction of pixels at luminance ≥ 90 |
| violation | fraction ≥ 0.015 |

Fixed before the fix arm was counted, and not marginal: a resting full-strength
cover measures 0.27–0.38 by this metric, while the damped plate and the
background gradient never exceeded 0.0003 across a whole 202-frame arm.

## Results

| arm | build | mode | frames | verdict |
|---|---|---|---|---|
| **V1** control | `115e4998` | cover | 663 (34 in the gesture) | **5 violating frames** — F-LATCH reproduces |
| **V2** control | `115e4998` | visualizer | 202 | **6 frames of full-strength artwork**, 217 ms — F-PLATE reproduces |
| **V4** after | `b8c63753` | visualizer | 230 | **0 violating frames**; loudest frame 0.0000 |

### V1 — the control reproduces F-LATCH

"29" (Gone Cold) → "2nd Sucks" (A Day to Remember), references separable at
ZNCC −0.009. The gesture runs 1.834–2.231 s, 34 frames. Through
f0214–f0228 the leftmost card is the outgoing cover, sliding off left, and the
next cover enters from the right — allowed, that is D2. Then, in one frame:

```
f0228 t=2.157  [70-653]X(+0.62/+0.05)   [798-1040]Y(+0.25/+0.57)
f0229 t=2.190  [113-776]Y(+0.01/+0.86)  [788-1012]Y(+0.06/+0.56)
```

The outgoing card does not leave the screen. It is *replaced in place*: a card
of the same size, with its right edge at 776 and well on screen, repaints with
the next cover and springs from 113 back to rest at 209. Five frames, about
41 ms. This is F-LATCH exactly as the plan describes it, and it violates D1's
cover wording under either reading of "until its right edge leaves the screen"
— the leftmost card's right edge is on screen, and the outgoing card's right
edge never left it.

### V2 — the control reproduces F-PLATE

"3 Axle" (King Conquer) → "4 Poisons 3 Words" (Emmure). Six consecutive frames,
t = 2.063–2.280 s (**217 ms**), carry full-strength artwork in the entry region
beside the dark plate, rising to a bright fraction of 0.182 — two thirds of
what a resting cover measures. Every other frame of the arm reads 0.0000.

### V4 — the fix removes F-PLATE

Same gesture, same mode, fix build: **0 of 230 frames**. Not one pixel in the
entry region reaches luminance 90 anywhere in the capture, against six frames
peaking at 0.182 on the control.

A zero needs a witness, or it cannot be told apart from a capture that missed
the gesture. V4 has one: tracked frame by frame, the plate's own left edge
leaves its resting 208 at t = 1.973 s, travels as far as 911, and is back by
t = 2.589 s. The drag is in the capture; the artwork is not.

The two visualizer arms used different track pairs, and the direction of that
difference is the safe one: the incoming cover measures 0.272 at rest in V2
(control) and **0.538** in V4 (fix). The brighter cover is on the fix side, so
a surviving F-PLATE would have registered harder there, not softer.

### V3 — the fix removes F-LATCH, and the shorthand needed D1's own clause

Same pair as V1, references separable at ZNCC −0.009. The gesture runs
1.785–2.201 s, 24 frames. The outgoing card genuinely leaves:

```
f0161 t=1.785  [190-773]X   f0170 t=1.902  [0-533]X
f0176 t=1.977  [0-402]X     f0181 t=2.127  [0-301]X
f0182 t=2.144  [296-954]Y   f0184 t=2.201  [240-901]Y
```

Its right edge travels 773 → 301 and off the screen, and only then does the
incoming card appear — at 296, **right** of the resting edge, still travelling
left to rest.

Counted by the plan's shorthand ("the leftmost quad is assigned Y") that is
3 frames. Counted by D1's actual sentence, which carries the clause *until its
right edge leaves the screen*, it is **0**. Both numbers are reported, and the
clause is expressed without judgement: the card D1 constrains is the one in the
outgoing slot, and that slot is on the far side of rest — a card arriving from
the other side is never there.

The same two counts on the control are **5 and 5**: there the leftmost card
showing the next cover sits at x0 = 113, 173, 181, 188, 196, all *left* of the
resting 209 and springing back towards it. That is the displaced centre card
repainting, not an arrival.

Verified against full-resolution frames: V1 f0229 shows one card, displaced
left, wearing the next cover; V3 f0180 shows the outgoing card leaving with its
own cover while the next one enters from the right; V3 f0182 shows the incoming
card still right of rest.

| arm | build | leftmost==Y | D1 as worded |
|---|---|---|---|
| **V1** control | `115e4998` | 5 | **5** |
| **V3** after | `b8c63753` | 3 | **0** |

### V6 — a rejected swipe, both directions (fix build)

A drag of the same speed over an eighth of the distance, so the only difference
from the committing gesture is the distance.

| arm | travel | returns to rest | track | worst match to the starting cover |
|---|---|---|---|---|
| V6a left | 224 → 161 | yes (213) | unchanged | ZNCC +0.642 |
| V6b right | 224 → 288 | yes (235) | unchanged | ZNCC +0.621 |

Neither ever stops being the cover it started as.

### V8 — two rapid swipes (fix build)

Two committing gestures back to back with no pause: both landed, "29" →
"2nd Sucks" → "3 Axle", and the scene settled to a single card at rest. The
landed track has no artwork of its own, so a cover assignment is not meaningful
for the end state; the geometric check is what stands.

### V5 — what was and was not run

**V5b** is the mode discriminator: the same drag, visualizer on versus off.
V2 and V4 *are* that comparison, on the two builds, and they are reported
above. As the plan insists, `window_animation_scale=0` was never used for it —
it does nothing to a finger-driven offset.

**V5a** was not run as a separate arm. Its question — whether F-LATCH's window
is the exit spring — is answered directly by V1's own frame trace: the
violating card appears at x0 = 113 and walks back 173, 181, 188, 196 towards
the resting 209 over five frames. That is the spring, observed rather than
inferred. A separate animations-off arm would have added a second reinstall
cycle for a fact already on the record.

### The previous direction, and a threshold that is not a bug

Swipe-right first appeared not to commit at all: the card travelled its full
distance, the previous neighbour was drawn at the left edge, and the track did
not change. The on-screen **Previous** button behaved the same way. It is the
ordinary media rule — past a few seconds into a track, "previous" restarts the
current track instead of stepping back; two taps in a row do step back. With
the track paused near 0:00 the gesture commits normally. Worth knowing before
someone else spends an hour on it.

## A regression the arms found: the previous direction settles with the wrong cover

With the track paused near 0:00 so the gesture commits, the same measurement
was run in the **previous** direction on both builds. In the *next* direction
each build flips the centre cover exactly once and keeps it. In the *previous*
direction they do not agree:

| | centre card at rest after the commit |
|---|---|
| control `115e4998` | settles on the incoming cover at **t = 2.276 s**, immediately after the gesture |
| fix `b8c63753` | settles at **t = 2.243 s** wearing the **outgoing** cover, and only flips to the incoming one at **t = 3.483 s** |

That is **1.24 s during which the card sits still, at its resting position,
showing the cover of the track the swipe left behind**. The title is not along
for the ride, and that was checked rather than assumed — the title strip was
cropped out of the frames themselves, not read from a UI dump seconds later:

| frame | t | title | cover |
|---|---|---|---|
| f0130 | 2.00 s | 2nd Sucks — A Day to Remember | outgoing |
| f0136 | 2.07 s | 2nd Sucks — A Day to Remember | outgoing |
| f0140 | 2.30 s | **29 — Gone Cold** | still outgoing |
| f0231 | 3.48 s | 29 — Gone Cold | incoming, at last |

So the title flips with the commit and the artwork trails it by **1.18 s**.
The scene is not late; the cover is. That is what makes A's timeout path the
first place to look rather than the general track-update path.

This is the mirror of the defect the branch fixes, and it is the failure mode
D4 names in as many words: *"If the new track has not arrived by then, the
centre card repaints the old track at rest — the same class of defect,
mirrored."* D4 required the snap, the latch release and the `track.id` change
to be observable in one frame. In the previous direction, on the fix build,
they are more than a second apart. The duration suggests the release is waiting
on something rather than racing it — A's timeout path is the first place to
look. It is not the only place: the previous neighbour's artwork is never
prefetched either, and one arm cannot tell the two apart. Both are set out
under "What the redesign inherits"; neither is settled.

One cell of the table was not measured, and it decides how the repair should be
framed: the fix build in the **next** direction, paused near 0:00. V3 was
measured while playing. If a paused next-swipe also trails by about a second,
the axis is the commit and data-arrival path, not the direction, and a
regression test written against "previous" would pin the wrong thing. That arm
needs another reinstall cycle, so it is named here rather than guessed at.

**It was a blocker for landing, and it outlived the branch.** The branch's own
subject is a card wearing the wrong cover; it must not introduce one in the
other direction. Nothing in the 542-test suite catches it, and one arm each way
is thin evidence for a timing defect. Since the branch is parked, the defect is
not repaired here — it is handed to the redesign as a required arm, together
with a second candidate cause that this run's data cannot separate from the
timeout. See "What the redesign inherits".

## Verdict

The plan's target — 0 violating frames for D1's wording — is met in both modes
for the direction the plan measures:

- **cover mode**: control 5 violating frames, fix **0**.
- **visualizer mode**: control 6 frames of full-strength artwork over 217 ms,
  fix **0 of 230**, with not a single pixel reaching the threshold.
- rejected swipes return to rest in both directions with the cover unchanged,
  and two rapid swipes both commit.

Against that stands the previous-direction regression above, which is not in
the plan's arms and which the plan's own D4 predicts. The measurement says the
branch does what it set out to do and breaks something adjacent while doing it.

## Not done

- The branch is **not pushed** and there is **no PR**. It is parked at
  `d349a8917d` in
  `/home/marvin/Projects/reprise-the-throw-and-the-plate-land-together`; the
  ten commits stay reachable from `feature/the-throw-and-the-plate-land-together`
  until someone deletes the branch on purpose.
- **PR #771 (`feature/android-swipe-discards-the-whole-card`) is still open.**
  Both this plan and the redesign say independently that it closes unmerged,
  with a comment pointing at the surviving documents. Not done here.
- The previous-direction regression is **not repaired and not diagnosed to a
  single cause**. It is handed to the redesign as a required arm — see below.
- The unmeasured cell (fix build, **next** direction, paused near 0:00) was
  never run, and is handed over with it.
- The user's own build was restored: 0.1.74, `24fd7d0c`, its `files/` and
  `shared_prefs/` put back from the backup taken before the first uninstall,
  and the SAF folder re-picked. Animation scales were never changed; they read
  1.0 throughout.

The two open questions this section used to carry are now answered, and not in
this branch's favour: the rebase onto `dev` is moot because the branch is not
landing, and the order of the two landings was decided by `dev` moving first.

## What the redesign inherits

`feature/the-whole-screen-moves-with-the-swipe-b` replaces `horizontalOffsetPx`
and the latch with `pos = index * screenWidth + dragDelta`. That changes what
the arms *assert*; it changes almost nothing about how they *measure*. The split
is worth writing down once, because the expensive part of this run was building
the reduction, not swiping the phone.

### Transfers unchanged

The whole harness below the verdict:

- the gesture (`adb shell input swipe 700 770 260 770 250`, mirrored) and the
  rejection of the wider one that the tab pager eats;
- the card rest bounds — left 208, right 870, top 591, bottom 1253 on the
  reference device;
- `run-arm.sh`'s three refusals: sheet open, card settled (two captures 0.6 s
  apart within 1.5 grey levels), and the mode the arm claims, decided by the
  near-black fraction of the card square with the threshold at 0.50 (covers
  0.11–0.40, visualizer plates 0.59–0.91);
- `screenrecord --bit-rate 20000000 --time-limit 6` with **every frame kept**;
- the ZNCC assignment reduction, both references taken from the arm's *own*
  rest frames, the abort when the two covers are not separable at
  `ZNCC(X,Y) > 0.5`, and the split of a too-wide span at its quietest interior
  column;
- `validate.py`, which renders the references at the app's own tilts, opacities
  and scales and refuses to count an arm until the reduction places all of them
  correctly;
- the visualizer brightness criterion — entry-side columns outside the resting
  card box, fraction of pixels at luminance ≥ 90, violation at ≥ 0.015 — and
  its calibration: a resting full-strength cover reads 0.27–0.38, the damped
  plate and the background never exceeded 0.0003 across 202 frames.

The two rejected reductions stay rejected for the same reasons: column
segmentation alone merges touching cards, and an absolute presence threshold
fires on mostly-off-screen windows. And correlation still cannot carry the
visualizer arms — a narrow strip of a dark cover's edge correlates at +0.96
with the plain background gradient.

### Must be re-expressed

D1's cover-mode sentence is written against a model with one moving card and
one neighbour. Under `pos` there is a row of panels and, in the redesign's own
words, "a displaced neighbour showing the next track is no longer a defect but
the point". So:

- **"the leftmost visible card"** is no longer a slot. The clause that decided
  V3 — *until its right edge leaves the screen*, i.e. the card D1 constrains is
  the one in the outgoing slot — has no counterpart when every panel is at
  `index * screenWidth`. The invariant has to be restated as a relation between
  a panel's `pos` and the artwork it carries, and stated **before** the arm
  runs, not while looking at frames. That rule is what kept this run honest.
- **The visualizer criterion does NOT survive, and must not be run as a gate.**
  It asked whether full-strength artwork appears in the entry region at all.
  The redesign answers "yes, on purpose": its plan specifies the neighbour in
  visualizer mode as showing its own album cover, blurred and desaturated,
  fading out as the bars rise — `neighbourCoverOpacity = 1 - near^1.6` — and
  records that as the variant the user approved (`scaffold: 'cover'`), against
  its own earlier draft of damped bars with no cover. Strand -b implements the
  formula as written: `coverOpacity = 1f - visualizerOpacity * near.pow(1.6f)`
  (`NowPlayingScene.kt:333-337`), which is the spec at `visualizerOpacity = 1`.
  Cover mode agrees too — the spec's 5 px blur and 0.4 saturation are
  `blurPx = (1f - near) * 5f` and `saturation = 0.4f + near * 0.6f`.

  Run the 0.015 brightness gate against -b and it fails, correctly measuring a
  thing that is now intended. The region, the metric and the calibration stay
  useful as *instrumentation*; the threshold is no longer a verdict. What
  replaces it is a curve check: does measured cover strength against distance
  follow `1 - near^1.6`, rather than jumping or flashing at the centre?
- **V5b stays the mode discriminator**, and `window_animation_scale=0` still
  must not be used for it — it does nothing to a finger-driven offset.

### The arm that is owed, and is not optional

**The previous direction settles with the wrong cover.** Measured on this
branch, with the track paused near 0:00 so the gesture commits: the title flips
at t = 2.30 s and the artwork follows at t = 3.48 s — **1.18 s of a card
sitting still, at rest, wearing the cover of the track the swipe left behind.**
The control build flips both at 2.276 s. The title was cropped out of the
frames themselves, not read from a UI dump afterwards.

Two candidate causes, and **one arm cannot separate them**:

1. **The previous neighbour's artwork was never prefetched.**
   `rememberPlayGestureNeighbours` calls
   `loadUpcomingTracks(LibraryWindowRange(0, 2))` and there is no
   `loadPreviousTracks()` at all — the previous neighbour is only ever carried
   as `rememberedTrack` state. The cover therefore has to be loaded on demand
   when the swipe commits backwards.
2. **The latch release ran its full timeout.** The release is
   `withTimeoutOrNull(TRACK_CHANGE_WAIT_MS) { snapshotFlow { latestTrack.id }.first { it != latchedTrack.id } }`
   with `TRACK_CHANGE_WAIT_MS = 1_000L`.

A measured 1.18 s against a 1 000 ms constant is not a coincidence anyone
should assume away, and it is not a distinction this run's data can make.
**Do not carry either cause forward as settled.**

Both have moved since. `#780` ("The queue window reaches both sides of the
cursor") widened the prefetch to `LibraryWindowRange(-3, 5)`, added
`TrackAnalysisLoader` prefetch/retain, and routed `seekToPrevious()` to a new
history-independent `previousInQueueOrder()`. Cause 1 is therefore plausibly
already gone; cause 2 lives in code the redesign deletes. Neither is verified,
and the redesign is a different rendering model, so:

> The previous direction, paused near 0:00, on both builds, is a **required
> arm** on `feature/the-whole-screen-moves-with-the-swipe-b`. It is not a
> repair owed to this branch.

One cell was never measured here and is still owed, because it decides the
axis: **the next direction, paused near 0:00**. V3 was measured while playing.
If a paused next-swipe trails by about a second too, the axis is the commit and
data-arrival path, not the direction — and a regression test written against
"previous" would pin the wrong thing.

Nothing in the 542-test Android suite catches any of this. The property is
cover-at-rest versus `track.id` over roughly a second; no test compares those
across time.

### Two things that will cost an hour if they are not read first

- **"Previous" past a few seconds into a track restarts it** instead of
  stepping back — the ordinary media rule, and the on-screen Previous button
  does the same. Swipe-right looks like it fails to commit. Pause near 0:00 to
  measure it. `#780`'s `previousInQueueOrder()` is meant to end this for the
  swipe; the swipe still called plain `controls.previous()` on this branch.
- **Both swipe branches predate `#783`**, which moved all 196 Android sources
  from `de.reprise.spike` to `io.github.marvinbaudach.reprise`. Any of them
  needs that rebase before it builds against `dev`.

The device notes stand as written above: the installed debug build cannot be
updated over (no matching keystore), so an arm costs an uninstall, a
`run-as` backup and restore of `files/` and `shared_prefs/`, a SAF re-pick and
a ~750-title rescan. `screen_off_timeout` was left at 30 minutes by an earlier
session and its original value is unknown. Animation scales read 1.0 throughout
and were never changed.
