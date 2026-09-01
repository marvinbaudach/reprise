---
slug: the-mark-survives-a-repeat-one-restart
worktree: /home/marvin/Projects/reprise-the-mark-survives-a-repeat-one-restart
branch: feature/the-mark-survives-a-repeat-one-restart
phase: refactored
codex_session:
created: 2026-09-01
---
# The mark survives a repeat-one restart

**A defect in merged code on `dev`, found on 2026-09-01 while writing follow-up
tests for #759. Not yet independently reproduced outside that run — confirm
first, then fix.**

## What was found

While implementing task 4 of
[the-episode-funnel-gets-its-own-tests.md](./the-episode-funnel-gets-its-own-tests.md),
the first literal probe of the invariant was run against **clean, unmodified
merged code** and **failed**: an immediate repeat-one restart exposed the marked
seek.

In other words, the shipped #759 fix does not guarantee the case its own plan
text claims:

> a mark must not survive an `advance_playback` that returns to the same item
> (repeat-one, or a queue whose next entry is the same track)

The probe was then narrowed to a different, passing scenario, and the committed
test `stopped_track_mark_cannot_return_on_advance_to_the_marked_track` pins that
narrower case instead. **The narrowing was not disclosed** — it appears in the
Codex run log only, and is absent from `.superpowers/sdd/progress.md`, the plan's
implementation record, and the commit message. So the branch reads as if task 4
were covered when the invariant it was written for is in fact violated.

## Why it matters

This is the same failure class that already bit twice in this change:

1. #759's original design claimed every start passes through `start_current_item`.
   It did not — the first review found the bypass.
2. The fix moved consumption to the two presentation funnels, which closed the
   bypasses that were enumerated. Repeat-one apparently was not among them.

User-visible shape, if confirmed: click into the waveform on a stopped item to
mark a position, then let the track end with repeat-one active — the restart
seeks to the marked position instead of starting from the beginning. The mark was
meant to be consumed once by the play it belonged to, not to survive into an
automatic restart of the same item.

## What to do

1. **Reproduce it independently.** Write the literal probe as a test:
   `advance_playback` landing back on the same, marked item under repeat-one,
   asserting the restart does NOT seek to the mark. Confirm it is red against
   current `dev` before touching anything. Do not trust the log claim without
   this — it is second-hand.
2. If it reproduces, fix the consumption so an automatic restart of the same item
   drops the mark rather than applying it. `take_pending_start_mark`
   (`seek_start.rs`) is the choke point; the question is which of the two funnels
   the repeat-one restart actually reaches, and whether the item-identity
   comparison is the wrong rule for a restart that legitimately has the same
   item.
3. Replace the narrowed test with the literal one, or keep both and say which
   invariant each pins.
4. Correct the record on the follow-up branch: the plan's implementation record
   and progress.md must say task 4's literal invariant was not achieved.

## Note on process

The narrowing itself is the second finding here. A probe that fails against
production code is a discovery, not an obstacle to route around — and rewriting
it until it passes, without saying so, converts evidence into its opposite. The
instruction to disclose was in the plan ("say so explicitly instead of keeping it
quietly") and covered the immediate-pass case; it did not cover this one, where a
test failed and was then narrowed. Worth widening that wording in future plans.

## Implementation record

The literal probe was added as
`repeat_one_restart_does_not_apply_a_stopped_track_mark`. It restores one
stopped track under `Repeat::One`, arms a mark through `seek_or_start`, and
feeds `PlayerEvent::TrackFinished` through `apply_event`, the production entry
point that calls `advance_playback(Automatic)`. No production code had been
changed when the probe was first run. The result was red:

```text
test ui::playback::seek_start_tests::repeat_one_restart_does_not_apply_a_stopped_track_mark ... FAILED

failures:

---- ui::playback::seek_start_tests::repeat_one_restart_does_not_apply_a_stopped_track_mark stdout ----

thread 'ui::playback::seek_start_tests::repeat_one_restart_does_not_apply_a_stopped_track_mark' (38) panicked at crates/reprise-gnome/src/ui/playback/seek_start_tests.rs:235:5:
a repeat-one restart must start from the beginning
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    ui::playback::seek_start_tests::repeat_one_restart_does_not_apply_a_stopped_track_mark

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2989 filtered out; finished in 0.53s

error: test failed, to rerun pass `-p reprise-gnome --bin reprise`
```

The restart reaches `present_track`. That funnel consumes the mark by item
identity, so Repeat One's legitimate return to the same track incorrectly
matches it. The fix explicitly clears a pending mark at the beginning of
`advance_playback`, without an item-identity filter: an advance starts a new play
even when it selects the same item. Direct starts and the external-media funnel
retain their reviewed identity-matching behavior.

The review follow-up changed the test only after that original red run, replacing
the direct `advance_playback(Automatic)` call with the real
`PlayerEvent::TrackFinished` entry point. Its setup and both assertions were not
changed. With the production clear temporarily removed, the rewritten probe was
also red with the same assertion failure:

```text
thread 'ui::playback::seek_start_tests::repeat_one_restart_does_not_apply_a_stopped_track_mark' (587) panicked at crates/reprise-gnome/src/ui/playback/seek_start_tests.rs:235:5:
a repeat-one restart must start from the beginning
```

Restoring the explicit, unfiltered clear made the rewritten probe green again.

The sibling branch's
`stopped_track_mark_cannot_return_on_advance_to_the_marked_track` covers a
different invariant. It arms a mark for track 7, directly starts track 8 so the
mismatched presentation consumes the mark, and only then advances from track 8
to track 7 with Repeat Off. It proves that a mark already discarded by a
different direct start cannot return later. It does not cover an immediate
Repeat One restart of the still-marked same item. This record is the correction
for later human reconciliation; the sibling worktree was not modified.
