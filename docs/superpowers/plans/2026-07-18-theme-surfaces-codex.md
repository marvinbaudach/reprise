# Codex Handoff — Theme Surface Hierarchy + Petrol Fallback

You are implementing
`docs/superpowers/plans/2026-07-18-theme-surfaces-taskplan.md` in this
repository. That plan is the single source of truth. The decisions behind it
(Beschlüsse 7/8) live in the ledger
`docs/superpowers/plans/2026-07-18-npp-beschluesse.md` on the sibling branch
`feat/now-playing-panel` — deliberately not on this branch; the plan restates
everything you need.

## Context

- Working directory / branch: this worktree, `feat/theme-surface-hierarchy`
  (base `main@fec994c`).
- Read `AGENTS.md`, `TESTING.md`, and `.superpowers/sdd/progress.md` (tail)
  before starting.
- A sibling branch `feat/now-playing-panel` works in parallel. The
  file-ownership table in the taskplan is **binding**: you may only touch
  `ui/style/theme.rs`, `ui/style/cover_accent.rs`,
  `ui/window/library_chrome.rs`, `ui/window/library_shell.rs`, their test
  files, `RELEASING.md` (own section), and the plan itself. Everything else —
  especially `ui/info_panel/`/`ui/now_playing/`, `ui/lyrics/`,
  `ui/sidebar/sidebar_presentation.rs`, `ui/style/mod.rs`,
  `ui/style/tokens.rs`, `docs/ux-rules.md` — is off-limits. Do not edit
  `AGENTS.md`.

## Execution protocol

1. Execute the plan's tasks **strictly in order S1 → S3**; one commit per
   task, exact commit messages from the plan.
2. TDD as written: failing test first where the plan names one (palette
   hierarchy ordering, `player_accent == accent`, CSS parse/class tests).
3. **Gates before EVERY commit** (all must pass):
   - `cargo fmt --check`
   - `cargo clippy --locked --all-targets --workspace -- -D warnings`
   - `env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) cargo test --locked --workspace`
   - `scripts/check-ux-traceability.sh`
   - `scripts/check-architecture.sh`
   Display-marked tests (`#[ignore]`) of the touched area via `xvfb-run -a`
   **if the sandbox allows it**; otherwise list them as "pending display
   verification" in your final ledger entry — never fake a green.
4. Headless verification only: never open a window on the user's desktop.
   Use Xvfb per `TESTING.md`. Screenshot-based acceptance (hairline visible,
   hierarchy reads) may be listed as pending if tooling is unavailable.
5. **Never push.** No attribution footers of any kind.
6. After S3, append ONE compact entry to `.superpowers/sdd/progress.md`
   (stage summary: tasks, commits, verification counts, anything pending) as
   a final `docs(progress): theme surface hierarchy stage` commit.

## Adaptation policy

- The palette values in the plan are binding. Selector details for the
  hairlines (libadwaita node names) may differ from assumptions — adapt the
  selector, keep the visual contract (1 px white 6 % at sidebar|content and
  under the headerbar, scoped so the right-hand panel is untouched).
- If a premise is wrong (e.g. a named color is consumed somewhere the plan
  forbids you to edit), STOP, write the blocker to `.codex-blocked.md`, and
  end the run instead of improvising.
- Commit messages English; doc comments follow the file's existing language.
