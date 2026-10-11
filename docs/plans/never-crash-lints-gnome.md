---
slug: never-crash-lints-gnome
worktree: /home/marvin/Projects/reprise-never-crash-lints-gnome
branch: feature/never-crash-lints-gnome
phase: planned
codex_session:
created: 2026-10-11
---
# Never-crash lints for reprise-gnome

The follow-up that `docs/plans/never-crash-lints-and-crate-layers.md` (landed as #1268)
left open: `reprise-gnome` is the last crate whose production code may still call
`unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!` or `unreachable!`, and it is
the one where a panic hurts most — a panic inside a GTK signal handler unwinds into C
and aborts the whole app. This plan removes its 93 production hits, puts the six-lint
block on its crate root and flips its row in `scripts/check-crate-layers.sh` from
`pending` to `deny`.

The work starts from `origin/dev` at `7163bdd1da`.

## Inherited decisions (settled in the predecessor's grill, not reopened here)

- Mechanism: the same crate-root block, verbatim, with `not(test)` — test builds exempt.
- Integration tests and `examples/` are separate crates and stay exempt (gnome: 41
  example hits in 6 files, `tests/gnome_conformance.rs`).
- Ratchet: an exact per-crate suppression budget in `panic_lint_budgets`; `#[allow]` of
  the six lints is never accepted; a stale `#[expect]` fails through
  `unfulfilled_lint_expectations`.
- Non-goals stay non-goals: `clippy::indexing_slicing`, `assert!`, `RefCell` borrow
  panics. Reviewers should not raise them against this plan.

## Decisions (settled in the grill, 2026-10-11)

- **D1 · `build.rs` is exempt.** Its 4 hits (`OUT_DIR`, `glib-compile-resources`, the
  brand palette) are a build script's error channel: a panic there fails the build
  loudly, which is exactly what it should do. The gate already scans only lib/bin `src`
  dirs, so nothing changes in the script; AGENTS.md names the exemption.
- **D2 · An impossible state goes through one crate-local macro: log, assert in debug,
  degrade.** A new `invariant_broken!("…")` (one small module, e.g.
  `crates/reprise-gnome/src/ui/invariant.rs`) expands to a `tracing::error!` that names the
  broken invariant plus an assertion that fires only in debug builds. Every removed panic
  whose condition cannot happen in a correct program becomes
  `let … else { invariant_broken!("…"); return …; }` (or the match-arm equivalent):
  - Release builds (the Flatpak) never crash; they log and take the neutral path.
  - Tests and `cargo run` still trip on a broken invariant — the `G_DEBUG=fatal-criticals`
    idea, in `tracing` terms.
  - The crate logs only through `tracing` (179 `error!`, zero GLib logging), so no
    `g_critical!`.
  - The macro must not trip the six lints or `clippy::assertions_on_constants` under
    `-D warnings`; its own unit test proves the debug assertion fires
    (`#[should_panic]`) and that the message reaches the log.
  - No silent default anywhere — the predecessor's review flagged silent fallbacks as
    its most common MEDIUM finding. Sites that are not invariant breaks (X11 on
    Wayland, the `main.rs` exit path) use plain `tracing` and are named in Task 1.
- **D3 · Constants are built infallibly, not suppressed.** The 8 `RGBA::parse(<const>)`
  sites and the 2 `RowHeight::new(<const>)` sites become compile-time-checked values
  (a `const fn` hex decoder feeding `gdk::RGBA::new`, a `const` `RowHeight` built through a
  `const fn` constructor or a `const` assertion). Target budget for gnome: **0**.
- **D4 · Variant mismatches are removed by types where the type is local.** The 8
  `unreachable!` sites in `ui/playback/external_media{,_fields,_mpris,_session}.rs`
  exist because podcast-only and radio-only code receives the whole `ExternalMedia`
  enum. Where a function only ever handles one variant, it takes that variant's data
  instead, so the mismatch cannot be constructed. Where that would ripple beyond the
  playback module, D2 applies instead.
- **D5 · `pending` is deleted from the layer gate.** After the flip no crate is pending.
  Keeping the mechanism means keeping a code path, a test seam and four test cases for
  a state nothing uses; AGENTS.md's "no backwards compatibility" rule says delete the
  old shape. A future crate starts under `deny` from day one.
- **D6 · GP-4 is reworded and activated in the same change.** `docs/ux-rules.md:7972`
  (GP-4, `[planned]`) currently says "either `expect()` with context or the error is
  propagated" — the new lints forbid that `expect()`. New wording: production code of
  the GTK frontend does not call `unwrap`, `expect`, `panic!`, `todo!`,
  `unimplemented!` or `unreachable!`; clippy enforces it from the crate root. Its
  `check-gnome-idioms.sh` count (2 today, both in `compact_mode_controls.rs`) reaches 0,
  so the status moves to `[active]` in the same commit. `[active]` requires a green
  rule-named `#[test]` (`scripts/check-ux-traceability.sh`, part of the merge gate):
  a GTK-free unit test named after GP-4 per that script's naming convention, which reads
  `src/main.rs` through `include_str!` and asserts the six-lint `cfg_attr(not(test),
  deny(…))` block is present with exactly the six lints.
- **Non-goal: smoke hooks in the release binary.** 8 `*_smoke.rs` modules (2004 lines)
  and 48 `REPRISE_SMOKE_*` variables ship ungated. Only one of the 93 hits sits in smoke
  code. Gating them behind a feature is its own plan.

## Measured starting point

Clippy on `origin/dev` `7163bdd1da`, `-p reprise-gnome --bins`, six lints at warn,
deduped by file/line/lint: **97** = 93 in `src` + 4 in `build.rs`. `--all-features`
adds nothing. No existing `#[allow]`/`#[expect]` of the six lints anywhere in the crate.

| Lint | expect | unreachable | panic | unwrap | todo/unimplemented |
|---|---|---|---|---|---|
| hits | 65 | 21 | 9 | 2 | 0 |

The 93 `src` hits by kind:

- **(a) RefCell/OnceCell init-order, 9** — `concerts_model.rs:43`, `radio_model.rs:43`,
  `releases_model.rs:45` (GObject row set at construction), `startup_quiet.rs:34`,
  `window/content_stack.rs:72,82`, `now_playing/cover_scrim.rs:72`,
  `window/wiring/mod.rs:24,30`.
- **(b) GTK downcasts / list-store invariants, 11** — `compact/compact_mode_controls.rs:79,80`
  (the only two `unwrap`s, X11 surface/display), the three model `downcast().expect`s
  (`concerts_model.rs:69`, `radio_model.rs:84`, `releases_model.rs:74`),
  `sidebar/sidebar.rs:293`, `stats/stats_band_card.rs:198`, `podcasts/add_dialog_rows.rs:59`,
  `table_columns/registry.rs:67`, `list_store_delta.rs:23`, `motion_reveal.rs:62`.
- **(c) Widget-tree lookups, 3** — `compact/minimal_view.rs:179`,
  `preferences/preferences_search.rs:67,515`.
- **(d) `unreachable!`, 21** — the 8 `external_media*` sites (D4),
  `player_event_handling.rs:330`, `motion_reveal.rs:65,72` (unknown GObject property
  name), `concerts_empty_state.rs:68`, `releases_view.rs:416`,
  `device_sync_effects.rs:105`, `end_of_results.rs:57`, `review_model.rs:56`,
  `review_search.rs:98`, `podcasts_episode_files.rs:39`, `podcasts_row_state.rs:41,94`,
  `podcasts_view_copy.rs:91`, `radio_view.rs:534`, `strings_stats.rs:169`.
- **(e) `main.rs`, 2** — `:34` gresource registration, `:43` `Display::default()`.
- **(f) smoke, 1** — `preferences/preferences.rs:360`.
- **(g) other, 46** — 10 `u32::try_from(..).expect` (`review_page.rs`, `review_snapshot.rs`,
  `list_store_delta.rs`), 5 cached-state lookups (`review_snapshot.rs:148–180`),
  10 constants (D3), 16 option/flow invariants ("checked above", `Weak::upgrade`),
  1 thread spawn (`lyrics/lyrics_worker.rs:47`), 4 unknown-id/binding panics
  (`preference_plugins.rs:128,383,639`, `table_columns/registry.rs:76`).

Full list with source lines: re-measure with the command above; line numbers drift.

**Files at the 800-line cap with hits** — `list_geometry.rs` 799, `table_columns/editor.rs`
794, `library_doctor/review_page.rs` 789, `table_columns/registry.rs` 787,
`radio/radio_view.rs` 779, `device_sync/device_sync_effects.rs` 773. A `let … else`
with a log line adds 2–4 lines per site; these files need a cohesive sibling extraction,
not trimmed docs.

## Tasks

The file names are a starting point, not a fence. Stop only when the contract here is
itself wrong.

### Task 1 — remove the 93 production hits

First add the D2 macro and its test. Then, by kind, applying D2–D4 — wherever this list
says `error!`, it means `invariant_broken!`, except where a bullet names plain `tracing`:

- (a) Init-order cells: `let … else { tracing::error!; return }` at each read. For the
  three GObject model rows, the getter returns `Option` (or the row is set through a
  constructor that makes the empty state unrepresentable — choose per site, smallest
  change wins).
- (b) Downcasts: `let Some(x) = … .and_downcast::<T>() else { error!; return }`.
  X11 (`compact_mode_controls.rs`): a non-X11 surface means the X11-only path does not
  apply — return without the X11 tweak, log at `debug!` (not an invariant break; Wayland
  is the normal case).
- (c) Lookups: degrade to "nothing to select / nothing to focus" with `error!`.
- (d) `unreachable!`: D4 for `external_media*`; GObject `property()`/`set_property()`
  unknown-name arms log `error!` and return the property's default `Value`; the
  remaining match arms either collapse by narrowing the matched type or log + take the
  neutral branch.
- (e) `main.rs` (plain `tracing`, not the macro — these are exit paths, not degrades):
  a failed gresource registration logs `tracing::error!` and `main` returns
  `glib::ExitCode::FAILURE` before building the app; a missing default display skips the
  icon-path registration with `tracing::error!` (GTK cannot open a window then anyway).
  If either happens before the `tracing` subscriber is initialised, write the message to
  stderr as well, or it is lost.
- (f) The smoke lookup logs and ends the smoke step as failed through the existing smoke
  failure path, not a panic.
- (g) `u32::try_from` → saturate at `u32::MAX` (positions/counts in a list store);
  cached-state lookups → `error!` + skip the row; constants → D3; thread spawn →
  the lyrics worker reports "unavailable" through its existing error path;
  unknown-id panics → typed id where the ids are a closed local set, else `error!` +
  skip the capability row.

### Task 2 — the policy attribute

Add the predecessor's block verbatim to `crates/reprise-gnome/src/main.rs` (its only
lib/bin root; it has no inner attributes today), after any `//!` doc and before the
first `mod`/`use`.

