# Handover — strand B complete and accepted, not yet landed, 15.08.2026 08:30

**Status: B-1 … B-5 are implemented, reviewed, refactored and measured against
the real library. Eleven commits sit on the branch. Nothing has landed.**

| | |
|---|---|
| Branch | `feature/doctor-review-selection-and-refresh-b`, 11 commits on base `8188bcf29a` |
| Worktree | `/home/marvin/Projects/reprise-doctor-review-selection-and-refresh-b` — clean |
| Plan | `docs/plans/doctor-review-selection-and-refresh-b.md`, `phase: refactored` |
| Mother plan | `docs/plans/doctor-review-selection-and-refresh.md` |
| Strand A | merged `604677322e` (#478) · B-0 merged `f24366b269` (#487) |
| Wake lock | `doctor-b`, still held — release it after landing |

The previous version of this file described the B-0 state and is superseded.
Strand A's separate handover was deleted on request; its substance is in #478,
in `-a.md` (`phase: shipped`) and in memory.

---

## The result, in one table

Same album (*ALL IS BEAUTIFUL… BECAUSE WE'RE DOOMED* / We Came As Romans, 41
proposals, all selectable), same 330-row list, same build, one run per arm.

| | Control (`REPRISE_DOCTOR_FULL_REFRESH=1`) | Fix |
|---|---|---|
| median per album toggle | 248 ms | **13.6 ms** |
| twelve toggles, in order | 13 → 667 ms, **monotonic** | 8–36 ms, **flat** |
| `DOC-9b` warnings | 356 | 66, **none after a toggle** |

**The ratio is not the headline; the second row is.** The old path got slower with
every single click — 13, 87, 131, 198, 200, 230, 266, 334, 388, 468, 543, 667 ms —
and B-0 measured it reaching 4,600 ms over a longer session. The new path shows no
trend at all; its last three toggles are among its cheapest. That growth is what
made the page feel broken, and it is gone rather than reduced.

The user, driving the same session, reported scrolling noticeably faster and no
stutter on toggle. That matches the numbers and is recorded because scroll
slowness was one of the original symptoms.

Supporting measurements:

- **Churn** 386 → **24** store items per album toggle, both numbers measured, with
  the correctness half asserted in the same test.
- **Synthetic probe**, both paths in one process: `full` 5,264 µs vs `selection`
  **422 µs** at 192 rows.
- **Display gate**, run by this session unmodified with `DISPLAY_TEST_JOBS=4`:
  **708 passed, 0 failed, `matched no executing test binary` = 0.**

---

## What is on the branch

`0406a6fe34` B-1 cached aggregates · `528802b6b3` B-2 `review_snapshot.rs` ·
`3a92637471` B-3 `apply_selection` · `0663a72796` B-4 header registry and push ·
`20130b03c6` B-5 conflicts fingerprint · then three commits from review and
re-measurement, plus three documentation commits.

Files: `review_page.rs`, `review_snapshot.rs` (new), `review_header.rs`,
`review_conflicts.rs`, `review_refresh_tests.rs` (new),
`review_page_perf_tests.rs`, one import line in `review_page_tests.rs`, one
module declaration in `mod.rs`. `reprise-core` untouched.

---

## The review, and why two thirds of it was wrong

`rust-reviewer` raised three HIGH findings and voted to block. Each was handed to
a skeptic told to refute it. **Two fell:**

- *"The aggregates lost the category filter."* The filter is applied one layer
  deeper than the reviewer looked, in `album_from_seed`
  (`reprise-core/.../grouping.rs:111`), before grouping, and the category lives on
  the session rather than on a parameter. `doc_9b_the_snapshot_is_the_visible_row_set`
  pins it with a `Some(ReviewCategory::Year)` arm and passes.
- *"The header registry closes a strong `Rc<ReviewState>` cycle."* The chain is
  real, but `connect_teardown` is wired and GTK guarantees `teardown` is the last
  signal emitted for a `ListHeader` on every destruction path. (The page is also
  reused across navigation, so "compounds per visit" was wrong regardless.)

**One survived, smaller than filed:** `apply_selection` spliced before updating
the snapshot. A skeptic instrumented `bind_album_header` and measured it —
toggling an interior row causes 0 rebinds, a section's *first* row causes exactly
1, and that rebind did read the stale snapshot. But `push_selection` overwrote it
inside the same synchronous call, so no frame ever painted wrong. Fixed for
consistency, not as a user-facing bug.

Also fixed: the `row_id → position` map kept only a `debug_assert!`. It now keeps
the **first** position and logs a collision via `tracing::error!`, because in a
release build a duplicate id would have written one row's content into another
row's store slot — silent corruption, no panic.

**The first fix cost time and had to be redone.** Closing the `mem::take` window
by cloning the snapshot put a full deep copy on the hot path: measured 435 →
623 µs with the `full` arm unchanged as a control. It is now closed without a
copy (`affected_albums` computed after the assignment), back to 422 µs. **The
lesson: a correctness fix on a hot path needs its own before/after measurement,
and this branch had the probe to do it.**

---

## What is left

1. **Land it.** `scripts/land.sh <pr> --plan docs/plans/doctor-review-selection-and-refresh-b.md`.
   Do not wait for CI; rebase, push, merge in one go, then watch the dev run.
2. **Release the `doctor-b` wake lock.**
3. **The seven remaining §J cross-checks** in the mother plan, due after B lands.
4. **`review_page.rs` is at 795 of the 800-line cap.** The identified extraction —
   `refresh_conflicts` + `skip_all_conflicts` (~74 lines) into
   `review_conflicts.rs`, which already owns `ReviewConflictsSlot` and
   `ReviewConflicts` — was offered and deliberately declined this round. It lands
   the file near 715 and touches no index-critical code. **The next change to this
   file trips the gate without it.**
5. **66 `DOC-9b` warnings remain in the fix arm**, from a trigger that is not a
   selection toggle (they burst after the last toggle, during scrolling, all with
   `start=end=4294967295`). Outside V-4(d)'s question, worth its own small look.
6. **The review page still has no search** — 433 fixes across 122 albums, no
   `SearchEntry`, no `<primary>f`. Carried from the last handover; still worth its
   own plan.
7. `git rm --cached .pipeline-codex.md` — fourth handover in a row that says so.
8. `check-display-tests.sh` and `check-ux-traceability.sh` still appear in no
   GitHub workflow.

---

## Reusable: the acceptance harness

```
~/.cache/reprise-doctor-b0-harness/doctor-b0-run.sh        # launch + isolated profile
~/.cache/reprise-doctor-b0-harness/doctor-b0-medians.sh    # evaluation
~/.cache/reprise-doctor-b0-harness/ACCEPTANCE-strand-b.md  # the full procedure
```

Extended on 15.08. to take both arms: `DOCTOR_FULL_REFRESH=1` selects the control
and the log is named per arm (`doctor-b0-control.log` / `doctor-b0-fix.log`).

**Pick the test album from the database, not from the UI.**
`library_doctor_proposals` joined to `library_doctor_scan_tracks` and then to
`tracks` on path/mtime/size/device/inode reproduces the Ready/Stale split exactly
— re-derived on 15.08. and matching the UI to the row (scan 3: 825 rows, 433
Ready, 122 albums, 38 with nothing selectable).

---

## Traps found in this run

**The control arm is not a pure revert, and saying so matters.**
`REPRISE_DOCTOR_FULL_REFRESH=1` only forces `apply_selection` back into
`refresh()`; B-1's aggregate cache and B-5's conflicts fingerprint live *inside*
`refresh()` and cannot be switched off. Measured: `refresh_conflicts` 424 → 5 µs
and the aggregate 1,987 → 6 µs against B-0's pre-fix profile, while
`store.splice` and `grouped_rows_for` matched. Those two stages were 1 % of the
cost so the comparison holds — but it is "old splice plus B-1", not the old build.

**One process cannot run both arms.** The variable is read once where the
`ReviewState` literal is built, and the review page is reused across navigation
(`library_doctor/mod.rs`, `if existing.is_none()`). The plan's "one session, both
arms" is not achievable; it is one build, two runs.

**`agent-tmp-gc` deletes logs of running commands.** Two log files vanished from
this session's own scratchpad mid-run, one of them held open by a live process
(`/proc/<pid>/fd/1 → … (deleted)`). Combined with the fact that
`check-display-tests.sh` prints **everything at the end**, a 26-minute gate run
produced nothing recoverable. Write long-run logs to `~/.cache/`, never to the
scratchpad.

**A worktree gate and a Codex run cannot share a worktree.** Started both against
the same tree once; they collide on the cargo lock and the gate would have
measured a half-edited tree. Gate after the code settles, never beside it.

**The strand-B branch survived its own merge.** `gh pr merge --delete-branch`
removed it on GitHub but the local ref stayed at the pre-squash tip, and
`worktree.sh ensure` reuses an existing branch — which would have rebuilt B-0 on
top of itself. Check `git show-ref` before creating a worktree for a slug that has
landed once already.

**Codex reported the display gate as "708 passed, 0 failed" without the line that
decides it.** The number turned out to be right when this session re-ran the gate
unmodified. Check anyway: `matched no executing test binary` = 0 is the evidence,
the balance sheet is not.
