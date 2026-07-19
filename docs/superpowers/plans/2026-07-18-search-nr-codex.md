# Codex Handoff — Search Bar + New Releases

You are implementing
`docs/superpowers/plans/2026-07-18-search-nr-taskplan.md` in this repository.
That plan is the single source of truth for the tasks; the normative decisions
behind it live in `docs/superpowers/plans/2026-07-18-search-nr-beschluesse.md`
(German — binding, read it first).

## Context

- Working directory / branch: this worktree, `feat/search-and-new-releases`
  (base `main@e0493d0`).
- Read `AGENTS.md`, `TESTING.md`, and the tail of `.superpowers/sdd/progress.md`
  before starting. Tasks marked complete in the ledger are done.
- `docs/ux-rules.md` is the binding UX contract and **this branch owns it** —
  you add sections Q (SEARCH-1..5) and R (NR-1..7 + DISCOVER-1). Rule IDs are
  append-only: FIL-4 gets `[ersetzt durch SEARCH-3]`, it is never rewritten.

## Execution protocol

1. Execute the tasks **strictly in order A1 → A5, then B1 → B8**. Part A must
   be complete and committed before Part B starts — both rework the same
   headerbar, and B assumes the space A frees up.
2. TDD per task: failing test first (red), then implementation (green). Where
   a task names test functions (`search_1_…`, `search_2_…`, `search_3_…`,
   `search_4_…`, `nav_6_…`, `nr_1_…`, `nr_2_…`, `nr_3_…`, `discover_1_…`), use
   exactly those names — the traceability gate matches on them.
3. **Gates before EVERY commit** (all must pass):
   - `cargo fmt --check`
   - `cargo clippy --locked --all-targets --workspace -- -D warnings`
   - `env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) cargo test --locked --workspace`
   - `scripts/check-ux-traceability.sh`
   - `scripts/check-architecture.sh`
   Display-marked tests (`#[ignore]`) of the touched area: run via
   `xvfb-run -a` if the sandbox allows it; if not, do not fake them — list
   them as "pending display verification" in your final ledger entry.
4. Use the commit messages exactly as given in the plan, one commit per task.
   **Never push.** No attribution footers of any kind.
5. Status flips `[geplant] → [aktiv]` happen in `docs/ux-rules.md`, only in the
   task that says so.
6. After B8, append ONE compact entry to `.superpowers/sdd/progress.md` as a
   final `docs(progress): search bar and new releases stage` commit. Note: that
   path is in `.gitignore` but the file is tracked — use `git add -f`.

## Things the audit already established — do not re-derive or duplicate

- **The MusicBrainz rate limit already exists** and is process-wide:
  `reprise-core/src/musicbrainz.rs` (`MIN_REQUEST_INTERVAL`, a
  `static LAST_REQUEST: Mutex<Option<Instant>>`, `respect_rate_limit()`).
  Route every new request through that module. Do NOT build a second limiter.
- **The User-Agent and the fixture seam are there too**
  (`REPRISE_MUSICBRAINZ_FIXTURE_DIR`). The fixture router currently understands
  only two URL shapes — extend it for the new endpoint rather than bypassing it.
- **`artist_news.rs` keeps its parsing, window and filter logic**; only its
  JSON file cache is replaced by the new table (Beschluss 1).
- **There is no `GtkSearchBar` anywhere in the repo** — no precedent to copy.
- **Escape currently hangs off the entry's `stop-search` signal**, which only
  fires while the entry has focus (`shortcuts.rs:214`). That is exactly why it
  has to move to the SearchBar.
- **Cover download and artist portraits are currently ungated** — no
  `is_enabled` check exists in their call paths. B7 adds it.
- Every source file must end **under 800 lines**, UI orchestrators under 600
  (`scripts/check-architecture.sh` enforces both). Extract test modules to
  `#[path]` siblings when a file grows — the repo does this in several places.

## Adaptation policy

- The plan was verified against the live code at `e0493d0`, but gtk4-rs /
  libadwaita API details may differ. Adapt the implementation, keep the
  specified behavior and the test names.
- Scope drift is not allowed. If a task's premise turns out wrong (an API does
  not exist in any form, a migration cannot be expressed as specified), STOP,
  write the blocker to `.codex-blocked.md` in the worktree root with the exact
  error, and end the run instead of improvising a different design.
- UI copy is English (existing `strings*` modules); `docs/ux-rules.md` and the
  Beschluss-Ledger are German; commit messages are English.
