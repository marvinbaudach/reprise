# Device arms — the whole screen moves with the swipe

> **Corrected 2026-09-01, same day. Do not read the verdict below as "the
> implementation is good."** The user looked at the running app and reported
> the animation as still completely broken. This document's checks were too
> narrow to see that: they test where layers are and whether a cover appears,
> at sampled instants, and never test motion over time. A correct ratio on the
> four frames that exist says nothing about the frames that are missing.
>
> What survives: the retirement of the old brightness gate, the harness
> geometry, and the observation that the previous-direction regression does not
> reproduce *in the sense measured*. What does not survive: any implication
> that the animation runs correctly.
>
> `the-swipe-animation-is-still-broken.HANDOVER.md` carries the evidence
> already in these captures that points the other way — a cover that switches
> in a single frame with no tween, gesture frame gaps of up to 87 ms, and a
> screen that did not settle — and says what to measure next.

First device measurement of `feature/the-whole-screen-moves-with-the-swipe-b`
(f402c2756e). The question asked was the user's: does the implementation
animate the way this plan says it does?

Criterion fixed before the device was touched, in
`~/.cache/reprise-swipe-arms/redesign/CRITERION.md`. Four claims, each taken
from this plan's own sentences, each falsifiable alone. A claim the capture
cannot decide is reported as undecided, never as a pass.

Device: Pixel 10 Pro XL `59100DLCQ006SB`, 1080x2404, animation scales 1.0.
Build under test carries 32 redesign symbols in `classes5.dex` and a fresh
arm64 `libreprise_android_ffi.so`.

## Verdict

| claim | what the plan says | result |
|---|---|---|
| **C4** layer translations | cover factor 1.0, title 1.282, waveform does not travel | **holds on one arm** — see the caveat |
| **C1** visualizer neighbour | shows its own cover, blurred and desaturated | **holds** |
| **C3** no neutral scaffold plate | `NowPlayingNeighbourScaffold` not carried over | **holds** |
| **C2** no jump, no flash at centre | continuous handover to the live visualizer | **undecided** — see below |

## C4 — the layer ratio matches, on one arm

Arm R1, cover mode, `input swipe 700 770 260 770 250`, 375 frames. Per frame
and per band, the horizontal shift that best aligns the frame with the resting
frame, sum-of-absolute-differences over overlapping columns only, 1 px steps.

Only the gesture window carries signal: once the track commits, aligning
against a rest frame of the *old* track is meaningless and the estimator
saturates. Four frames are clean.

| frame | t | cover | title | title/cover |
|---|---|---|---|---|
| f0024 | 1.983 | 26 | 34 | 1.308 |
| f0025 | 1.993 | 60 | 77 | **1.283** |
| f0026 | 2.032 | 93 | 120 | 1.290 |
| f0027 | 2.065 | 287 | 366 | 1.275 |

Mean **1.289** against the specified **1.282** (`= 500 / 390`).

**How much this actually constrains.** Two of those four rows carry the result.
At f0024 the cover has travelled 26 px, where a 1 px estimator error moves the
ratio by about 0.04, so 1.308 is inside the noise; f0027's 287 px is the frame
where the title content is about to swap. The rows that genuinely constrain
1.282 are f0025 (60 -> 77) and f0026 (93 -> 120). Both come from **one gesture
on one arm**. The parked log's own standard — one arm each way is thin evidence
— applies here as much as it did there: this is a match, not a measurement of
the ratio. A second R-arm would make it one.

The waveform band reads **0, 0, 0** across the three clean frames: it does not
travel, which is the claim. Frames f0027 onward show -7, -6, -5 px, which is
the size the plan's `-off * 70` predicts, but those are the frames where the
track content is swapping and the title estimate has already gone to nonsense
(-178, -172, -165). The waveform band does not carry the title text, so it
degrades later than the title does — but not demonstrably later, so the
counter-offset is reported as consistent with the spec and **not** as evidence
for it.

So the three layers move at three different rates, and on this arm the rates
are the ones the plan names. That is the claim the redesign exists for.

## C1 and C3 — the neighbour carries its own artwork

Arm V2, visualizer mode, "29" (Gone Cold) -> "2nd Sucks" (A Day to Remember),
461 frames. Both tracks have real cover art; that matters, see the false start
below.

Measured in the entry region (columns outside the resting card box on the side
the neighbour enters from), the fraction of pixels at luminance >= 90 rises
0.0000 -> 0.0297 -> **0.1994** -> 0.1952 -> 0.0000 across the crossfade.

Frame f0166 at t = 2.151, checked at full resolution as the counting rule
requires, shows the mechanism directly:

- the outgoing panel keeps **its own** cover ("Gone Cold") with the live bars
  drawn over it, sliding out;
- the incoming panel shows **its own** cover ("2nd Sucks"), hazed and
  desaturated, with no bars yet;
- there is no neutral plate anywhere in the frame.

That is C1 and C3 as written. It is also, visually, the thing the original bug
report complained about — "man sieht das Cover des nächsten Songs" — which is
why the old gate had to be retired before this arm ran rather than after. Under
`the-throw-and-the-plate-land-together.device-arms.md`'s 0.015 threshold this
frame scores 0.199 and reads as a gross violation. It is the approved design.

## C2 — undecided, and the reason is the instrument

The entry region is a fixed strip, not a slot. Brightness there falls from
0.1952 to 0.0000 in one frame, but that is the strip emptying as the panel
travels past it, not a visual discontinuity. A fixed-region measure cannot
answer a question about one panel's own continuity.

The capture cannot answer it either way: screenrecord drops duplicate frames,
and the crossfade occupies **3 frames over 165 ms** (~55 ms apart). A flash
shorter than that is invisible to this instrument. Answering C2 needs a
panel-relative measure — track the incoming panel, sample a fixed sub-rectangle
*of the panel*, and test that curve against `1 - near^1.6` — at a frame rate
that resolves it. Not run.

## A false start worth keeping

Arm V1 ran the same gesture into "02 Lifted", which has **no cover art**. It
reported 0 of 527 frames above threshold, with the gesture provably in the
capture (the plate travels 26 -> 500 px at t = 2.015..2.151). The entry region
peaked at luminance 67, mean 41 — content present, nothing bright.

A zero from a coverless neighbour and a zero from a neighbour that never
renders look identical in that reduction. Pick a track pair where **both**
covers exist before running a visualizer arm.

## Harness deviation, recorded

`run-arm.sh`'s settle precondition locates the card's left edge by vertical
structure. On the redesign it reports edges of 53 and 139 px on a screen whose
measured pixel drift is 0.02 grey levels: the detector locks onto the wide
radial fog the redesign draws around the cover, not onto the card.

`run-arm-redesign.sh` asks the settle question of the pixels instead (drift
over the card box <= 1.5 grey levels) and drops the resting-left-edge
assertion. Sheet-open and mode checks unchanged; the verdict reduction
untouched.

This contradicts the parked log's claim that the settle gate transfers
unchanged. It does not. The geometry does — the card rest bounds are still
left 208, right 870, top 591, bottom 1253.

## Not measured

- **C2**, above.
- The **previous direction**, which the parked branch's arms found settling
  with the wrong cover for 1.18 s. Still the required arm named in
  `the-throw-and-the-plate-land-together.device-arms.md`, still not run against
  this build.
- Rejected swipes and two rapid swipes in a row, on this model.
