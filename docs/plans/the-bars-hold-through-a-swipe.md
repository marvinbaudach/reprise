---
slug: the-bars-hold-through-a-swipe
worktree: /home/marvin/Projects/reprise-the-bars-hold-through-a-swipe
branch: feature/the-bars-hold-through-a-swipe
phase: planned
codex_session:
created: 2026-10-06
---
# The bars hold through a swipe (#1091)

## Problem

After a next-track swipe in the Android now-playing sheet (visualizer mode), the mirrored bars
sometimes collapse to caps-only or black for 25–450 ms around the settle, then pop back at full
height.

Device run of 2026-10-05: `mid-next` failed 2 of 5, and `slow-next` failed. Evidence is in
`~/.cache/reprise-song-swipe-evidence-2026-10-05/`.

This is the third repair in this area. #980 and #983 each fixed one mechanism. The plan therefore
fixes both mechanisms the code allows, **and** it adds permanent instrumentation, so a device run
names the cause instead of guessing it.

## Facts (origin/dev `d68a21e2a0`)

**One native engine for both panels.** `ReprisePlaybackService.kt:532-538` gives every panel a
`LiveVisualSceneEngineLease` over the single `liveVisualEngine`. The outgoing panel's
`currentBands()` and the incoming panel's `noteTrackChanged`/`adoptShape` therefore hit the same
Rust state.

**The adoption order** (`NowPlayingScene.kt:408-448`), when the new live panel composes:
1. During composition it reads `adoptedBands = previousLiveEngine.currentBands()`. That is the
   engine's *displayed* bars (`engine.rs:243` `display_bands`), not a live-shape snapshot.
2. `DisposableEffect(engine, trackId)` runs `noteTrackChanged()`. In Rust (`visualizer.rs:211-228`)
   this zeroes `bands_current`/`bands_peaks`, sets `has_track(false)` and arms the hold.
3. `DisposableEffect(engine)` runs `adoptShape(adoptedBands)` (`visualizer.rs:287-312`).

**How the display decays.**
- `set_playing(false)` (`visualizer.rs:164-185`) clears `awaiting_stream_after_reset` and sets the
  engine not playing.
- Core `refresh_display_bands` then blends toward the resting shape (idle fade), and the peaks fall.
  The result is caps-only or black.
- `setPlaying` is called from a `SideEffect` with `playback.visualizerActive`
  (`NowPlayingScene.kt:463`), which equals `hasPlayIntent` (PLAYING or BUFFERING).
- Any snapshot during the item change that maps to PAUSED or STOPPED (`Media3PlaybackPort.kt:438-448`)
  turns the visualizer off for that moment.

**When the shape is read.** The new panel composes only when the track id changes, after the
transport answered (`holdSettledPositionUntilTheTransportAnswers`, `NowPlayingSheet.kt:262-271,
449-465`, grace 1500 ms). If the old audio stopped first, or `visualizerActive` blipped false, the
displayed bars have already decayed by then. The adoption then seeds the decayed shape, and the
first PCM block pops to full height. That matches `slow-next` and the fling failures.

**The open question from #980/#983.**
- Does `visualizerActive` blip false through the item transition? The code allows it.
- Nobody has measured it, and there is no log line that would show it.

**The three review minors**, still present:
- (a) An empty `ingest_bands` leaves the hold set (`:253-255`).
- (b) `reset_live_presentation` arms the hold regardless of `state.playing` (`:537`).
- (c) `reset_stream_holds_the_bar_shape_until_the_next_stream_speaks_or_playback_stops`
  (`visualizer_stream_reset_tests.rs:160-228`) never pins the clear.

**Rules.** AC-29 [core] [gtk] (`docs/ux-rules.md:4699`) says "A track change and a seek keep the
bar shape on screen … instead of collapsing to zero". No `[android]` rule covers the swipe.

**Sibling work.** #1141 (`.worktrees/viz-quiet-intro`) touches `cava/boundary.rs` and the boundary
tests, including `visualizer_boundary_tests.rs`, and AC-29's text. It does not touch
`visualizer.rs`, the hold, adopt or reset path, or any Kotlin.

## Decisions (as drafted — see "Grill decisions" at the end, which wins where they differ)

**D1: The engine keeps a last live shape.**
- `AndroidVisualEngine` records the displayed bars on every tick where live PCM was analysed while
  playing (`has_live_audio && playing`), as `last_live_bands`.
