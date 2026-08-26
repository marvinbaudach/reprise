---
slug: queue-landing-flash-follows-the-drop
worktree: /home/marvin/Projects/reprise-queue-visual-queue-landing-flash-follows-the-drop
branch: feature/queue-landing-flash-follows-the-drop
phase: refactored
codex_session:
created: 2026-08-26
---
# Queue: the landing tint belongs to the landing

## The complaint

Dropping a queue row makes it light up teal *a moment after* it has already come
to rest. It reads as a second, unrelated event rather than as the drop being
confirmed.

## What is actually happening, measured

Pixel 10 Pro XL, arm64 debug build of `110641af2c` (Android 0.1.54), one
adjacent swap driven through the drag handle, screen recording at 30 fps. Two
oracles over the same clip: the longest empty run of scanlines in the title
column (the row-hole metric from #704), and the mean `(G+B)/2 − R` over the
destination row's band, which is the teal tint's signature in this dark theme.

| t | what the frames show |
|---|---|
| 2.63 s | finger lifts, settle animation starts |
| **3.77 s** | tint jumps 7.0 → 22.4: **the flash starts here** |
| 4.03 s | tint back to baseline |

The tint lands on the correct row (verified frame by frame: `PRAY FOR PEACE`,
the row that moved) and its duration matches `QUEUE_DRAG_FLASH_MS = 520`.

**How long the tint is late is not measurable from this clip.** The obvious
anchor — the row-hole metric returning to its 94 px baseline — is the wrong
one. That metric was built in #704 to catch a *missing* row, and it reaches
baseline while the dropped row is still several pixels from home, so reading a
"row is at rest" moment off it overstates the delay. The delay the change
actually removes is the edit's round trip, because `move` is dispatched only
after the settle animation completes in both the old and the new code. Measured
paired, arm against arm, that round trip is **≈140 ms** — see the verification
table, which is the number that counts.

## Why it is late

`QueueReorderState.end()` runs the settle animation to completion, *then* sets
`pendingFlashSlot = destination` and calls `move`. The tint is not started
there — it is started by `release()`:

```kotlin
pendingFlashSlot?.let { slot ->
    flashSlot = slot
    flashToken += 1
    pendingFlashSlot = null
}
```

and `release()` is reached either from `onOrderChanged()`, when the reloaded
window comes back, or from the `QUEUE_RELOAD_GRACE_MS = 600` backstop. The tint
is therefore bound to **the round trip of the edit**, not to the landing. It is
the same instant the #704 artifact sat on — both were triggered by the reload
arriving.

## Why it cannot simply be started earlier

**Obstacle 1 — a slot index means different rows at different times.**
`queueRowColor` matches `reorder.flashSlot == slot`, where `slot` is the row's
composed queue position. Until the reload lands, the composed order is still the
old one; the dragged row only appears to sit at `destination` because of its
translation offset. Setting `flashSlot = destination` at settle-end would tint
whichever row occupies that index in the old window — for a one-slot move
upwards, the neighbour that parted.

**Obstacle 2 — the flashing row is disposed mid-flash.** The tint's `Animatable`
lives in a `remember` inside `queueRowColor`, i.e. in the row's own composition.
The queue row key is `queue-$index-$uri`, so a reorder changes the key of every
row whose index moved, and Compose answers a changed key by disposing the row
and composing a new one. A flash started at the landing would be mid-fade when
the reload disposes the row; the replacement composes with a fresh
`Animatable(0f)`, sees `owed == true`, and runs `snapTo(1f)` — **the tint
restarts at full brightness.** Two flashes instead of one: worse than today.

This obstacle is independent of how the row is identified. Keying by track id
does not help; the `remember` is still inside the disposed composition.

Obstacle 2 is the load-bearing claim of this plan and is currently *derived*,
not observed. The verification below proves it with its own arm.

## The change

Three moves, in this order. None of them is useful alone.

### 1. The tint's animation moves into `QueueReorderState`

The state outlives every row, so an animation it owns survives the reload that
disposes the row reading it.

- add a private `Animatable(0f)` and expose its value;
- `queueRowColor` drops its `remember { Animatable(0f) }` and its
  `LaunchedEffect` and reads that value instead;
- `clearFlash`, `flashSlot`, `flashToken` and `pendingFlashSlot` disappear. With
  a single owner there is no owed tint to hand over and no token needed to
  re-trigger a re-used slot.

**A new drop supersedes the previous one.** There is one animation, so a second
drop within the 520 ms restarts it and the earlier row's tint ends at once
rather than fading out. This is intended.

### 2. The tint starts when the row lands

In `end()`, where `pendingFlashSlot = destination` stands today — immediately
after `settlePx.animateTo(...)` returns, before `move` is dispatched:

```kotlin
flashFrom = start
flashTo = destination
scope.launch {
    flash.snapTo(1f)
    flash.animateTo(0f, tween(QUEUE_DRAG_FLASH_MS, easing = QueueDragEasing))
}
```

`release()` loses its flash block entirely. A drop that lands back on its own
slot (`destination == start`) keeps returning early and still does not flash.

The tint fires without knowing whether the edit will be accepted. That is
deliberate: `move` is fire-and-forget and has no return channel, and waiting for
one would restore exactly the coupling this plan removes.

### 3. The moved row is identified through its own one-way latch

The first implementation reused #704's offsets predicate to answer "Which
composed slot is the moved row at right now?" That was wrong. The predicate is
deliberately true whenever the offset hold is idle, so `release()` made the
tint jump back to `start` after the reload even though the tint had already
handed over to `destination`.

The tint therefore owns a separate one-way latch in `QueueReorderState`:

- at settle-end, when the flash starts, its composed slot is **`start`**;
- the first `onOrderChanged()` after that changes the slot to
  **`destination`**, whether or not `awaitingReload` is still set;
- later offset release or order callbacks cannot move it back;
- when the animation finishes, the slot is cleared. A new `begin()` also
  cancels and clears an older tint before starting the next gesture.

The pure selection predicate describes only that latch:

```kotlin
internal fun queueFlashSlot(
    flashing: Boolean, handedOver: Boolean, from: Int, to: Int,
): Int? = if (!flashing) null else if (handedOver) to else from
```

`queueRowColor` checks the latched slot before reading the animated fraction.
Because the animation now lives in the state, the handover from `start` to
`destination` at the reload is invisible: the old row stops reading the value,
the new row starts reading the same still-decaying value. Only that one row
subscribes to the fraction's per-frame changes.

**Why not key by track id.** Simpler, but a queue may hold the same track more
than once — the core tests this explicitly
(`queue_remove_tests.rs:82`, `queue_order_tests.rs:54`) — and an id-keyed tint
would light every occurrence.

**This also fixes a latent defect.** Today, when the core refuses the move, the
order never changes, `onOrderChanged()` never fires, and the grace backstop
reaches `release()`, which tints slot `destination` in a window that is still in
the *old* order: the neighbour, not the moved row. Under the new rule the latch
remains at `start`, the row the user sees, because the composed order never
changes; the 520 ms flash is over before the 600 ms grace expires.

## What is pinned, and what cannot be

Same split as #704, for the same reason: the defect is a *timing* one and
`mainClock` erases it. With the clock paused, `waitForIdle()` drains the very
effects whose ordering is the bug, so a Robolectric test is green with and
without the change. Do not write one and do not claim one is possible.

- **Pinned as a predicate:** `queueFlashSlot(flashing, handedOver, from, to)`
  with `QueueFlashSlotTest`: not flashing → `null`; before handover → `from`;
  after handover → `to`; `from == to` → the same slot either way; once handed
  over, an independently idle-true offsets predicate cannot send it back.
- **Pinned as evidence:** the on-device measurement below.

## Verification

### On device — three arms

Pixel 10 Pro XL, arm64 debug build, all three arms driven by one script so
gesture, scroll state and timing are identical. Each recording is bracketed by a
row-order capture and **counts only if it produced exactly one adjacent swap**: a
synthesized `input swipe` can trip the auto-scroll and carry the row several
slots, which silently invalidates the clip. Scroll the queue to the top first.

Every arm's APK must be confirmed by its packaged bytecode, not by the build
log — a "BUILD SUCCESSFUL in 7s" has packaged a stale APK before, and two
different builds have already come out byte-identical in size. Compare the
`classes*.dex` digests and grep for the changed symbols.

Oracle: the destination row's teal metric per frame.

Measure the moved row's band **and** the neighbour's band. A tint on the wrong
row is invisible to a single-band oracle.

| arm | build | measured |
|---|---|---|
| control | `dev` unchanged | one rise at **3.77 s** on the moved row, clean decay; neighbour flat |
| **negative** | steps 2+3 **without** step 1 | **two rises** on the moved row (3.63 s, then again at 3.73 s back to full) **and the neighbour's tint sticks at full for the rest of the clip** |
| fix | all three steps | **one rise at 3.63 s** on the moved row, continuous decay 23→20→14→12→10; neighbour shows a single handover frame |

All three arms produced exactly one adjacent swap and identical hole curves, so
the arms are comparable frame for frame. The fix moves the tint **140 ms**
earlier — from the reload's arrival to the end of the drop animation. That is
the whole delay there is to remove.

The negative arm was not optional and it paid for itself: it shows the row-local
animation does not merely restart the fade, it also leaves the neighbour tinted
permanently, because that row's `LaunchedEffect` never fires again. Skipping
step 1 would have shipped a worse defect than the reported one.

**Known residue:** at the handover frame the fix draws the tint on the source
slot's position for a single 30 fps sample. It is one frame of a 520 ms fade and
below the threshold this change was asked to fix; recorded rather than hidden.

The row-hole metric from #704 must stay at **0 frames** over 150 px after the
settle in the fix arm — this change must not reintroduce that artifact.

### Suites and gates

`scripts/check-android-suite.sh`, `scripts/check-android-theme.sh`, and
`scripts/check-merge-readiness.sh` with `MERGE_READINESS_BASE_REF=origin/dev`.

## Files

- `android/app/src/main/java/de/reprise/spike/QueueReorder.kt`
- `android/app/src/main/java/de/reprise/spike/LibraryTrackRows.kt`
- `android/app/src/test/java/de/reprise/spike/QueueFlashSlotTest.kt` — new

The list is a starting point, not a fence. If the contract cannot be met inside
it, say so rather than guessing.

## Deliberate non-changes

- `QUEUE_DRAG_FLASH_MS` (520) and `QUEUE_DRAG_FLASH_ALPHA` (0.16) keep their
  values. The complaint is about *when*, not how long or how bright.
- `QUEUE_RELOAD_GRACE_MS`, the offsets hold and `queueOffsetsDescribe` are
  untouched. They solve #704 and nothing here needs them to change.
- The row key stays `queue-$index-$uri`. Making it stable would let the
  animation stay in the row, but row identity reaches LazyColumn, the drag's
  `pin()` and every measurement keyed on it — a larger change than this defect
  warrants.
- The drop haptic stays where it is, at lift-off.
- **Reduced motion stays out of scope.** The queue drag honours
  `ValueAnimator.areAnimatorsEnabled` nowhere — not the lift, the parting, the
  settle, or the tint — although `PlayGestureState` and the Now Playing scene
  do. Wiring only the tint would be inconsistent; wiring all of it is its own
  plan. Recorded here so the finding is not lost.
- No UX rule covers the arrival tint (ACC-8 governs only the alternative to
  direct manipulation), and `scripts/check-motion-tokens.sh` reads only
  `crates/reprise-gnome/src/ui`. Neither needs an edit.

## Parallelität

**No cut.** Both source files change in service of one behaviour and the three
steps are not separable: step 1 without step 2 changes nothing visible, step 2
without step 1 produces a *double* flash — worse than the reported defect — and
the new test depends on the symbol step 3 introduces. One strand, one worktree,
one Codex run.
