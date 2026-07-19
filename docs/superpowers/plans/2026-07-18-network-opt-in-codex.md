# Codex Handoff — Network Features Opt-In

You are implementing
`docs/superpowers/plans/2026-07-18-network-opt-in-taskplan.md`. The normative
decisions are in `docs/superpowers/plans/2026-07-18-network-opt-in-beschluesse.md`
— **read its "Korrekturen nach dem Code-Audit" section first; it overrides the
text above it.**

## Context

- Working directory / branch: this worktree, `feat/network-opt-in`, base
  `main@c2569e8a`.
- Read `AGENTS.md`, `TESTING.md`, and the tail of `.superpowers/sdd/progress.md`.
- This branch owns `docs/ux-rules.md` and adds **section T** (S is taken by
  STYLE-1 as of today). Rule IDs are append-only.

## Execution protocol

1. Tasks strictly in order **T1 → T7**.
2. TDD per task; use the exact test names the plan gives (`net_1_…`, `net_2_…`,
   `lyr_2_…`, `lyr_3_…`, `discover_1_…`, `discover_2_…`) — the traceability
   gate matches on them.
3. **Gates before EVERY commit**: `cargo fmt --check`;
   `cargo clippy --locked --all-targets --workspace -- -D warnings`;
   `env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) cargo test --locked --workspace`;
   `scripts/check-ux-traceability.sh`; `scripts/check-architecture.sh`.
4. **Display tests need one process each** — run them through
   `xvfb-run -a scripts/check-display-tests.sh`. Several GTK display tests in
   one process fail on `gtk4::init()`; that looks like a real failure and is
   not. If the sandbox blocks Xvfb/D-Bus, list them as "pending display
   verification" rather than faking a green.
5. **Translate every new UI string in the same commit.** `po/de.po` must stay
   free of untranslated and fuzzy entries or the release check fails. Never mark
   icon glyphs with `N_!` — a translator cannot act on `🗑`.
6. Exact commit messages from the plan, one commit per task. **Never push.**
   No attribution footers.
7. After T7, append ONE compact entry to `.superpowers/sdd/progress.md` as a
   final `docs(progress): network opt-in stage` commit (path is gitignored but
   tracked — use `git add -f`).

## Traps the audit already mapped — do not rediscover them

- **Cover download has TWO paths.** `CoverLoader::load_target` already checks a
  flag, but `CoverDownloadBatch::start` sends straight to the worker and never
  consults it. Gating only the loader leaves the batch downloading.
- **`CoverLoader` copies its flag once at construction.** A plain `bool` will
  not react to a settings change — share an `Rc<Cell<bool>>`, as
  `ArtistNewsRuntime` does.
- **Portraits serve cache hits inside the core fetch function.** Gating
  dispatch would also hide already-cached portraits, which NET-2 forbids. Add a
  cache-only entry point in core and let the gate choose which one to call.
- **`rusqlite::Connection` is `!Sync`** and lives behind one `Rc<RefCell<…>>` on
  the main thread. Every gate belongs *before* worker dispatch, on the main
  thread. Do not try to hand a `Connection` to a worker.
- **The migration cannot be pure SQL.** Grandfathering probes the filesystem, so
  v13 calls a Rust function inside the same transaction. Inject both cache
  directories as parameters — otherwise tests depend on the developer's real
  `~/.cache` and become machine-dependent. `.notfound` markers are *not* usage.
- **`module.artist_news.enabled` is orphaned** since New Releases replaced it.
  The migration carries it over; that is also NET-2's evidence for NR.
- **The lyrics status page is not an `adw::StatusPage`** — it is a hand-rolled
  box whose single button is bound to retry. Add a fourth stack page instead of
  overloading it.
- **`PreferencesContext` is constructed after `LyricsView`.** Follow the
  `device_view.set_on_settings` pattern: a setter on the view, wired in
  `window.rs` where the context exists.
- **The Plugins page rebuilds its rows on every `open()`**, so a deep-link
  highlight needs row handles captured at construction time.
- Every source file must end under 800 lines, UI orchestrators under 600.

## Adaptation policy

- Verified against `c2569e8a`; gtk4-rs / libadwaita details may differ. Adapt
  the implementation, keep the behaviour and the test names.
- If a premise turns out wrong, STOP, write `.codex-blocked.md` with the exact
  error, and end the run. Do not improvise a different design.
- UI copy is English; `docs/ux-rules.md` and the ledger are German; commit
  messages are English.