- `noteTrackChanged` does not clear it. Only `set_has_track(false)` with no track at all, or a stop,
  clears it.
- A new export, `adoptable_bands()`, returns `last_live_bands` when present, else `current_bands()`.
- Kotlin's adoption reads `adoptable_bands()` instead of `currentBands()`.
- The effect: a decayed display is never what gets adopted.

**D2: The visualizer stays on through a committed swipe.**
- From the swipe commit (`settleTrack` with `changesTrack`) until the transport answers or the grace
  period ends, the scene passes `setPlaying(true)`, even if a transient snapshot says PAUSED or
  STOPPED.
- A real pause by the user is not a committed swipe. Its path is unchanged.
- This removes the "collapse before the settle" frames.
- The sheet already knows the window through `holdSettledPositionUntilTheTransportAnswers`. A flag,
  `trackChangeInFlight`, reaches `updateVisualSceneEngine` as an input.

**D3: The minors.**
- (a) An empty `ingest_bands` clears the hold, because "the new stream spoke: nothing".
- (b) The hold is armed only while `state.playing`.
- (c) A test pins the flag's clear through an observable effect: after a live block, a stop decays the
  bars at once.

**D4: Permanent instrumentation.**
- One `Log.d` line each, tag `RepriseVisualizer`, at these events: `setPlaying` edges (with the
  snapshot state), `resetAudioStream`, `noteTrackChanged`, `adoptShape` (with the band energy and
  whether it came from `last_live_bands`) and the swipe commit.
- The 2026-10-06 spectrum run showed that `Log.d` is visible in the release build.
- Rate: only at edges, never per tick.

**D5: Rule.** Add an Android paragraph to AC-29, a prose deviation in the ACC-8 shape: "On the phone,
a swipe to the next or previous song keeps the outgoing bar shape on screen until the new song's
audio speaks."
- Its tests carry the `ac_29_` prefix.
- AC-29's text is also edited by #1141, so this paragraph goes at the **end of AC-29's
  Stream-boundaries block**, not near lines 4706/4733, to stay out of #1141's hunks.
- Alternatively, a new `[android]` rule could hold the text. The grill picks.

**D6: Device diagnosis is a post-merge check, not a pre-code step.**
- The phone is held by another session.
- The harness assumes the phone's 1080x2404 geometry and exactly one attached device.

## Tasks (test first)

**T1: Rust last live shape.** `crates/reprise-android-ffi/src/visualizer.rs`, plus a sibling if it is
near the cap. Tests go in `visualizer_shape_adoption_tests.rs`, or a new `visualizer_swipe_hold_tests.rs`:
- `ac_29_adoptable_bands_survive_a_stop_of_the_old_stream`: live blocks, then `set_playing(false)`,
  then ticks until the display decays. `adoptable_bands()` still equals the last live shape, and
  `current_bands()` does not.
- `ac_29_a_track_change_keeps_the_last_live_shape_for_adoption`.
- `ac_29_adoptable_bands_fall_back_to_the_display_without_live_audio`.
- The tests for minors (a), (b) and (c) from D3.

**T2: Kotlin adoption source.** `NowPlayingScene.kt` reads `adoptableBands()`. The fake engines in
`NowPlayingSceneEngineTest.kt` gain it. Test: `the_new_live_panel_adopts_the_last_live_shape_not_the_decayed_display`.

**T3: Visualizer on through the swipe.** `NowPlayingSheet.kt` exposes `trackChangeInFlight`, and
`NowPlayingScene.kt`/`updateVisualSceneEngine` ORs it into `setPlaying`. Tests:
- `a_committed_swipe_keeps_the_visualizer_playing_through_a_paused_snapshot`;
- `a_user_pause_still_stops_the_visualizer`;
- `the_hold_ends_when_the_transport_answers_or_the_grace_expires`.

**T4: Instrumentation** (D4). It needs no test beyond compiling. Keep it edge-triggered.

**T5: Rule** (D5). It lands in the same commit as T1–T3, or as the last commit with every named test
present.

## Verification (worker, in the worktree)

AGENTS.md's "all gates before every commit" is overridden for this run; the orchestrator runs the
full gates after the code phase. Do NOT run the unfiltered `cargo test --workspace`,
`check-merge-readiness.sh` or the GNOME display suites. Every cargo and Gradle command runs under
`heavy-run heavy --`.

