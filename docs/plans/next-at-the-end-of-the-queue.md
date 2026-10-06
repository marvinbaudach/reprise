---
slug: next-at-the-end-of-the-queue
worktree: /home/marvin/Projects/reprise-next-at-the-end-of-the-queue
branch: feature/next-at-the-end-of-the-queue
phase: shipped
codex_session:
created: 2026-10-06
---
# Next at the end of the queue (#1096)

## Problem

On Android, pressing Next on the last track of the play order with Repeat off stops playback and
clears the current index. `previous_in_queue_order()` at position 0 does nothing.

Next is reachable without any guard:
- the notification, headset and widget (`CoreControlledPlayer.seekToNext`/`seekToNextMediaItem`);
- the volume-key skip gesture;
- dock mode and the mini player (always enabled).

The now-playing sheet is the one surface that clamps Next at its last panel.

## Facts (origin/dev `d68a21e2a0`)

**The path.**
- `AndroidPlaybackSession::next()` (`crates/reprise-android-ffi/src/playback_session.rs:583-587`)
  first tries `forward_from_history()` (PLAY-14), then `move_playhead(Queue::next_manual)`.
- `move_playhead` (`:733-752`) calls `stop_backend()` when the move yields no track. That stop
  (`:411-420` → `SessionState::stop()`, `:224-231`) sets `Stopped`, `current_index = None` and
  position 0.
- `Queue::next_manual_matching` (`crates/reprise-core/src/queue.rs:177-190`) sets
  `self.pos = target`. `forward_matching_position` (`:193-217`) returns `None` at the end with
  `Repeat::Off`, so the core cursor is cleared as well.
- `previous_in_queue_order` (`:599-616`) returns `Ok(())` at position 0. Its test is
  `previous_in_queue_order_at_the_first_position_is_a_no_op`
  (`queue_boundary_reorder_tests.rs:321`).

**Other users of the same core call.** `Queue::next_manual` / `next_manual_matching` is also used
by:
- the Android fault-skip path (`playback_session/stream_events.rs:151`): when nothing follows it
  stops at exhaustion, pinned by `fb_6_fault_on_the_last_track_stops_at_queue_exhaustion`;
- the desktop manual advance (`reprise-gnome/src/ui/playback/up_next_transport.rs:53`).

The core function must therefore keep its semantics.

**Kotlin callers that mean "leave this track".** `PendingDeletions.kt:161` calls
`playback.next()` when the track being deleted is the current one. That caller needs
"advance or stop", never "do nothing".

**Natural end.** The last track ending with Repeat off goes through `advance_auto`
(`stream_events.rs:89-125`) and also ends in `state.stop()`. PLAY-8 [core]
(`docs/ux-rules.md:460-471`) says: "After the last context track, playback ends with Repeat off …".
That part is correct and stays.

**Desktop** (for reference only, unchanged): Next stays sensitive while the queue has tracks
(`player_bar_external.rs:33`, MPRIS `can_next = queue_has_tracks`). A manual next past the end
refills the queue from the visible view (`up_next_transport.rs:259`); otherwise it resets to
stopped.

**Rules.** No rule covers a manual Next on the last track, on either platform.

**Platform convention.** Media3's own `seekToNext()` does nothing when there is no next item and
the item is not live. On the phone, a no-op is therefore what the system surfaces already imply.

## Decisions (as drafted — see "Grill decisions" at the end, which wins where they differ)

**D1: A manual Next on the last track with Repeat off does nothing.**
- `next()` returns `Ok(())` with no state change, no backend call and no notify.
- Playback, position and `current_index` stay as they were.
- This mirrors `previous_in_queue_order` at position 0 and Media3's convention.
- Rejected alternative: keep the stop but leave `current_index` on the last track. That still
  ends the song on a stray headset press, which is the user-visible harm.

**D2: The check happens before anything moves.**
- `next()` asks whether the queue has a manual forward target, after the history check, and returns
  early when it has none.
- A small additive core query does this, for example `Queue::has_manual_next()` over
  `forward_matching_position(false, |_| true)`.
- `next_manual` and `next_manual_matching` themselves stay unchanged, so desktop, the fault-skip
  path and FB-6 are untouched.

**D3: Deleting the playing last track still leaves it.**
- The Kotlin pending-deletion path must not inherit the no-op.
- Proposed: a separate export `skip_current_or_stop()` that keeps today's `move_playhead(Queue::next_manual)`
  behaviour. `PendingDeletions.kt` calls it instead of `next()`.
- The worker confirms the Rust trash path's own behaviour (`trashing_the_last_playing_track_stops_playback`)
  is independent of this export and stays green.

**D4: Repeat One and Repeat All are unaffected.** With Repeat All, `next` wraps to the start as
today. With Repeat One, a manual next follows the existing `next_manual` rules.

**D5: The rule.** Add **PLAY-8b [core] [android]** (the ID must be verified free; take the next
free suffix otherwise): "On the phone, Next on the last track of the play order with Repeat off
does nothing — like Previous on the first. Playback continues. The automatic end of the last
track still ends playback (PLAY-8)."
- Rule-named tests carry the `play_8b_` prefix.
- The worker checks how `scripts/check-ux-traceability.sh` maps `[android]` (Kotlin `fun play_8b_…`
  under `android/app/src/test`), and whether Rust tests in `reprise-android-ffi` count for `[core]`.
  The worker names the tests so that the gate sees every level the rule claims.
