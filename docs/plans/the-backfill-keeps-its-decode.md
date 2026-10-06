---
slug: the-backfill-keeps-its-decode
worktree: /home/marvin/Projects/reprise-the-backfill-keeps-its-decode
branch: feature/the-backfill-keeps-its-decode
phase: refactored
codex_session:
created: 2026-10-06
---
# The backfill keeps its decode through a playback blip (#1129)

## Problem

On a loaded device the Android track-analysis backfill finishes no track. It is cancelled and
restarted on every playback-state flicker, and each cancel throws away the decode in progress.

The 2026-10-05 emulator run for #1089 shows `Track analysis backfill: 0/12 (0 failed)` 20 times in
under five minutes (104 times over the whole run). Each line has a new thread id and follows an
`onSessionPlaybackStateChanged` burst within about 0.3 s. Under load the player drops into
`BUFFERING` and back. Evidence: `~/.local/share/reprise-device-run-20261005-1089/run-logcat.log`.

## Goal

A brief departure from `PLAYING` no longer discards the backfill's current decode. A test
reproduces the restart before the fix.

Out of scope:
- The Rust backfill worker's behaviour: start, cancel, item order and progress counting stay as
  they are. The fix sits entirely in the Kotlin decision of when to cancel.
- The foreground decode and #1149's supersede.
- A thermal or charging policy.

## Facts the design rests on (origin/dev `d68a21e2a0`)

All Kotlin paths are below `android/app/src/main/java/io/github/marvinbaudach/reprise/`.

**The one decision point.** `ReprisePlaybackService.handleTrackAnalysis(snapshot)` is called from
`coreListener.onPlaybackChanged` on the main thread (`ReprisePlaybackService.kt:~175-185`). At
`:435-441` it does:

```kotlin
val shouldRun = analysisBackfillShouldRun(
    playing = snapshot.state == AndroidPlaybackState.PLAYING,
    powerSaveMode = isPowerSaveModeOn(),
)
if (shouldRun != analysisBackfillRunning) {
    analysisBackfillRunning = shouldRun
    if (shouldRun) startAnalysisBackfill() else cancelAnalysisBackfill()
}
```

- `TrackAnalysisBackfillPolicy.kt:14` defines `analysisBackfillShouldRun = playing && !powerSaveMode`.
- `startAnalysisBackfill` and `cancelAnalysisBackfill` (`:517-527`) are `internal open`. They
  launch onto `analysisBackfillScope` (`SupervisorJob() + Dispatchers.IO.limitedParallelism(1)`),
  so the start and cancel calls run in order.
- `onDestroy` (`:374-382`) cancels synchronously.

**How a flicker arrives.**
- The Media3 listener in `Media3PlaybackPort.kt:160-177` calls `emitState()`. That maps `isPlaying` to `PLAYING`,
  `STATE_BUFFERING && playWhenReady` to `BUFFERING`, `IDLE`/`ENDED` to `STOPPED`, and everything else to `PAUSED`.
- It dedups only on an unchanged state, so a blip is PLAYING → BUFFERING → PLAYING. Each step reaches
  `handleTrackAnalysis`: one cancel, then one start.

**The Rust worker** (`crates/reprise-android-ffi/src/track_analysis/backfill.rs`):
- A start while a run is alive is a no-op.
- A start when idle resets the progress counters, which is why `done` restarts at 0.
- A cancel sets the flag, cancels the in-flight sink and joins the worker.
- A cancelled decode stores nothing (`compute.rs` `decode_one`).
- `pending_render_data_tracks` is re-queried every item in id order, so a restart re-decodes the same
  first pending track from the beginning.

**Why the backfill stops at all.** Decision 5 of `docs/plans/the-phone-analyses-its-own-music.md`
says the backfill runs only while playback is `Playing` and is skipped under battery saver. The
reasons are CPU and battery. `docs/ux-rules.md` has no rule on when the Android backfill runs.
NAV-15d and NAV-15e only say the backfill carries on through a supersede.

**Test seams.**
- `ReprisePlaybackServiceAnalysisTest.kt` is Robolectric and drives the main Looper with
  `shadowOf(Looper.getMainLooper())`. Its service subclasses override start and cancel with `Unit`.
  No fake records those calls today.
- `TrackAnalysisBackfillPolicyTest.kt` holds the four truth-table tests.
- The service has no `postDelayed` of its own. `Handler(Looper.getMainLooper())` is already used,
  and `Media3PlaybackPort` and `SleepTimer` both use `postDelayed`.

## Decisions (as drafted — see "Grill decisions" at the end, which wins where they differ)

**D1: A grace period before the cancel.** When `shouldRun` turns false because playback left
`PLAYING`, the cancel is not issued at once.
- It is posted on the main Looper after `ANALYSIS_BACKFILL_STOP_GRACE_MS = 10_000`.
- If `shouldRun` turns true again before then, the pending cancel is removed and nothing reaches Rust:
  no cancel, no start, and the running decode continues.
- `analysisBackfillRunning` keeps meaning "a run has been asked for and not cancelled".
- One mechanism covers `BUFFERING`, a short pause and any state flicker during a track change.

**D2: Two cancels stay immediate.**
- Battery saver turning on cancels at once. The user asked for less work.
- `onDestroy` cancels at once and also drops a pending delayed cancel.

**D3: The policy function stays a pure truth table.** The grace is a timing decision in the
service, not in `analysisBackfillShouldRun`. A small, separately testable helper holds it: a
`BackfillStopDebouncer`, or a pair of private functions over the Handler, whichever reads better
next to `SleepTimerController`.

