# Handover — one-centering-path-for-jump-and-clear, 19.08.2026 08:39

**Worktree:** `/home/marvin/Projects/reprise-one-centering-path-for-jump-and-clear`
**Branch:** `feature/one-centering-path-for-jump-and-clear` (clean tree)
**Plan:** `docs/plans/one-centering-path-for-jump-and-clear.md` (`phase: coded`)
**Companion TODO:** `docs/plans/queue-centering-ignores-section-headers.md` (`phase: todo`)

No PR exists. The branch carries four commits; a fifth, deliberately parked,
lives on `wip/one-centering-path-rebuild`.

---

## State right now: the measurement stopped the rebuild

Tasks 1, 2 and 5 are on the branch. Tasks 3, 4 and 6 are not — not because the
work ran out of time, but because Task 2 (the control arm) overturned the
plan's own premise. Details and the raw value sequences are in the plan under
"Stand 19.08.2026"; this file is the short form plus the mechanics.

| Commit | Task |
|---|---|
| `140fbf14b2` | Task 1 — every writer on the centering restore path is named |
| `464a85c1b9` | Task 2 — control arm records the value *sequence*, not the endpoint |
| `f41a50c89e` | Task 5 — SEARCH-16 names the intermediate state |
| `0fd5c893c0` | docs — what the measurement overturned |

### What the control arm showed

- The two-step exists for **clearing the search only**
  (`centered.scroll_to 3026.0` → `centered.changed.apply 2923.5`).
  **App start is already single-step** (`centered.initial.apply 4657.5`); its
  geometry is settled, the immediate `apply()` succeeds, the edge snap never
  runs. The plan expected two steps for both occasions; that held for one.
- The plan framed (a) our edge snap and (b) GTK's own allocation write as
  alternatives to be decided by measurement. They are not alternatives — the
  snap was **masking** (b). With the snap removed and Task 3 otherwise built as
  specified, GTK writes over our clean centering afterwards
  (`landed at 6460 instead of 2923.5`). The snap was not merely the visibility
  promise; it was the anchoring.
- Task 4 as planned does not cover this. A hold defends a *value*, and the
  centered value does not exist until the geometry settles; until then the hold
  defends whatever offset the cleared list happens to sit on. Measured as a
  four-step brawl (`gtk 6460 / hold 482 / hold 2923.5 /
  centered.reveal.instant 482 / hold 2923.5`) — right endpoint, worse path than
  the state the plan set out to fix.

### What a successor has to do differently

Make GTK's allocation write **land on the centered value** instead of
correcting it afterwards. The anchor path already does exactly this:
`reload_anchor_scroll::apply` seeds the range (`geometry.configure`) and sets
the hold target **before** the settled check. The centering path needs the same
treatment — a *predicted* target from the cached row height, not one derived
after the settle.

Two dead ends are closed and must not be re-tested:

- **GTK cannot scroll centered.** `gtk4::ScrollInfo` (0.11.4) exposes only
  `set_enable_horizontal` / `set_enable_vertical`, no alignment. The edge snap
  is all the API offers.
- **A hold armed at write time is too late** — GTK's write precedes it and is
  by then already a visible step.

## The parked rebuild

`wip/one-centering-path-rebuild` = **one commit on top of `f41a50c89e`**, i.e.
it branches *before* the docs commit `0fd5c893c0`. A diff against
`feature/…` therefore shows the plan documents losing ~150 lines; that is the
base offset, **not** a revert. Do not "restore" those lines.

It contains Tasks 3 and 4 fully executed: `RevealMotion`,
`ScrollGlide::jump_to`, the fallback snap moved behind the attempts, and the
removal of the helpers this made dead (`live_row_height`, `release_now`,
`centered_track_scroll_target` reduced to `#[cfg(test)]`).

**It is not landable:** the two SEARCH-16 endpoint tests are red. It is kept
because the shape of the rewrite is reusable — only the anchoring is missing.

## Open work

- Task 3, Task 4 (in a reworked form, see above), Task 6. Task 6 depends on a
  finished Task 3 (`RevealMotion::Instant`) and was never started.
- `queue-centering-ignores-section-headers` stays a separate TODO: both
  centering paths drop section header heights from the target value. Code-level
  finding only, not measured, and only the Queue is sectioned.

## Before resuming

The plan's line references are pinned to `origin/dev = 9ac0aa425d` (after
#568). `origin/dev` has since moved to `5dff480ed1` (#572). Rebase first, then
re-check the references — `git show origin/dev:<path>` rather than reading the
shared main checkout, which sits further back still.