- If the gate reads the `[android]` level from Kotlin only, a Kotlin test drives the export
  through the existing fakes.
- Mark it `<!-- REVIEW: rule proposal -->`, the same way NAV-15d did.

## Tasks (test first)

**T1: Rust.** In `crates/reprise-android-ffi/src/playback_session.rs` (plus a sibling if the file is
near the 800-line cap), with the tests in `playback_session/…_tests.rs` beside
`queue_boundary_reorder_tests.rs`. Tests:
- `play_8b_next_on_the_last_track_with_repeat_off_is_a_no_op`: state, `current_index`,
  position and the backend calls are unchanged. It fails before the fix, because the state is
  `Stopped` and the index `None`.
- `play_8b_next_on_the_last_track_with_repeat_all_wraps`.
- `play_8b_next_after_a_back_step_still_returns_through_history`: PLAY-14 is intact at the last
  position.
- `skip_current_or_stop_on_the_last_track_stops`.
- `fb_6_fault_on_the_last_track_stops_at_queue_exhaustion` stays green unchanged.

**T2: Core query.** `crates/reprise-core/src/queue.rs`, tested in `queue_tests.rs`: the new
query is true before the end, false at the end with Repeat off, and true with Repeat all.

**T3: Kotlin.** Switch `PendingDeletions.kt` to the new export. Adjust the Kotlin fake playback
controls and interfaces that list the exports, and add a test that deleting the playing last
track still leaves it.

**T4: Rule.** Add PLAY-8b to `docs/ux-rules.md` in the same commit as the code that makes it true.

## Verification (worker, in the worktree)

AGENTS.md's "all gates before every commit" is overridden for this run; the orchestrator runs the
full gates after the code phase. Do NOT run the unfiltered `cargo test --workspace`,
`scripts/check-merge-readiness.sh`, or the GNOME display suites. Every cargo and Gradle command
runs under `heavy-run heavy --`.

Run:
- `cargo fmt --check`;
- `cargo clippy -p reprise-core -p reprise-android-ffi --all-targets -- -D warnings`;
- `cargo test -p reprise-core queue`;
- `cargo test -p reprise-android-ffi playback`;
- `cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'`, which must print nothing;
- the Android suite, with the worktree-local env prefix below;
- `scripts/check-ux-traceability.sh`.

```
ANDROID_HOME=/home/marvin/.local/share/android-sdk ANDROID_SDK_ROOT=/home/marvin/.local/share/android-sdk \
JAVA_HOME=/usr/lib/jvm/java-21-openjdk ANDROID_USER_HOME="$PWD/.cache/android-user-home" \
XDG_DATA_HOME="$PWD/.cache/xdg-data" GRADLE_USER_HOME="$PWD/.gradle-user-home" scripts/check-android-suite.sh
```

## Parallelität

**No cut: a single strand.** The new export crosses UniFFI, so Rust and Kotlin compile together;
splitting them would leave the generated bindings ahead of their consumer. Ownership:

- `crates/reprise-android-ffi/src/playback_session.rs` and new sibling tests;
- `crates/reprise-core/src/queue.rs` and `queue_tests.rs`;
- `PendingDeletions.kt`, the Kotlin playback-controls interface and fakes that list the exports,
  and their tests;
- the PLAY-8b block in `docs/ux-rules.md`, section-aware;
- this plan.

Sibling plans running now are #1129 (`ReprisePlaybackService.kt`) and #1091 (visualizer files).
Neither owns any of the files above. The CUE wave-3 plan is still in planning; check its ownership
right before the code phase.

**Merge order:** none.

**Post-merge cross-checks:**
1. The full gate on `dev`.
2. On a device or the emulator, at the last track with Repeat off: notification Next, headset or
   volume-key skip, and the mini player each keep the song playing. Deleting the playing last track
   still stops it.

## Grill decisions (2026-10-06)

- **G1: No-op confirmed (D1).** A manual Next on the last track with Repeat off changes nothing.
  The automatic end still ends playback (PLAY-8).
- **G2: Separate export confirmed (D3).** `skip_current_or_stop()` keeps today's
  advance-or-stop semantics for `PendingDeletions.kt` only. `next()` stays the pure user gesture.
- **G3: Rule PLAY-8b, level `[android]`,** marked `<!-- REVIEW: rule proposal -->`. Verify the ID
  is free, and take the next free suffix if it is not.
  - Kotlin tests carry `fun play_8b_…` under `android/app/src/test`, so the traceability gate sees the
    `[android]` level.
  - The Rust tests may carry the prefix too.
  - Read `scripts/check-ux-traceability.sh` for the exact matching rule before naming them.
- **G4: Untouched files.**
  - `ReprisePlaybackService.kt:203/581` route the media-session Next to `next()`. That is a user
    gesture and must stay a no-op. It needs no change, and #1129 owns that file.
  - Do not edit it.
- **G5: File cap.** `playback_session.rs` is 756 lines. Put new code and tests in siblings, for
  example `playback_session/next_at_end.rs` plus a `_tests.rs`, so the file stays under 800.
