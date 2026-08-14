# Handover — queue-section-anchor (#444), 14.08.2026 02:47

**Worktree:** `/home/marvin/Projects/reprise-queue-section-anchor`
**Branch:** `feature/queue-section-anchor`
**Plan:** `docs/plans/queue-section-anchor.md` (`phase: refactored`; the plan file
itself is already on `origin/dev` via #459)

---

## State right now: a rebase is IN PROGRESS and unfinished

`git -C <worktree> status` is mid-`rebase origin/dev`, one unmerged file, Codex
running against it (pid 2102316 at the time of writing).

- Conflicted: `crates/reprise-gnome/src/ui/track_list/reload_anchor_scroll.rs`,
  **9 hunks**.
- Cause: dev's #463 ("display gate covers the whole ignored suite") added 260
  lines of probe instrumentation (`apply_probe`, `hold_probe`, `scroll_probe`,
  `set_hold_target`, `matches`, a `mod tests`) to exactly the functions this
  branch re-signatured from `row_height: f64` to `&ListLayout`.
- **Both sides are wanted.** The instruction given to Codex is to thread dev's
  probes through this branch's signatures, never to pick a side.
- The commit `docs: plan the sectioned Queue's header-aware scroll anchor` was
  deliberately `git rebase --skip`ped — dev already carries that plan file.

If the rebase has to be restarted: `git rebase --abort` returns to the verified
pre-rebase state, which is the state all the green display evidence below refers to.

### Three invariants the conflict resolution must not lose

Each was proven on a real display run; losing one silently reintroduces a bug.

1. `apply` builds the `ListLayout` **once** and passes that same layout to
   `scroll_to_anchor` / `prepaint_guard_position`. Two layouts → the guard row and
   the written scroll value describe different rows.
2. A failed geometry validation must **never** drop the anchor. Unsectioned lists
   skip validation; "no opinion" keeps the section geometry; only a proven
   rejection falls back to rows-only. Losing this puts the viewport back at the
   top of the list — the regression that was fixed tonight.
3. `shared.queue_sections` is a `RefCell`; section data must be copied out in its
   own statement before any GTK call.

---

## What is DONE and PROVEN

Production side, measured on real Xvfb runs (each test in its own process, so not
the known herd flakiness):

| Run | Result |
|---|---|
| `navback_anchor_display_tests` × 4 (unsectioned control group) | **all green** |
| `queue_anchor_names_the_row_at_the_viewport_top` (the #444 assertion) | **green** |
| `queue_section_header_display_tests`, `reveal_track_display_tests` | green |
| workspace suite, fmt, clippy `-D warnings`, arch gate | green |

The semantic test is the one the plan calls "the assertion carrying the defect":
the anchor names the row that was at the viewport top, and that row returns to the
same on-screen y. It is green without stubs.

Four reviewers (three rust-reviewer scopes + a plan-conformance pass) found no
CRITICAL or HIGH issues and verified both anchor halves moved onto the codec, the
RefCell discipline, the guard/write consistency, and 10/10 plan decisions —
re-running two mutations independently rather than trusting the report.

---

## What is OPEN

### 1. Finish the rebase, then RE-VERIFY on display  ← blocking the merge

All the green evidence above was obtained **before** the rebase. After the
conflict resolution, `reload_anchor_scroll.rs` has different content than what was
measured. The display pass must run a fourth time before merging, or what lands on
dev is not what was verified. Minimum set:

```bash
cd /home/marvin/Projects/reprise-queue-section-anchor
# the driver used all night:
bash /tmp/.../scratchpad/display-pass3.sh     # 4× navback + both Queue tests
```
Recipe per run (also in the script): own XDG roots, `dbus-run-session`,
`xvfb-run -a`, `GDK_BACKEND=x11 WAYLAND_DISPLAY= GSK_RENDERER=cairo
REPRISE_AUDIO_SINK=fakesink`, judge on `^test result:` **and its count**.
Afterwards `xvfb-orphan-gc --apply`.

### 2. `q-journey` is still RED — and the reason is a real finding

`nav_back_to_a_large_sectioned_queue_never_visits_the_top` fails on its own
oracle, not on the anchor:

```
bands(rows=7 headers=2 row_samples=[34.0,34.0,34.0,34.0] header_samples=[20.0, 34.0])
```

**The two section headers are 20 px and 34 px — not uniform, and neither is the
assumed 36 px.** Together 54 px, which exactly explains the earlier measurement
`upper = 77438 = 2276×34 + 54`. So:

- `ListLayout` carries **one** `section_header_height` for all sections. Reality
  has two different ones. That is a wrong model assumption, not a wrong constant.
- `validate` was **right** to reject: the model's 72 px genuinely disagrees with
  reality's 54 px. (An earlier hypothesis of mine — "upper had not settled" — was
  wrong; correct that if you read it anywhere.)
- The anchor nonetheless works (semantic test green), partly because capture and
  restore share the same 18 px error and it cancels — the same cancellation the
  plan set out to end, now smaller.
- This test was **already red on `dev`** before this branch (it is in the plan's
  own known-red list), so leaving it red is not a new failure.

The plan declared header-height provenance out of scope ("measuring it properly is
its own strand"). Tonight produced what that strand needs: the first real
measurement, and the fact that header heights are **not uniform**. That is the
follow-up worth filing.

### 3. #463 widened the display gate — check the consequence

Before #463, the whole ignored suite did not run in the gate; now it does. A red
`q-journey` that used to be invisible can now turn the dev gate red after the
merge. Check how #465 ("Bring the dev gate back to green") treats this test before
landing, and decide whether to exclude it, fix the oracle, or accept a red gate.

### 4. Review findings deliberately NOT applied

Chosen consciously, all low severity:

- duplicate entries in `section_starts` are double-counted by `headers_above`
  (unreachable today, undocumented invariant)
- `content_height`/`max_scroll` return `Option` that the constructor makes
  unreachable
- `rendered_queue_headers` (pre-existing helper) does not filter zero-height
  widgets, unlike the two new helpers
- `uniform()`'s 0.5 px tolerance — note this is what rejects the 20 vs 34 headers,
  so it interacts with item 2

Also unverified: the "rows-only `scroll_target`" mutation (16 passed / 3 failed)
was never independently reproduced — plausible, not confirmed.

### 5. Follow-up already filed

Issue **#460** — `scroll_center::centered_scroll_value_with_height` and
`track_list_reload::pending_reveal_anchor` keep the rows-only model for centring
(plan Decision 10).

---

## Operational notes from this run

- Wake lock `queue-section-anchor` is **still held**; release it when the strand
  ends: `wake-lock release queue-section-anchor`.
- Codex runs must go through `heavy-run medium`, not `heavy` — `heavy` needs 4 of
  6 slots and starves behind other sessions' runs.
- `heavy-run` swallows `codex-run.sh`'s stderr, so its launcher log stays 0 bytes.
  That is not a hung run; watch worktree file mtimes instead.
- `.pipeline-codex.md` is a tracked file and carries a stale copy from earlier
  runs — "the file exists" is not a finish signal; compare its checksum.
- `find` on this host is `bfs` and silently fails on relative `-newermt` (exit 0,
  no output). Compare `%T@` epochs instead.