### Task 3 — the layer gate (D5)

In `scripts/check-crate-layers.sh`:

- the `reprise-gnome` row: `pending` → `deny`;
- delete `pending_allowed`, the `ARCH_LINT_PENDING_CRATES` seam and its header comment,
  the `pending` branch of the table-parse loop, the `pending` skip in the policy loop,
  and `pending` from the field-5 regex, message and comment;
- `panic_lint_budgets` gains no gnome entry if the budget is 0 (D3).

In `scripts/tests/crate-layers.sh`: drop the `fixture-ui` pending row (give the fixture
a root attribute through `make_fixture`), `ARCH_LINT_PENDING_CRATES`, the cases at
lines 181–186 and 358–362, and the 401–402 assertion; add one case proving a
`pending` row is now rejected as an unknown policy; assert the real run prints
`reprise-gnome: panic policy deny`. Update `scripts/tests/qa-linters.sh` if it pins any
of the removed text.

### Task 4 — rules and docs

- `docs/ux-rules.md` GP-4: reword and activate (D6) in the same commit as its
  rule-named test, following the status rules at the top of the document.
- `AGENTS.md`: the never-crash bullet names all nine crates and drops "pending"; the
  exemptions line adds `build.rs`.
- No file under `crates/` or `scripts/` cites this plan's path.

