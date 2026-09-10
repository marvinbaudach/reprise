---
title: The compact→library switch shows one wrong-size frame for a sixth of a second
phase: findings
date: 2026-09-09
---

# Compact → Library: where the visible breakage comes from

User report: switching from compact view to full view looks slow and broken —
the full app appears in a small corner first and only then spreads over the
screen.

## How this was measured

- Worktree on `origin/dev` (`11bfff321f`), `target/` seeded by btrfs reflink.
- **Release** build (a debug build would have made every number meaningless).
- Real user library (1 959 tracks), real Wayland/GNOME session — not Xvfb.
  Under X11 the compositor round-trip behaves differently, so a headless
  "green" here would have proven nothing.
- Driven by the app's own smoke hook (`REPRISE_SMOKE_MINIMAL_VIEW`), extended
  to a `cycle` mode: settle 10 s, then 3 enter/restore pairs 4 s apart. No
  synthetic input — measured from a warm Library tree, not one second into
  startup.
- Instrumentation: `add_tick_callback` logging every presented frame with the
  toplevel's actual size, plus notify probes on both property spellings.
- n = 4 restores, `measure-cycle.log`.

## Result

Restore #0 is the startup `apply_initial()` path (window not yet mapped,
`width=0`), not a toggle. It is listed separately and never averaged in.

Two independent runs, six toggles:

| Restore | Body blocks | `set_content` | Wrong-size frames | Body end → first correct frame | First correct frame |
|---|---|---|---|---|---|
| #1 | 101 ms | 68 ms | 1 | 108 ms | 209 ms |
| #2 | 71 ms | 70 ms | 1 | 93 ms | 164 ms |
| #3 | 78 ms | 92 ms | 1 | 113 ms | 190 ms |
| #1 (run 2) | 68 ms | 68 ms | 1 | 107 ms | 175 ms |
| #2 (run 2) | 70 ms | 70 ms | 1 | 92 ms | 162 ms |
| #3 (run 2) | 93 ms | 92 ms | 1 | 99 ms | 192 ms |
| #0 startup | 45–46 ms | 46 ms | 1 | 336–396 ms | 383–441 ms |

Breakdown inside `restore_library()` — everything except the remount is noise:

```
set_content_ms = 46.4 / 67.7 / 70.1 / 92.3
css_ms         = 0.0008 / 0.006 / 0.005 / 0.005
geometry_ms    = 0.005 / 0.198 / 0.173 / 0.190
```

After the jump there is one more dropped frame (gap 79–177 ms) before the
frame clock settles to ~16 ms.

## Two costs, and they are independent

**Cost A — the remount blocks the main thread for 46–92 ms.** Measured
directly inside the function body, four times, tight. `set_content` is the
whole of it.

**Cost B — a further ~100 ms passes before the first correct-size frame.**
Measured from the end of the body: 92, 93, 99, 107, 108, 113 ms across six
toggles. **This gap is constant and does not track the remount cost** — a
68 ms remount produced a 107 ms gap, a 101 ms remount produced a 108 ms gap.

So the two are additive, not causal. An earlier draft of this document claimed
the remount is what stretches the wrong-size frame on screen; the gap column
refutes that. Time to the first correct frame ≈ body + ~100 ms.

Cost B was not decomposed further. It contains the Wayland configure
round-trip *and* the first layout/paint of the Library tree at full size; this
measurement cannot separate them. ~100 ms is about six frames at 60 Hz, which
is long for a round-trip alone, so the first full-size layout is the likelier
majority — stated as a hypothesis, not a result.

## What the user actually sees

1. `t=0` — menu item activated.
2. `t=0…92 ms` — main thread blocked in `set_content`, remounting the Library
   tree. The resize request has not gone out yet.
3. `t≈72–186 ms` — **one** frame is presented: window still 430×76, but the
   Library content is already in it. This is the "app in a small corner".
4. `t≈162–209 ms` — first frame at 1728×1048, maximized.
5. one further dropped frame, then 60 Hz.

Visible disruption per toggle: roughly 250–400 ms.

## Two plausible causes that the measurement REFUTED

- **Not a double configure.** `set_default_size(...)` followed by `maximize()`
  was suspected of producing an unmaximized 1728×1048 frame before the
  maximized one. No such frame exists in any trace — GTK coalesces both into
  one `present_toplevel` within the tick.
- **Not "many frames of squeezed Library".** Exactly one wrong-size frame per
  restore, every time. On Wayland one such frame is the structural minimum: the
  content swap is local and paints immediately, the resize needs a round-trip.

The existing code comment and the pinned test
(`library_root_is_mounted_before_full_geometry_is_requested`) are therefore
right about the ordering — mounting first genuinely avoids the opposite
artefact. **The ordering is not the defect.**

## Where a fix has to attack

Two separate targets, because the two costs are independent:

- **Cost A** — stop reparenting the whole Library tree on every toggle. Keeping
  both roots realized (a `GtkStack` page swap) removes the 46–92 ms block.
  This shortens the transition but, on its own, leaves ~100 ms of wrong-size
  frame.
- **Cost B / the visible artefact** — change *what* is on screen during those
  ~100 ms rather than trying to shorten them. Deferring the Library remount
  until the resize configure has landed would make the one unavoidable
  wrong-size frame carry the compact card (which looks correct at 430×76)
  instead of a squeezed Library. Note this is the exact trade the current code
  deliberately made in the other direction, so it needs the pinned test
  rewritten, not just flipped.

Neither target is proven to fix the perception on its own; Cost B is the one
that owns the "small corner" symptom.

## Side finding (real, unrelated to the visuals)

`wire_full_geometry_tracking` subscribes to `"width"` and `"height"`. A
`GtkWindow` has no such properties — they are `default-width` /
`default-height`. GObject does not validate a notify detail at connect time, so
these two subscriptions **fail silently and never fire**. Confirmed: across the
whole run the only properties that ever notified were `maximized`,
`default-width` and `default-height`; the tracking closure fired 7 times, all
from `maximized`.

Not currently harmful, because `enter_compact(capture_full_geometry = true)`
captures the real geometry at toggle time before unmaximizing. But two of the
three subscriptions are dead code that reads as working, and `updated_full_geometry`
discards the value whenever `maximized` is true.

## Where a fix has to attack

The 46–92 ms remount, not the call order. Options worth weighing:
keeping both roots realized instead of reparenting on every toggle
(e.g. a `GtkStack` page swap), or deferring the Library remount until after the
resize configure lands so the wrong-size frame carries the compact card
instead of a squeezed Library.

## Not covered here

General app slowness is a separate investigation. One concrete result from it:
this branch is missing `#849` and `#850` from `origin/dev`, two landed
main-thread-cost fixes for deleting and tag saving.
