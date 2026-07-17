# Codex Handoff — FIL Filter Visibility

You are implementing `docs/superpowers/plans/2026-07-17-fil-filter-visibility.md`
in this repository. That plan is the single source of truth — this file only
replaces its Claude-specific process notes with a Codex execution protocol.

## Context

- Working directory / branch: this worktree, `feat/global-search-rework`
  (base commits `d18edc7` = ux-rules.md section K, `8649f9b` = the plan).
- The UX contract is `docs/ux-rules.md` section K (FIL-1a/1b, FIL-2..6, German).
  The plan implements FIL-1a and FIL-2..6; FIL-1b stays `[geplant]`.
- Also read `AGENTS.md` and `TESTING.md` before starting.

## Execution protocol

1. Execute the plan's tasks **strictly in order 1 → 10**. The "Parallel
   Execution Map" is for multi-agent setups; as a single agent, ignore the
   waves and go sequentially. Do not start a task before the previous one is
   committed.
2. Follow each task's checkbox steps as written: failing test first (red),
   then implementation (green). Run the exact commands given; the expected
   outcomes are stated per step.
3. **Gates before EVERY commit** (all must pass):
   - `cargo fmt --check`
   - `cargo clippy --locked --all-targets --workspace -- -D warnings`
   - `env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) cargo test --locked --workspace`
   - `scripts/check-ux-traceability.sh`
   - `scripts/check-architecture.sh`
4. Use the commit messages exactly as given in the plan. One commit per task
   (Task 2 has two: the mechanical split, then the feature). **Never push.**
   No attribution footers of any kind.
5. `docs/ux-rules.md` status flips (`[geplant] → [aktiv]`) happen ONLY in the
   tasks that say so (T4 → FIL-1a, T6 → FIL-4, T7 → FIL-5, T8 → FIL-3,
   T9 → FIL-6, T10 → FIL-2), inside that task's commit.

## Adaptation policy

- The plan's code snippets were verified against the live code at `d18edc7`,
  but gtk4-rs API details may differ (e.g. `StyleContextExt::lookup_color`
  return shape, `OnActivate` / `QueueViewModel` constructor shapes in the
  Task 4 display test). Adapt minimally where the compiler disagrees — but
  the **Interfaces blocks and rule-named test names are contracts**: do not
  rename `fil_*` tests, do not change produced signatures other tasks consume.
- If a fallback named in the plan applies (e.g. accent color unavailable →
  `<b>`-only highlighting), take it and note it in your final report.
- If you hit UI behavior no rule covers: do NOT decide locally. Add a
  `[geplant]` draft rule with the next free ID in the affected section of
  `docs/ux-rules.md`, marked `<!-- REVIEW: Regelvorschlag -->`, and continue.

## Test conventions (merge-gate critical)

- Rule-named tests (`fn fil_<nr><suffix>_…`) must be **display-free**: no
  `gtk4::init()`, no widget construction. They run un-ignored in
  `cargo test --workspace`. Put a `// UX FIL-…:` comment ABOVE the `#[test]`
  attribute (never between attribute and fn).
- Widget-level checks needing a display are NON-rule-named and carry
  `#[ignore = "requires a display; run via xvfb-run"]`.
- Never launch the app or any test window on the real desktop. Anything
  needing a display runs under `xvfb-run -a` with the TESTING.md environment
  (private D-Bus session, temp `XDG_DATA_HOME`/`XDG_CACHE_HOME`, forced X11,
  Wayland unset, `REPRISE_AUDIO_SINK=fakesink`). If Xvfb is unavailable in
  your environment, skip the display suite and the Task 10 acceptance
  walkthrough and mark them **NOT RUN** in your report — never substitute a
  visible window.

## Completion

After Task 10:

- Run `scripts/check-merge-readiness.sh --no-fetch` and include its output
  summary in your report. Do not merge; leave the branch for human review.
- Report per task: commit hash, tests added (names), gate results, and every
  deviation from the plan with a one-line reason.
