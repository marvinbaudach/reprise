---
slug: one-missing-file-no-longer-ends-the-queue-a
worktree: /home/marvin/Projects/reprise-one-missing-file-no-longer-ends-the-queue-a
branch: feature/one-missing-file-no-longer-ends-the-queue-a
phase: shipped
codex_session:
created: 2026-09-01
---
# Strand A — Every playback surface obeys FB-6

Mother plan: `docs/plans/one-missing-file-no-longer-ends-the-queue.md`. Read it
and `docs/plans/android-source-error-on-synced-track.findings.md` first; the
decisions D1–D6 there are settled and are not to be relitigated.

## File ownership

Touch only:

- `crates/reprise-core/src/playback/**`
- `crates/reprise-android-ffi/**`
- `crates/reprise-gnome/src/ui/playback/playback_faults.rs`
- `crates/reprise-runtime/**`

**Do not edit `crates/reprise-core/src/lib.rs`** — the module trees are already
declared. Do not touch any Kotlin; strand B owns it.

## A1 — Lift the skip guard into core

`crates/reprise-gnome/src/ui/playback/playback_faults.rs` holds the rule
privately:

```rust
fn should_stop_skipping(consecutive_skips: usize, queue_len: usize) -> bool {
    queue_len == 0 || consecutive_skips >= queue_len
}
```

Move it into `crates/reprise-core/src/playback/fault_policy.rs`, beside
`playback_fault_policy`, and export it from
`crates/reprise-core/src/playback.rs`. Keep the semantics **exactly** as they
are — this task moves a rule, it does not tune one.

Unit-test the boundaries in core: empty queue, `skips == queue_len - 1`,
`skips == queue_len`, `skips > queue_len`.

Then make `playback_faults.rs` call the core function and delete the private
copy. Everything else in that file — `failure_limit`, `consecutive_skips`,
the toast branch — stays as it is; GNOME's behaviour must not change at all.

## A2 — The Android Error arm skips

`crates/reprise-android-ffi/src/playback_session.rs:452`. Today:

```rust
PlayerEvent::Error(message) => {
    state.snapshot.state = AndroidPlaybackState::Stopped;
    state.snapshot.error = Some(message.into_message());
    state.current_loaded = false;
    (FollowUp::Stop, None, None)
}
```

**Model this on the `TrackFinished` arm (line 416), not the `AdvancedToNext`
arm.** The two are not interchangeable and picking the wrong one produces a
silently dead player:

- `TrackFinished` — the player is **idle**, so it does `advance_auto()`,
  `adopt_current()`, and returns **`FollowUp::Start`**, which calls
  `start_current()` and actually plays the track.
- `AdvancedToNext` — Media3 has **already** transitioned itself, so it only
  returns `FollowUp::Feed(state.next_uri())`, and `Feed` does nothing but
  `backend.set_next(…)`. It never starts anything.

After a fault the player is idle, exactly like `TrackFinished`. So the arm must
return `FollowUp::Start` on a successful advance, and `FollowUp::Stop` when
`advance_auto()` returns `None`.

Two things from `TrackFinished` that must **not** be copied: a fault is not an
automatic advance, so do not increment `snapshot.automatic_advance_count`, and
the faulted track did not play, so do not record it via `play_to_record(true)`.

Requirements:

- **The banner carries FB-6's sentence, not the backend string** (D4/D5). Map
  `playback_fault_policy(..).notices[0]` to its English sentence here in Rust
  and put that in `snapshot.error`. Pass `file_exists = true`, i.e.
  `CouldNotPlaySkipped`: a SAF URI cannot be probed with `Path::is_file`, and
  Android's own scan is what marks tracks missing. Do not add an FFI probe.
- Log the raw backend message with `tracing` so it still reaches logcat — it
  just does not reach the UI.
- `snapshot.error` must be cleared on the next successful start, or the banner
  outlives the track it describes.
- Count consecutive faults on the session state; consult A1's guard. **Latch
  the bound**: take `state.queue`'s length at the *first* fault of a run and
  keep using that value until a successful start resets both counter and latch.
  GNOME does the same thing through `failure_limit(..)`, because a queue that
  shrinks while skipping would otherwise move the target mid-run. A1 does not
  move `failure_limit` — only `should_stop_skipping` — so this latch is
  Android's own small piece of state, not a shared helper.
- Reset the counter on **any** successful start, not only a fault-free one.
- When the guard trips, stop and set a distinct message — "too many unplayable
  tracks", not the single-fault sentence.
- Persist the advanced queue exactly as the advance arm does
  (`Some(state.queue.clone())`, so `persist_queue` runs).

## A3 — The runtime service skips too

`crates/reprise-runtime/src/transport.rs:433`, the `PlayerEvent::Error` arm.
`player_event` already has `backend` and `library` in scope, and the crate
already has `advance_past_failures(backend, library)` with its own bound
(`up_next.len() + queue.len()`). Call it instead of `self.stop(backend)`, after
recording the `Failure` as it does today.

**Carry over the external guard.** The `TrackFinished` arm checks
`self.external_is_loaded()` first, so a finished podcast or stream never
launches whatever music was queued behind it. The Error arm needs the same
guard for the same reason — GNOME does the equivalent with its
`is_external_mode` early return. A failing radio stream must not start the music
queue.

Two existing tests in `crates/reprise-runtime/src/transport_failure_tests.rs`
assert the old behaviour. **Preserve their intent, rewrite their assertions:**

- `a_backend_error_stops_playback_rather_than_leaving_a_phantom_track` (line
  477) — the real requirement is *no phantom track*. After the change, a
  single-track queue still ends stopped with `track_id: None`; on a longer
  queue the next track must be playing and `current` must name it. Rewrite it
  to assert that, and add the multi-track case.
- `a_backend_error_mid_playback_is_named_rather_than_a_silent_stop` (line 156)
  — `failure_kind` and `failure_track_id` must survive the skip, so a surface
  can still say which track dropped out. This assertion should still hold; if
  it does not, the fix is wrong, not the test.

## A4 — Tests named for the rules they carry

`scripts/check-ux-traceability.sh` requires a rule-named test per `[active]`
rule and forbids `#[ignore]` on them. Write them as `// UX FB-6: …` /
`// UX PLAY-5b: …`, matching the comment idiom already used in
`crates/reprise-core/src/queue_ux_rules_tests.rs`.

In `crates/reprise-android-ffi/src/playback_terminal_event_tests.rs`:

- a fault on a multi-track queue advances and keeps playing;
- a fault on the **last** track still stops — queue exhaustion, not a
  regression;
- a queue where every track faults stops at the bound instead of spinning;
- the counter resets: fault → good track → fault does not stop.

Two tests already in that file assert `Stopped` after an `Error` on
**single-track** queues (`buffering_from_the_failed_stream_cannot_revive_a_stopped_snapshot`
and its neighbour). Both should still pass, because a one-track queue is
exhausted either way — verify that rather than assume it, and if one genuinely
needs changing, say why in the commit message.

## Verification

Runs entirely within this strand's ownership:

```sh
cargo test -p reprise-core playback
cargo test -p reprise-android-ffi
cargo test -p reprise-runtime
cargo test -p reprise-gnome playback
```

The GNOME suite is slow and is known to be red on `dev` for reasons that have
nothing to do with this branch. Before debugging a failure there, check the
control arm — run the same test on the base commit — rather than assuming the
change caused it.

`scripts/check-ux-traceability.sh` reads `docs/ux-rules.md` alongside every
crate — it is a **post-merge** check on the mother plan, not this strand's.