**D4: No Rust change.**
- The worker's progress line after a cancel stays, because it is the evidence the post-merge
  check counts.
- The run-local `done` counter stays. A new run counting from 0 is correct; only the needless runs
  were wrong.

**D5: UX rule.** No new rule is proposed. The backfill is not user-facing, and the
rulebook has none for its timing. The plan records the contract, and the tests carry it.

## Tasks (test first: write the failing test, see it fail, implement, see it pass)

**T1: A recording seam and the reproduction.** In `ReprisePlaybackServiceAnalysisTest.kt`, or a
sibling `ReprisePlaybackServiceBackfillTest.kt` if the file is near the cap, add a service subclass
that records `start`/`cancel` calls in order.

Tests:
- `a_buffering_blip_keeps_the_backfill_running`: PLAYING, BUFFERING, PLAYING within 1 s of Looper
  time. The recording shows exactly one start and no cancel. Before the fix it shows start, cancel,
  start.
- `a_short_pause_keeps_the_backfill_running`: PLAYING, then PAUSED, then PLAYING after 5 s. One
  start, no cancel.
- `a_pause_longer_than_the_grace_cancels_the_backfill`: PLAYING, then PAUSED, then the Looper idles
  past 10 s. Start, then cancel.
- `battery_saver_cancels_the_backfill_at_once`: with power save on, the cancel is recorded without
  advancing the Looper.
- `destroying_the_service_drops_a_pending_cancel`: after the pause and `onDestroy`, exactly one cancel
  is recorded, the synchronous one, and none fires later.
- `a_restart_after_the_grace_starts_a_new_run`: pause, idle past the grace, then play again. Start,
  cancel, start.

**T2: The grace in the service.** `ReprisePlaybackService.kt`, plus a small sibling file if the
helper is extracted. Add the constant `ANALYSIS_BACKFILL_STOP_GRACE_MS`. KDoc on the constant
names the reason: #1129, BUFFERING blips under load.

**T3: Policy KDoc.** `TrackAnalysisBackfillPolicy.kt`: one sentence that a stop is applied after the
service's grace period, so the policy reads correctly on its own. No logic change.

## Verification (worker, in the worktree)

This plan touches ONLY `android/` and `docs/plans/`. The Rust gates in AGENTS.md do not apply, so
no cargo command is run by hand. AGENTS.md's "all gates before every commit" is overridden for this
run: the orchestrator runs the full gates after the code phase.

Run the Android suite with the worktree-local env prefix, one Gradle invocation at a time,
under `heavy-run heavy --`:

```
ANDROID_HOME=/home/marvin/.local/share/android-sdk ANDROID_SDK_ROOT=/home/marvin/.local/share/android-sdk \
JAVA_HOME=/usr/lib/jvm/java-21-openjdk ANDROID_USER_HOME="$PWD/.cache/android-user-home" \
XDG_DATA_HOME="$PWD/.cache/xdg-data" GRADLE_USER_HOME="$PWD/.gradle-user-home" scripts/check-android-suite.sh
```

If the suite's floor check complains about new tests, raise the floor to the measured count.

## Parallelität

**No cut: a single strand.** T1–T3 all meet in `ReprisePlaybackService.kt` and its test file, so
there is no disjoint file group. Ownership:

- `android/app/src/main/java/io/github/marvinbaudach/reprise/{ReprisePlaybackService,TrackAnalysisBackfillPolicy}.kt`;
- an optional new sibling helper;
- `android/app/src/test/java/io/github/marvinbaudach/reprise/ReprisePlaybackService*Test.kt`;
- `TrackAnalysisBackfillPolicyTest.kt`;
- this plan.

Sibling plans running at the same time are #1096, #1097 and #1091. #1096 may touch
`ReprisePlaybackService.kt` only if its grill decides to change the notification's next command.
Check this right before the code phase.

**Merge order:** none.

**Post-merge cross-checks:**
1. The Android suite and the Android lint stage on `dev`.
2. An emulator run under host load, on the 2026-10-05 setup, with a library holding pending tracks:
   - the backfill's `done` count rises while playback runs through BUFFERING blips;
   - the per-thread restarts stop: one thread id per run, not one per blip;
   - a pause longer than 10 s logs one cancel.

## Grill decisions (2026-10-06)

- **G1: Both mechanisms.** They are independent and are tested separately.
  - (a) The policy input becomes play intent: `playing = snapshot.state` is `PLAYING` **or** `BUFFERING`.
    That is the same test `PlaybackUiState.hasPlayIntent` uses for `visualizerActive`, so reuse it
    rather than restating it. This removes the measured cause without a timer.
  - (b) On top of that, the 10 s grace period from D1 covers every other departure from play intent:
    a short pause, or a PAUSED/STOPPED flicker during a track change.
- **G2: The grace is tracked separately and is idempotent.**
  - `analysisBackfillRunning` keeps meaning "a run is requested". A separate pending-stop state, the
    posted `Runnable` or a flag, says a delayed cancel is scheduled.
  - Further not-playing snapshots inside the window do not post a second cancel or restart the timer.
  - Returning to play intent removes the pending cancel.
  - Extra test: `two_pause_snapshots_inside_the_grace_cancel_once`.
- **G3: The tests for (a) and (b) are separate.**
  - `a_buffering_blip_keeps_the_backfill_running` proves (a): no Looper advance is needed, and there is
    no cancel even when idled past the grace.
  - The PAUSED tests prove (b).
- **G4: Battery saver and `onDestroy` cancel at once** (D2). No Rust change (D4). No rule (D5).
- **G5: Ownership.** This strand alone owns `ReprisePlaybackService.kt`. The sibling plans (#1096,
  #1091) must not edit it.
