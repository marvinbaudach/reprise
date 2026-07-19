# Codex Handoff — UI-Politur Batch A

You are implementing
`docs/superpowers/plans/2026-07-19-ui-polish-taskplan.md`. The normative
decisions are in `docs/superpowers/plans/2026-07-18-ui-polish-beschluesse.md`.

## Context

- Working directory / branch: this worktree, `feat/sidebar-visual-improvements`,
  base `main@b0965905`.
- Read `AGENTS.md`, `TESTING.md`, and the tail of `.superpowers/sdd/progress.md`.
- This branch adds **section U** to `docs/ux-rules.md`. `feat/network-opt-in`
  adds section T in parallel and is not merged yet — **append at the end of the
  file, after whatever is already there.** Do not reorder, do not create T.
- Rule IDs are append-only. `ALB-1`/`ALB-2` and `NAV-5` already exist with
  different meanings than in older notes — do not reuse them.

## Execution protocol

1. Tasks strictly in order **T1 → T7**.
2. TDD per task; use the exact test names the plan gives — the traceability
   gate matches on them.
3. **Gates before EVERY commit** (listed in the plan). Run them; do not assume.
4. **Display tests need one process each** — run them through
   `xvfb-run -a scripts/check-display-tests.sh`. Several GTK display tests in
   one process fail on `gtk4::init()`; that looks like a real failure and is
   not. If the sandbox blocks Xvfb/D-Bus, list them as "pending display
   verification" rather than faking a green.
5. **Translate every new UI string in the same commit.** `po/de.po` must stay
   free of untranslated and fuzzy entries. Never mark icon glyphs with `N_!`.
6. Exact commit messages from the plan, one commit per task. **Never push.**
   No attribution footers.
7. After T7, append ONE compact entry to `.superpowers/sdd/progress.md` as a
   final `docs(progress): ui polish batch A` commit (path is gitignored but
   tracked — use `git add -f`).

## Traps the audit already mapped — do not rediscover them

- **`now_playing_tests.rs:90-91` asserts the bug.** It requires
  `background-color: #17191c` and forbids `@sidebar_bg_color`. T2 removes
  exactly that state, so the test must be rewritten in the same commit. Do not
  work around it by keeping the hardcoded colour.
- **`@sidebar_bg_color` is emitted but never consumed** (`theme.rs:193` defines
  it, no call site exists). You are adding the first consumers — check that the
  colour actually lands, do not assume the define is enough.
- **The status line is not styled, it is placed wrong.** `track_content.rs:10`
  returns a `gtk4::Overlay`; raising the text alpha alone cannot fix contrast
  because the background is whatever row scrolls underneath. Give it a surface
  first (T3), then the tone (T4). The two tasks are ordered for that reason.
- **Contrast tests must measure against the surface colour**, not the
  rendering, and are only meaningful after T3 created a surface.
- **The scroll fix is not "suppress the scroll".** Suppression already works —
  the log line `centering skipped: table activation` proves it. The jump comes
  from GTK's focus restore after `items_changed` recreates the focused row. Save
  and restore the adjustment; do not add another suppression flag.
- **Geometry and surfaces are verified by result, not by property** (STYLE-1,
  section S). A test asserting "we called set_background" is not acceptable
  where the plan asks for a measured outcome.
- Every source file must end under 800 lines, UI orchestrators under 600.

## Adaptation policy

- Verified against `b0965905`; gtk4-rs / libadwaita details may differ. Adapt
  the implementation, keep the behaviour and the test names.
- If a premise turns out wrong, STOP, write `.codex-blocked.md` with the exact
  error, and end the run. Do not improvise a different design.
- UI copy is English; `docs/ux-rules.md` and the ledger are German; commit
  messages are English.