Run:
- `cargo fmt --check`;
- `cargo clippy -p reprise-android-ffi --all-targets -- -D warnings`;
- `cargo test -p reprise-android-ffi visualizer`;
- the Android suite, with the worktree-local env prefix below;
- `scripts/check-ux-traceability.sh`.

```
ANDROID_HOME=/home/marvin/.local/share/android-sdk ANDROID_SDK_ROOT=/home/marvin/.local/share/android-sdk \
JAVA_HOME=/usr/lib/jvm/java-21-openjdk ANDROID_USER_HOME="$PWD/.cache/android-user-home" \
XDG_DATA_HOME="$PWD/.cache/xdg-data" GRADLE_USER_HOME="$PWD/.gradle-user-home" scripts/check-android-suite.sh
```

The `cava_tests` `ac_26`/`ac_28` failures under load are known flakes.

## Parallelität

**No cut: a single strand.** The new export crosses UniFFI (Rust and Kotlin compile together),
and T2 and T3 both edit `NowPlayingScene.kt`. Ownership:

- `crates/reprise-android-ffi/src/visualizer.rs` and its `visualizer_*_tests.rs`, **except**
  `visualizer_boundary_tests.rs`, which #1141 owns;
- `NowPlayingScene.kt`, `NowPlayingSheet.kt`, `NowPlayingSceneModel.kt` and their tests;
- the AC-29 Android paragraph only, at the end of its Stream-boundaries block;
- this plan.

It must not touch `ReprisePlaybackService.kt`, which #1129 owns.

**Merge order:** after #1141 if both are ready, because both edit AC-29. Otherwise none.

**Post-merge cross-checks:**
1. The full gate on `dev`.
2. A phone run with the 2026-10-05 harness, once the phone is free:
   - `hist-then-next` ×2;
   - `mid-next` ×5;
   - `slow-next` ×2.

   No dark span may exceed 3 frames (`gap.py`). The `RepriseVisualizer` log lines show whether
   `setPlaying` blipped false and which shape was adopted.
3. The control: a track change from the play button behaves as before.

## Grill decisions (2026-10-06) — these override the drafted Decisions and Tasks

- **G1: The scope is split.** This plan ships D1 (the last live shape plus `adoptable_bands()`, and
  Kotlin adopts it), D3 (the three minors) and D4 (permanent edge-triggered `Log.d`).
  - **D2 and T3 are dropped from this plan.** Forcing `setPlaying(true)` cannot hold the bars:
    `expire_stale_live_audio` (`visualizer.rs:549-553`) resets the presentation with
    `holds_display = false` once the old stream's PCM goes stale.
  - A commit-time hold follows in a later plan, after an instrumented phone run shows which
    mechanism fires: a `setPlaying(false)` blip, staleness, or a decayed adoption.
- **G2: The AC-29 Android paragraph goes in now.**
  - Place it at the end of AC-29's "Stream boundaries" block, away from #1141's hunks at about lines
    4706 and 4733. The exact text: "On the phone, a swipe to another song hands the new song's bars
    the outgoing song's last live shape, never an already decayed one."
  - Its tests carry the `ac_29_` prefix: the T1 tests and the Kotlin adoption test.
  - Check how `scripts/check-ux-traceability.sh` maps AC-29's `[core] [gtk]` levels to Rust tests in
    `reprise-android-ffi`, and name the tests so the gate counts them.
  - Do not add `[android]` to AC-29's level tags unless the gate requires it for the Kotlin test.
- **G3: Merge order.** Land after #1141 (`fix/visualizer-quiet-intro`) if both are ready. If not,
  whichever lands second rebases; the hunks are disjoint.
- **G4: File caps.**
  - `NowPlayingSheet.kt` is 794 lines. The swipe-commit log line must not push it past 800: put it in a
    small helper in a sibling file, or in the scene side.
  - `visualizer.rs`: put new code in a sibling if it nears 800.
- **G5: Ownership.**
  - This plan owns `visualizer.rs`, the new sibling module, `visualizer_shape_adoption_tests.rs` or a
    new `visualizer_*_tests.rs`, and `visualizer_stream_reset_tests.rs` for minor (c).
  - On the Kotlin side it owns `NowPlayingScene.kt`, `NowPlayingSceneModel.kt`,
    `NowPlayingSceneEngineTest.kt` and the fake engines that implement the scene-engine interface.
  - It also owns the AC-29 paragraph.
  - It must not touch `visualizer_boundary_tests.rs` (#1141), `ReprisePlaybackService.kt` (#1129) or
    `PendingDeletions.kt` (#1096).
