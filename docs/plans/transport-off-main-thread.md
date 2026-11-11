---
slug: transport-off-main-thread
worktree:
branch:
phase: planned
codex_session:
created: 2026-11-11
strands: a,b
merge_order: a,b
---
# Play, skip, pause, seek and stop never freeze the window on a stuck source

Closes #1327. Follow-up of #1323, which #1329 (`a3b9beb3e8`) fixed for the start
watchdog's own restart; its plan listed the paths below as known residuals. Base:
`origin/dev` at `003ae49fd3`.

This is the mother plan. It holds the shared context, the decisions, the cut and
the post-merge check, and it stays frozen once the plan phase ends. The work is
in the strand files:

- `docs/plans/transport-off-main-thread-a.md`, the player backend
  (`reprise-platform-linux`);
- `docs/plans/transport-off-main-thread-b.md`, the frontend's fault handler
  (`reprise-gnome`).

## Problem in one paragraph

On a dead hard NFS or SMB mount, `filesrc`'s `open()` and `read()` block in the
kernel. The player calls blocking `set_state` and seeks on the GLib main context,
or while it holds the playbin lock: in `try_play`, `stop`, `toggle_pause`,
`seek_to`, `abort_crossfade`, the CUE `complete_start`, and the crossfade
`promote`. Each of them freezes the window (strand A). Once the backend no
longer blocks, a track on a dead mount ends in the frontend's fault handler,
which runs `is_file()` and `mark_track_missing_if_current` on the main thread and
freezes there instead (strand B). Only both strands together take the freeze
away.

## Decisions (grill, 2026-11-09 to 2026-11-11)

1. **Scope.** The player backend, plus the frontend's fault handler. The
   frontend's success path is already off the main thread (scout, 2026-11-11).
   The first draft named cover and lyrics loading as a residual, and that was
   wrong.
2. **Failure channel.** `play` waits up to 250 ms (`START_ANSWER_WAIT`) for the
   start's answer. A fast failure such as a missing file stays a synchronous
   `Err`, so the frontend's `Ok` side effects (scrobble, history, now-playing
   marker) never run for a file that never plays. Only a start that hangs is
   answered `Ok` after the bound and left to the watchdog.
3. **A fresh pipeline per start.** The previous pipeline is retired. The
   wedged-pipeline retry (`rebuild_playbin`, retry once) is removed, because no
   instance ever sees a second start.
4. **One ordered command queue per pipeline.** Start, pause, seek, the CUE
   completion and the retiring `Null` all go through it. `call_async` is not
   ordered, so it cannot serve here. `toggle_pause` follows the state the user
   asked for. `seek_to` waits up to the same bound and returns `Err` on a
   timeout, which lets the frontend's one-shot retry take over.
5. **No UX rule change.** No PLAY-27. FB-6 and PLAY-4a/5a/5b keep their
   behaviour, including FB-6's toast wording.
6. **Strand B's toast.** The skip happens at once. The toast follows the probe's
   answer with today's wording. If the probe has not answered within about 1 s,
   a neutral "could not play" toast is shown, and exactly one toast appears per
   fault.
7. **The cut.** Two strands, as below. #1333, filed during the grill for the
   frontend part, is folded into strand B.

## The cut

| Strand | Purpose | Owns |
|---|---|---|
| A | Backend: per-pipeline queue, fresh pipeline per start, bounded answer, stop, pause, seek, CUE completion, watchdog restart from a start record, crossfade | `crates/reprise-platform-linux/src/{player.rs,player/**,crossfade.rs,crossfade/**,player_pipeline.rs}`, its strand file |
| B | Frontend fault handler: skip at once, probe on `gio::spawn_blocking`, toast after the answer or a neutral one after about 1 s | `crates/reprise-gnome/src/ui/playback/playback_faults.rs`, its tests, a probe seam in the same directory, its strand file |

The file groups are disjoint, and neither strand needs the other to build or to
test. A's tests run on FIFOs, and B's test injects a blocking probe. No public
type or trait changes: `PlaybackBackend` keeps its signature.

**Merge order:** none required. Land A first only by convention, since A is the
issue's core.

**Post-merge cross-checks** (orchestrator, with both strands on `dev`):

1. The full merge-readiness gate on a clean tree, including the unfiltered
   `reprise-gnome` suite and strand B's new display test.
2. Read both diffs together for the seam: the bus `Error` from strand A's
   watchdog give-up (`StartNeverFinished`) and A's synchronous `Err` both still
   reach strand B's `handle_unplayable_track`. Neither strand can check that
   alone, because neither reads the other's files.
3. A manual check on a real dead mount, which no automated test can reach. Mount
   an NFS export, start a track from it, then stop the server. Play, skip, pause,
   seek and stop must keep the window responsive, and after about 24 s the track
   is skipped with one toast.

## Coordination

- #1332 (`003ae49fd3`, the seek dedupe) has landed. Strand A's line numbers in
  `segment.rs` are from after it.
- #1316 (a CUE start hanging in PAUSED after preroll) is still open. When this
  plan was written it had no branch and no worktree. If a branch for it appears
  before strand A lands, whichever lands second rebases. Both touch
  `start_watchdog.rs` and `complete_start`.

## Parallelität

Two strands (A, B), no merge order, disjoint file ownership as in the table
above. Strand A cannot be cut further: the per-pipeline queue changes the type of
the shared playbin slot, which every one of its tasks touches, and a promoted
crossfade secondary must carry that same type. Post-merge cross-checks: see "The
cut".
