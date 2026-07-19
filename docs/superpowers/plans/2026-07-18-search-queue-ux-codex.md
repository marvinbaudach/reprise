# Codex Handoff — Search Strip + Queue Unification

You are implementing
`docs/superpowers/plans/2026-07-18-search-queue-ux-taskplan.md` in this
repository. The normative decisions are in
`docs/superpowers/plans/2026-07-18-search-queue-ux-beschluesse.md` (German —
binding, read it first).

## Context

- Working directory / branch: this worktree, `feat/search-and-new-releases`,
  at the merge commit `2783fa4`.
- Read `AGENTS.md`, `TESTING.md` and the tail of `.superpowers/sdd/progress.md`
  first.
- `docs/ux-rules.md` is the binding UX contract and this branch owns it. You
  **restate** SEARCH-2/3 in section Q and QUE-1/2/5 in section J, and add
  QUE-6. Rule IDs are append-only — restate the text of an existing ID, never
  renumber it.

## Execution protocol

1. Tasks strictly in order **A1 → A3, then B1 → B6**.
2. TDD per task. Where a task names a test function (`search_2_…`,
   `search_3_…`, `que_1_…`, `que_2_…`, `que_3_…`, `que_4_…`, `que_5_…`,
   `que_6_…`), use exactly that name — the traceability gate matches on it.
3. **Gates before EVERY commit**: `cargo fmt --check`;
   `cargo clippy --locked --all-targets --workspace -- -D warnings`;
   `env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) cargo test --locked --workspace`;
   `scripts/check-ux-traceability.sh`; `scripts/check-architecture.sh`.
4. **Display tests need one process each.** Run them through
   `xvfb-run -a scripts/check-display-tests.sh` — several GTK display tests in
   a single process fail on `gtk4::init()`, which looks like a real failure
   and is not. If the sandbox blocks Xvfb/D-Bus entirely, list them as
   "pending display verification" instead of faking a green.
5. Exact commit messages from the plan, one commit per task. **Never push.**
   No attribution footers.
6. After B6, append ONE compact entry to `.superpowers/sdd/progress.md` as a
   final `docs(progress): search strip and queue unification` commit. That
   path is gitignored but tracked — use `git add -f`.

## Things the audit already settled — do not re-derive

- **The search bar is already a second top bar** of the `adw::ToolbarView`
  (`library_chrome.rs:56-66`) and already pushes content. Do **not** re-parent
  it and do **not** introduce a `gtk::Overlay`. What is missing is styling
  (background + bottom hairline, explicitly, because `ToolbarStyle::Flat`
  suppresses them) and an `adw::Clamp` around the entry.
- **Do not replace `GtkSearchBar` with a hand-rolled revealer** to control the
  animation duration. Its private revealer already matches the standard token;
  the rule and the test assert that a reveal exists, not the milliseconds.
- **The queue keeps two surfaces.** The `ColumnView` (sidebar "Queue") stays
  the management surface with sections, DnD, right-click, Clear and
  StatusPage. The panel tab is a VIEW: sections + jump + remove, **no
  reorder, no DnD**. Do not move DnD into the panel and do not delete the
  ColumnView or its sidebar row.
- **Both surfaces read one model.** The section composition already exists in
  `ui/track_list/queue_sections.rs:48` (`compose`) — the panel must consume
  that, not grow a parallel implementation.
- **`show_lyrics()` (`now_playing.rs:336`) is the template** for the
  player-bar routing; `show_up_next()` does not exist yet.
- Every source file must end under 800 lines, UI orchestrators under 600
  (`scripts/check-architecture.sh`). Extract test modules to `#[path]`
  siblings when a file grows — the repo does this in several places.

## Adaptation policy

- Verified against the live code at `2783fa4`; gtk4-rs / libadwaita details
  may differ. Adapt the implementation, keep the behavior and the test names.
- If a task's premise turns out wrong, STOP, write `.codex-blocked.md` with
  the exact error, and end the run. Do not improvise a different design.
- UI copy is English; `docs/ux-rules.md` and the Beschluss-Ledger are German;
  commit messages are English.