## Verification

### What the implementer runs

Codex is out of quota until 2026-10-14; a Claude worker implements. This list replaces
the generic AGENTS.md gate list for that run:

- `cargo fmt --check`
- `heavy-run heavy -- cargo clippy --locked --all-targets --workspace -- -D warnings`
- `heavy-run heavy -- cargo test --locked --workspace --exclude reprise-gnome`
- Not the unfiltered gnome suite: it contains the `settle()` family, which blocks under
  load. The implementer runs only the gnome unit tests of the modules it touched, by
  module-path filter, and leaves the full suite to the orchestrator.
- `scripts/check-architecture.sh`, `scripts/tests/crate-layers.sh`,
  `scripts/tests/qa-linters.sh`, `scripts/check-gnome-idioms.sh`,
  `scripts/check-ux-traceability.sh`, the GP-4 rule-named test,
  `scripts/check-frontend-thinness.sh` (no new text matching its `rusqlite`/`filesystem`/
  `threads` patterns, no new `*worker*.rs` file).
- `wc -l` on every touched code file: each < 800.

### What the orchestrator runs after the code phase, on the clean worktree

- `heavy-run heavy -- scripts/check-newest-clippy.sh`
- `heavy-run heavy -- scripts/check-merge-readiness.sh` (includes the gnome suite and
  the rule-named display tests).

## Parallelität

**Not cut: one strand.**

- **Why.** Task 2's crate-root attribute only compiles once every one of Task 1's 93
  sites is gone, across ~60 files in 20 directories. Splitting Task 1 by directory gives
  strands whose own verification (`clippy -D` with the root attribute) cannot go green
  before the others merge, so the flip would become a third, sequential PR that no strand
  owns. Module-level `deny` blocks per strand would work, but add a temporary mechanism
  that the flip PR then has to delete again.
- **Size.** 93 mostly mechanical sites, one script section, two doc edits: one
  implementation run.

**Merge order:** none, a single branch.

**Post-merge cross-checks.**

1. On `origin/dev` after landing, `scripts/check-architecture.sh` reports
   `reprise-gnome: panic policy deny` and no crate as pending.
2. The first completed dev CI run that contains the commit is green, including the GNOME
   suite. A cancelled run proves nothing.
3. The AGENTS.md panic-policy bullet names the same crates as the table's `deny` rows
   (all nine).
