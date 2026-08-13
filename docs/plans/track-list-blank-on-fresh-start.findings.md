# A1 — the verdict

Measured 2026-08-13 against one release build of `feature/track-list-blank-on-fresh-start`
(base `28103bd584`) carrying throwaway probe-gated arm switches. Seven isolated
starts, each with its own Xvfb display, its own `dbus-run-session` and its own
`XDG_DATA_HOME` holding a copy of the real 2 132-track library database.

Metric: `ROWSALLOC present=<n> allocated=<n>` — descendants of the track list's
`ColumnView` whose type name contains `ColumnViewRow`, and how many of those have
`height() > 0`. Sampled at 0.5 s / 2 s / 5 s. The probe calls only `height()`,
never `measure()`, so it cannot perturb the layout it measures.

| # | Arm | `allocated` @5 s | `value-changed` | Verdict |
|---|-----|-----------------:|----------------:|---------|
| 1 | baseline | **2** | 3 | the bug |
| 2 | `REPRISE_NO_ANCHOR` — no restore at all | **20** | 0 | **flips** (control) |
| 3 | `REPRISE_NO_PRESEED` — no `list_geometry::configure` | 2 | — | no change |
| 4 | `REPRISE_NO_SCROLL_TO` — no final `scroll_to` | 2 | — | no change |
| 5 | `REPRISE_NO_HOLD` — no `AdjustmentHold` | 2 | — | no change |
| 6 | `REPRISE_RESTORE_AFTER_ALLOCATION` — same restore, deferred to first `page_size > 0` | **24** | 2 | **flips** |
| 7 | `REPRISE_FORCE_ALLOCATE` — extra `queue_allocate()` after the last write | 2 | 3 | no change |

`present` is 206–207 in every arm, including the failing ones: the rows are always
there. Only their allocation differs.

## The causal chain

**The viewport restore runs before the `ColumnView` has ever been allocated, and a
restore written onto an unallocated list leaves it unallocated.**

`window_runtime_wiring::wire` (`ui/window/window.rs:497`) performs the model load
and the whole viewport restore; `window.present()` is only reached at `:583`. At
restore time the vadjustment therefore still reports `page_size = 0`, which is
exactly what the original evidence showed
(`SCROLLUPPER writer=anchor.configure … page=0.0`). Arm 6 changes nothing about
*what* is written — same target, same preseed, same hold, same `scroll_to` — only
*when*, and the list renders in full at the restored offset with the playing track
highlighted at the right index. Arm 2, which suppresses the restore entirely,
renders too, but gives up the remembered position; it is the control that proves
no other startup step is implicated.

## What the failing arms refute

- **Not the preseed** (arm 3). Writing `upper` onto an unallocated adjustment
  worsens the picture — it parks the viewport where not even the two allocated
  rows are — but it is not the cause. Suppressing it still leaves 2 allocated rows.
- **Not `scroll_to`** (arm 4) and **not the `AdjustmentHold`** (arm 5). Neither
  writer is implicated on its own.
- **Not a missing layout request** (arm 7). Forcing `queue_allocate()` after the
  last write changes nothing. GTK is not sitting on an un-actioned request; it has
  already decided the list needs no rows, and asking again does not revisit that.

That last one matters for A2: the fix cannot be a nudge. It has to be the ordering.

## Caveat on the exit criterion

The plan required *exactly one* arm to flip. Two did — but they are not two
mechanisms. Arm 2 is the control (no restore at all) and arm 6 is the same restore
merely deferred; both describe the single statement above. Arm 6 is the
informative one, because it keeps every writer and every value and changes only
the timing. Had arm 2 flipped while arm 6 did not, the verdict would have been the
opposite: something in the restore's content, not its schedule.

## Consequence for A2

Follow the plan's "arm 2 or 6 flips (ordering)" branch: `reload_anchor_scroll::apply`
must treat `page_size == 0` as *geometry not available yet* and take the existing
`arm_refinement` path **instead of writing anything** — the precondition check
moves above the `configure` call at `:153` rather than staying below it at `:163`,
and the `scroll_to` at `:43` defers with it. The trigger is one subscription to the
first `page_size > 0`, not a retry loop. `arm_refinement` already exists for exactly
this situation; the defect is that `apply` writes first and asks afterwards.

Per the grill, exactly one switch survives into the shipped code as a permanent
probe-gated counterprobe: `REPRISE_RESTORE_AFTER_ALLOCATION`, the arm that proved
the mechanism. The other five die with the throwaway worktree.
