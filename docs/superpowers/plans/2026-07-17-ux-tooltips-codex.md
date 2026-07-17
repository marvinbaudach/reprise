# Codex Handoff — UX Tooltips (Section L)

You are implementing `docs/superpowers/plans/2026-07-17-ux-tooltips-taskplan.md`
in this repository. That plan is the single source of truth — this file only
adds the Codex execution protocol.

## Context

- Working directory / branch: this worktree
  (`.worktrees/ux-rules-tooltips`), branch `feat/ux-rules-tooltips`,
  based on `main` at `4a97698`.
- The UX contract is `docs/ux-rules.md` (German). Task 1 adds section L
  (TIP-1a/1b, TIP-2a/2b, TIP-3/4/5) with the exact wording given in the
  plan. Read the document's process rules (top of file) first — status
  flips, ID discipline, and test naming are binding.
- Also read `AGENTS.md` and `TESTING.md` before starting.

## Execution protocol

1. Execute the plan's tasks **strictly in order 0 → 9**. The dependency map
   is for multi-agent setups; as a single agent go sequentially. Do not
   start a task before the previous one is committed.
2. Follow each task's checkbox steps as written: failing test first (red),
   then implementation (green). Expected red/green outcomes are stated per
   step.
3. **Gates before EVERY commit** (all must pass):
   - `cargo fmt --check`
   - `cargo clippy --locked --all-targets --workspace -- -D warnings`
   - `env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) cargo test --locked --workspace`
   - `scripts/check-ux-traceability.sh`
   - `scripts/check-architecture.sh`
   - When a task touched `po/`: the msgfmt/msgcmp/msgattrib commands from
     the plan's Global Constraints.
4. Display tests run headless only — never on a live desktop. Use
   `scripts/check-display-tests.sh`, or for a single test the
   dbus-run-session/xvfb-run pattern from that script. New display tests
   MUST carry `#[ignore = "requires a display; run via xvfb-run"]` so the
   runner picks them up.
5. Use the commit messages exactly as given. One commit per task.
   **Never push.** No attribution footers of any kind.
6. `docs/ux-rules.md` status flips happen ONLY where the plan says so
   (T3 → TIP-1a, T5 → TIP-2a, T8 → TIP-3/4/5), inside that task's commit.
   TIP-1b and TIP-2b stay `[geplant]` — do not flip them.

## Hard boundaries

- **Never modify** `crates/reprise-gnome/src/ui/tag_edit/**` or
  `crates/reprise-gnome/src/ui/browse/**` — they are owned by parallel
  branches (`feat/tag-editor-rework`, `feat/global-search-rework`).
  Deviations you notice there go into the Task 9 handoff list, nothing
  else.
- Do not rename or retext the existing `PLAY`/`PAUSE`/`PREVIOUS`/`NEXT`
  string constants — they serve menu labels and the locked tag editor. The
  plan introduces separate `TOOLTIP_*` constants instead.
- Every new or changed user-visible string goes through an `N_!` catalog
  and gets its `de.po` entry in the same commit (the release gate enforces
  100 % coverage). Align German phrasing with existing `de.po` entries.

## Adaptation policy

- Code snippets were verified against the live code at `4a97698`, but line
  numbers drift and gtk4-rs borrow/clone details may differ. Adapt
  minimally where the compiler disagrees — the **Interfaces blocks, rule
  wording in section L, and rule-named test names are contracts**: do not
  rename `tip_1a_*`/`tip_2a_*` tests, do not change the exact TIP rule
  texts, do not alter produced signatures other tasks consume.
- Test-fixture construction (ScanControls, album card, mini layout,
  DeviceView struct) follows whatever the file's existing tests-mod does;
  create one after the `player_bar_layout.rs::tests` pattern if none
  exists.
- If you hit UI behavior no rule covers: do NOT decide locally. Add a
  `[geplant]` draft rule with the next free ID in the affected section of
  `docs/ux-rules.md`, marked `<!-- REVIEW: Regelvorschlag -->`, and
  continue.

## Final report

Summarize per task: commit hash, red→green evidence (test names), and the
full Task 9 handoff list (tag-editor items, global-search items,
out-of-scope findings). List any adaptation you made under the policy
above.
