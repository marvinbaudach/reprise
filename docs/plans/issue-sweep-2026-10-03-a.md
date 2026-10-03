---
slug: issue-sweep-2026-10-03-a
worktree: /home/marvin/Projects/reprise-issue-sweep-2026-10-03-a
branch: feature/issue-sweep-2026-10-03-a
phase: planned
codex_session:
created: 2026-10-03
---
# Strand a — a stop during the portrait phase no longer swallows a later cover pass (#1041)

Mother plan: `docs/plans/issue-sweep-2026-10-03.md`.

## Diagnosis (origin/dev @ 3441c38c59)

- `cover_backfill.rs` `cancel()` (about lines 98–120): with no active cover run it sets the
  sticky `cancel_requested` and returns. That latch exists for the short window between the
  portrait run's `Complete` and the FFI closure calling `start()` for the chained cover pass.
- `crates/reprise-android-ffi/src/artist_portrait.rs:236-239` cancels the portrait backfill and
  then the cover backfill unconditionally. A portrait run that is cancelled publishes idle with
  `run_id == 0` (`backfill.rs` `finish_cancelled`), so the closure never chains and nothing
  consumes the latch.
- The next unrelated `start()` hits `if shared.cancel_requested` in `launch()` (about line 188),
  clears it and silently returns `false`. The "Stop artwork download" menu entry from #1042
  makes this reachable.

## Required behaviour

1. A stop during the portrait phase prevents the cover pass that this portrait run would have
   chained into.
2. A stop during the window between the portrait run's `Complete` and the chained `start()`
   still prevents that chained pass. This is what the latch is for; keep it covered.
3. A stop that has no chained pass to prevent leaves no state behind: the next `start()`,
   whether user-triggered or chained from a later portrait run, runs normally.

Pick the smallest mechanism that satisfies all three, for example clearing the latch when the
portrait run it was meant for ends without chaining, or scoping the latch to that portrait run.
State the choice and why in the commit body.

## Tasks (test-first)

1. Failing test first, in `cover_backfill_tests.rs` or the FFI chain tests, whichever can drive
   the real path: the issue's scenario. Stop during the portrait phase, let that portrait run end
   cancelled, then start a cover pass; it must run (`start()` returns `true`, progress moves).
2. Tests for behaviours 1 and 2, if no existing test already pins them. Name the existing test
   when it does.
3. Implement.
4. Gates: `cargo fmt --check`,
   `cargo clippy -p reprise-core -p reprise-android-ffi --all-targets -- -D warnings`,
   `cargo test -p reprise-core -p reprise-android-ffi`, and the core purity check
   `cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'` (must be empty).
   Every touched code file stays under 800 lines.

Commit subjects are prose in the repo's style; the fix commit body says `Closes #1041`.
