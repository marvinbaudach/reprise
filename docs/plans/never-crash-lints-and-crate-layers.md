---
slug: never-crash-lints-and-crate-layers
worktree: /home/marvin/Projects/reprise-never-crash-lints-and-crate-layers
branch: feature/never-crash-lints-and-crate-layers
phase: shipped
codex_session:
created: 2026-10-10
---
# Never-crash lints and a crate layer table

Two hardening measures adapted from PhotoCraft (github.com/storytold/photocraft):

1. **Never-crash lints.** Production code in eight of the nine workspace crates may
   not call `unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!` or
   `unreachable!`. Clippy enforces this. Every remaining exception is a counted
   `#[expect(…, reason)]` at its site. Those six calls are what the lints remove;
   panics from `[]` indexing and slicing, `assert!` and `RefCell` borrows remain and are
   out of scope (see the non-goals).
2. **A table-driven crate layer check.** `scripts/check-architecture.sh` delegates to a new `scripts/check-crate-layers.sh` with one
   table that names every workspace crate. Each row lists the crate's allowed direct
   workspace edges and its banned dependency families. The gate fails on a crate the
   table does not name, and on a row whose crate no longer exists.

The work starts from `origin/dev` at `ad74ee08c9`.

## Decisions (settled in the grill, 2026-10-10)

- **Scope.** Every crate except `reprise-gnome`:
  - `reprise-core`, `reprise-view`, `reprise-runtime-protocol`, `reprise-stems`
  - `reprise-android-ffi`, `reprise-platform-linux`, `reprise-cli`, `reprise-mcp`

  `reprise-gnome` has 97 hits, many of them `RefCell` and UI invariants plus
  `build.rs`. It moves to a follow-up plan and appears in the table as `pending`.
- **Mechanism.** One crate-root attribute,
  `#![cfg_attr(not(test), deny(…))]`, in every lib and bin root of the covered crates.
  - It needs no `clippy.toml` change.
  - Test builds are exempt as a whole. The `allow-*-in-tests` keys would leave 32 core
    test lines red:
    - 22 `unreachable!` in test doubles, which no key covers.
    - 10 hits in `cfg(all(test, unix))` and `cfg(all(test, debug_assertions))`
      modules, which clippy does not recognise as tests.
  - `--all-targets` still lints each non-test lib and bin target, so production stays
    fully covered.
  - Integration tests and `examples/` are separate crates and stay untouched. That
    includes `reprise-core/examples/album-glow-cost.rs` with its 5 `expect`s.
  - `[lints]` in `Cargo.toml` is not an option. All nine crates set
    `[lints] workspace = true`, and Cargo cannot mix that with crate-local
    `[lints.clippy]`.
- **Mutex poisoning recovers.** The mutexes in `podcasts/stream_proxy.rs` guard four
  simple values:
  - `SERVER: Mutex<Option<Arc<Server>>>`
  - `ACTIVE_TOKEN: Mutex<Option<StreamToken>>`
  - `registrations: Mutex<HashMap<String, Arc<Registration>>>`
  - `source_url: Mutex<String>`

  None holds a multi-step invariant across a lock. A panic on one proxy thread
  therefore must not cascade into every later lock. Recovery goes through
  `PoisonError::into_inner`, as Task 2 specifies.
- **Ratchet.** An exact suppression budget, kept per crate (`panic_lint_budgets`) in
  `scripts/check-crate-layers.sh`, modelled on the
  existing `too_many_arguments_budget` block (origin/dev lines 312–330):
  - It fails when the count grows.
  - It fails when the count shrinks without the budget being lowered.
  - A stale `#[expect]` already fails by itself, through
    `unfulfilled_lint_expectations` under `-D warnings`.
- **Non-goals:**
  - `clippy::indexing_slicing` (core 245, view 14): index and slice panics stay possible
  - `assert!` in production code
  - `RefCell` borrow panics
  - `reprise-gnome`

  Each is a candidate for a later plan. Reviewers should not raise them as findings
  against this one.

## Measured starting point

Clippy 0.1.97 on `origin/dev`, `--lib --bins`, with the six lints at warn.

| Crate | unwrap | expect | panic | unreachable | todo / unimplemented | total |
|---|---|---|---|---|---|---|
| reprise-core | 24 | 46 | 0 | 4 | 0 | **74** |
| reprise-view | 0 | 0 | 0 | 0 | 0 | 0 |
| reprise-runtime-protocol | 0 | 0 | 0 | 0 | 0 | 0 |
| reprise-stems | 0 | 0 | 0 | 0 | 0 | 0 |
| reprise-android-ffi | 0 | 4 | 0 | 2 | 0 | 6 |
| reprise-platform-linux | 0 | 6 | 0 | 0 | 0 | 6 |
| reprise-cli | 0 | 1 | 0 | 0 | 0 | 1 |
| reprise-mcp | 0 | 1 | 0 | 1 | 0 | 2 |
| reprise-gnome (bin + build.rs) | 2 | 65 | 9 | 21 | 0 | 97 — out of scope |

The 74 core hits by category:

- (a) **Mutex poisoning.** Every `.lock().unwrap()` and `.lock().expect()` is in
  `podcasts/stream_proxy.rs`, roughly 10–12 sites.
- (b) **Static invariants, 9.** Literal URLs in `concerts/similar.rs:27,35`, plus
  `format.rs:271`, `spectrogram.rs:243`, `equalizer.rs:198` and
  `artist_portrait/placeholder.rs:135`.
- (c) **Binary readers, 13.** Each does `take(n)?.try_into().expect(..)`:
  - `device_sync/analysis_sidecar.rs:236`
  - `device_sync/listen_report.rs:445`
  - `device_sync/track_metadata_list.rs:168`
- (e) **`unreachable!` in match arms, 4:**
  - `device_sync/settings.rs:634`
  - `library/playlists.rs:643`
  - `podcasts/itunes_charts.rs:136`
  - one more
- (f) **Other, 38:**
  - 10 in `diagnostics/render.rs`: `write!(String).unwrap()` and
    `rsplit().next().unwrap()`
  - 4 in `podcasts/ytdlp*.rs`: `child.stdout.take().expect(..)`
  - 3 URL parses in `radio/search.rs`
  - 2 serde serialisations in `library/startup_tasks.rs`
  - 2 `chars().next().unwrap()` in `diagnostics/redact.rs`
  - about 17 "validated earlier" invariants, for example
    `device_sync/device_case.rs:89,99`, `library_doctor/review.rs:581` and
    `tag_edit_write_pipeline.rs:183`

The line numbers are from `origin/dev` at `ad74ee08c9`. Re-measure; do not trust them
blindly.

## Tasks

The file names below are a starting point, not a fence. A task may touch the module
that actually holds the state or the type it has to change. Stop only when the
contract in this plan is itself wrong.

### Task 1 — the policy attribute

List the crate roots of the eight covered crates with
`cargo metadata --no-deps --format-version 1`: every target of kind `lib` or `bin`.
On `origin/dev` the roots are:

- `crates/reprise-core/src/lib.rs`
- `crates/reprise-view/src/lib.rs`
- `crates/reprise-runtime-protocol/src/lib.rs`
- `crates/reprise-stems/src/lib.rs`
- `crates/reprise-android-ffi/src/lib.rs`
- `crates/reprise-android-ffi/src/bin/uniffi-bindgen.rs`
- `crates/reprise-platform-linux/src/lib.rs`
- `crates/reprise-cli/src/main.rs`
- `crates/reprise-mcp/src/main.rs`

`reprise-cli` and `reprise-mcp` have no `lib.rs`.

Add this block verbatim to each root, after the crate-level doc comment (`//!`) and
any existing inner attributes, and before the first `mod` or `use`:

```rust
// Production code must not panic on bad input or broken invariants: every
// fallible step returns an error instead. A genuine, documented invariant is
// suppressed at its site with `#[expect(clippy::<lint>, reason = "…")]`, and
// scripts/check-crate-layers.sh counts those suppressions against a budget.
// Test builds are exempt; test doubles legitimately use unreachable!/expect.
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented,
        clippy::unreachable
    )
)]
```

### Task 2 — `reprise-core`: remove the 74 panic paths

Rules by category:

- **(c) Binary readers: no `#[expect]` at all.** These are the hostile-input
  parsers the measure exists for.
  - Convert each to fallible code, either with a small helper per reader module or
    with `try_into()` plus `map_err(..)?` into the reader's existing error type. A
    helper might look like `fn take_array<const N: usize>(…) -> Result<[u8; N], E>`.
  - Each of the three readers needs a test that feeds truncated and garbage bytes
    and asserts an `Err` rather than a panic. Add it only where no such test exists
    yet.
- **(a) Mutex poisoning recovers.**
  - Add `crates/reprise-core/src/sync.rs`, declared in `lib.rs`, with
    `pub(crate) fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T>`. The
    body is `mutex.lock().unwrap_or_else(PoisonError::into_inner)`, plus a doc comment
    that says why recovery is safe for the values it guards.
  - Route every production lock in `stream_proxy.rs` through it.
  - Unit test: poison a mutex by panicking on a spawned thread while it holds the
    guard. Join that thread, which returns `Err`. Then prove that `lock_unpoisoned`
    returns the guard and the data is intact.
- **(b) Static invariants.** Remove the panic path where it costs little, for
  example a `const` value or a fallible constructor whose error the caller already
  handles. Otherwise mark the site
  `#[expect(clippy::<lint>, reason = "<the invariant>")]`. The reason names the
  invariant itself, for example "literal URL, parsed in a unit test". It never says
  just "cannot fail".
- **(e) and (f), everything else.** Remove the panic path using `let … else`, an
  early `return Err(..)`, `?`, or a match restructured so the impossible arm no longer
  exists in the type. Special cases:
  - Writing into a `String` is infallible. `let _ = write!(…)` is allowed there and
    only there.
  - Child-process pipes return `io::Error::other("…")` through the function's
    existing error path.
  - Serde serialisation propagates with `?`.
  - "Validated earlier" invariants get `#[expect]` only when removing them would need
    a type redesign.
- **Budget: at most 15 `#[expect]`s in core.** Above that, stop and report the list
  rather than inflating the budget.
- **Always `expect`, never `allow`.** A suppression of one of the six lints is
  written as `#[expect(…, reason = "…")]`, never as `#[allow(…)]`. An `allow` never
  goes stale, so it would keep the budget up after its code is gone. Task 5 counts
  `allow` for these lints and requires zero.
- **No leak into `reprise-gnome`.** Gnome is `pending`: it has neither the deny nor
  a budget.
  - Prefer fixes that keep public signatures unchanged.
  - Where a signature has to become fallible, adapting call sites in gnome (and in
    android-ffi, mcp and cli) is allowed.
  - Those call sites must handle the error: log it, fall back or skip. They must
    never `.unwrap()`, `.expect()`, `panic!` or `unreachable!` it. Otherwise a core
    panic just moves into gnome, where nothing counts it.

### Task 3 — the small crates: remove the 15 panic paths

`reprise-android-ffi` (6), `reprise-platform-linux` (6), `reprise-mcp` (2) and
`reprise-cli` (1) follow the same rules as Task 2. `reprise-view`,
`reprise-runtime-protocol` and `reprise-stems` receive only the attribute.

### Task 4 — the layer table in a new `scripts/check-crate-layers.sh`

**Where it lives.**
- The table, the membership check, the edge and family checks, and the whole of
  Task 5 go into a new sibling script, `scripts/check-crate-layers.sh`.
- `check-architecture.sh` delegates to it the same way it already delegates to six
  siblings at the end of the file (origin/dev L517–522), so no new gate line exists
  anywhere.
- Add the new script wherever `scripts/tests/qa-linters.sh` and
  `.github/tests/repo-wide-gates-are-unrouted.sh` enumerate scripts. Read both
  first; the `.github/tests` contracts pin exact text.
- Why a separate script: `check-architecture.sh`'s fixture mode
  (`ARCH_LINT_SIZE_ROOT`) exits before the dependency section. Every negative case
  would otherwise run the whole lint, about 36 `cargo tree` calls, and any unrelated
  red would hide the expected message.

**Table.** Put one declarative table at the top of `check-crate-layers.sh`, with one
row per workspace crate. Each row has five fields:

1. crate
2. allowed direct workspace edges, default features
3. allowed direct workspace edges, all features
4. banned families on the full normal tree
5. panic policy, `deny` or `pending` (used by Task 5)

The table can be a heredoc or an array. Values from the current manifests:

| crate | direct edges, default | direct edges, all features | banned families (full `-e normal --target all` tree) | panic policy |
|---|---|---|---|---|
| reprise-runtime-protocol | — | — | gtk4 libadwaita glib gstreamer zbus | deny |
| reprise-core | — | — | gtk4 libadwaita glib gstreamer zbus | deny |
| reprise-view | core | core | gtk4 libadwaita glib gstreamer zbus | deny |
| reprise-stems | core | core | gtk4 libadwaita glib gstreamer zbus | deny |
| reprise-android-ffi | core view | core view | gtk4 libadwaita glib gstreamer zbus | deny |
| reprise-cli | core | core stems runtime-protocol | default: gtk4 libadwaita glib gstreamer zbus · all features: gtk4 libadwaita gstreamer | deny |
| reprise-mcp | core | core runtime-protocol | default: gtk4 libadwaita glib gstreamer zbus · all features: gtk4 libadwaita gstreamer | deny |
| reprise-platform-linux | core runtime-protocol | core runtime-protocol | gtk4 libadwaita | deny |
| reprise-gnome | core view platform-linux | core view platform-linux | — | pending |

- `zvariant` in `reprise-runtime-protocol` is a data format, not `zbus`, and is
  allowed.
- `reprise-platform-linux` pulls in `glib` through gstreamer and `zbus` for MPRIS, so
  neither is banned there.
- For the all-features rows of cli and mcp: whether `zbus` pulls in `glib` is
  measured, not assumed. If the measured all-features tree contains no `glib`, ban it
  there too.
- Verify every row against the real workspace before freezing it:
  - edges: `cargo tree -p <crate> -e normal --prefix none --depth 1 --target all`,
    with and without `--all-features`
  - families: the same command without `--depth 1`

  If a row disagrees with the measured tree, stop and report. Never loosen a ban to
  make the table pass.

**Membership.**
- Read the workspace members, preferably with
  `cargo metadata --no-deps --format-version 1 | jq`.
- jq is preinstalled on the `ubuntu-24.04` runner that runs `base-contracts`, and
  explicitly installed in the Arch container jobs. Check `command -v jq` anyway and
  fail loudly when it is missing. A silent skip is forbidden: an exit 0 with no
  output looks exactly like a pass.
- A member without a row fails, and so does a row without a member.
- A failing `cargo metadata` fails closed and loud, under the same contract as
  `run_dependency_probe`.

**Edges.** For every row and both modes, compare the direct workspace edges against
the allowed set. A stray edge fails, naming the crate, the mode and the edge.

**Families.** These run on the full tree in both modes, as today.

**Bespoke rules stay.** Each of these keeps working exactly as before:
- no `reprise-stems` edge in core or mcp
- stems depends on core only
- no stems in gnome's default build
- no `ort` in gnome with all features
- the `rusqlite` depth-1 rule for cli and mcp
- every non-dependency rule

A block that a table row now fully covers may be removed, but its error message must
survive. `scripts/tests/qa-linters.sh:281–299` pins 19 literal strings from this
script. Every pinned string either stays in the script, or is updated in qa-linters
in the same change.

**Test seam.** Three environment overrides, read only by `check-crate-layers.sh`,
in the spirit of `ARCH_LINT_SIZE_ROOT`:
- `ARCH_LINT_LAYER_TABLE` points to a replacement table file.
- `ARCH_LINT_LAYER_MEMBERS` replaces the member list.
- `ARCH_LINT_PANIC_ROOT` points the Task 5 attribute scan and the suppression count
  at a fixture tree with the layout `crates/<crate>/src/…`. Under this override the
  crate roots come from the fixture's file layout (`src/lib.rs`, `src/main.rs`,
  `src/bin/*.rs`), not from `cargo metadata`, and no `cargo tree` runs.

Each override only redirects input. None of them skips a check. A gate fix can make
the gate blind; this one must not.

### Task 5 — the panic-policy ratchet, in `check-crate-layers.sh`

**Attribute check.** Every crate whose row says `deny` must carry the Task 1
attribute in every lib and bin root that `cargo metadata` lists. A missing attribute
fails and names the file. Match the six lint names inside a
`cfg_attr(not(test), deny(…))`, not just any mention.

**Suppression budget.**
- Count suppressions over `crates/<crate>/src` for every `deny` crate with:

  ```
  rg -U --count-matches '(allow|expect)\(\s*clippy::(unwrap_used|expect_used|panic|todo|unimplemented|unreachable)'
  ```

- Separately, count `allow(` suppressions of the six lints. Any count above zero
  fails: "write #[expect(…, reason)] instead of #[allow] so a stale suppression
  surfaces".
- Pin one exact `panic_lint_budget=<N>`, on a line of its own, where N is what Tasks
  2 and 3 leave behind.
- Mirror the `too_many_arguments_budget` block's structure and messages:
  - "panic-lint suppressions grew from N to M": return an error instead.
  - "panic-lint suppressions are down to M": lower `panic_lint_budget` to M.
- Print the count on success.

**Pending rows.** Both checks skip a `pending` row, but print one line per pending
crate ("reprise-gnome: panic policy pending") so the gap stays visible.

### Task 6 — negative proof

- Add `scripts/tests/crate-layers.sh`. Wire it everywhere
  `architecture-size-limits.sh` is wired today: find the callers first.
- It drives `scripts/check-crate-layers.sh` directly, never the whole
  `check-architecture.sh`, through the three Task 4 overrides and fixture trees it
  builds in a `mktemp -d`. It proves that each case below fails with its expected
  message:
  1. a member with no row
  2. a row with no member
  3. a stray direct edge (a fake table that forbids `reprise-core` for
     `reprise-view`)
  4. a `deny` crate root without the attribute
  5. a suppression count of N+1
  6. a suppression count of N−1
  7. a single `#[allow(clippy::unwrap_used, reason = "…")]`
- For cases 5 and 6, the harness reads N from the `panic_lint_budget=` line of the
  real script and generates the fixtures from it. A hard-coded N would break every
  time the budget is lowered.
- It also proves the green path, both on a clean fixture and on the real workspace.

**Mutation proof, by hand, never committed.** Each step uses a pristine copy and is
reverted after the run:
1. Delete the attribute from `crates/reprise-view/src/lib.rs`. The architecture lint
   must fail.
2. Add one `.unwrap()` to a core production file. `cargo clippy` must fail.
3. Revert both and confirm `git status` is clean.

### Task 7 — documentation

- **AGENTS.md.** The paragraph after the crate list ("`scripts/check-architecture.sh`
  enforces the dependency direction, but not for all nine. It names seven…") becomes
  the new truth:
  - one table in the script names every crate
  - an unregistered crate fails the gate
- **AGENTS.md, "Key conventions to match".** Add one bullet:
  - the never-crash attribute and its six lints
  - `#[expect(clippy::…, reason = "…")]` only for genuine invariants
  - the exact `panic_lint_budget` in `scripts/check-crate-layers.sh`
  - `allow` is not permitted for these lints
  - `reprise-gnome` is still pending
- **No citations of this plan.** No file under `crates/` or `scripts/` cites this
  plan's path. The retention rule in `docs/plans/README.md` would otherwise make the
  plan permanent.

## Verification

### What Codex runs

This list replaces the generic gate list in AGENTS.md for the Codex run, on purpose:

- `cargo fmt --check`
- `cargo clippy --locked --all-targets --workspace -- -D warnings`
- `cargo test --workspace --exclude reprise-gnome`
  - No GTK code changes in this plan.
  - The unfiltered gnome suite contains the `settle()` family, which blocks under
    load. Do **not** run it, and do **not** run `scripts/check-merge-readiness.sh`:
    the orchestrator runs both after the code phase.
- `cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'` must print
  nothing.
- `scripts/check-architecture.sh`, then `scripts/tests/qa-linters.sh`, then the
  Task 6 harness.
- `scripts/check-frontend-thinness.sh`.
  - `view_floor` is an exact ratchet, and the attribute adds lines to
    `reprise-view/src/lib.rs`.
  - If it trips, adjust the floor in the same change and say so in the commit
    message.
- `wc -l` on every touched code file. Each must stay below 800 lines.
- The gnome leak check must print nothing. Run it over the diff:

  ```
  git diff "$(git merge-base HEAD origin/dev)" -- crates/reprise-gnome \
    | rg '^\+.*(\.unwrap\(\)|\.expect\(|panic!|unreachable!)'
  ```

### What the orchestrator runs after the code phase, on the clean worktree

- `heavy-run heavy -- scripts/check-newest-clippy.sh`. A newer clippy can find an
  `#[expect]` unfulfilled, or new hits.
- `heavy-run heavy -- scripts/check-merge-readiness.sh`.
  - The display tests are not evidence for this change.
  - A script-only diff skips its own CI suites, so local evidence is the evidence.

## Parallelität

**Not cut: one strand.**

- **Why.** The halves look disjoint, crates on one side and scripts on the other,
  but they are coupled:
  - Task 5's exact budget lives in `scripts/check-crate-layers.sh`.
  - Its value is the outcome of Tasks 2 and 3.
  - Task 5's attribute check needs Task 1's attribute in place.
- **What a split would force.** Either of these:
  - a post-merge code edit to set the budget, which no strand owns and the pipeline
    has no phase for
  - a ratchet outside the script, which means a new merge-readiness gate line, a
    `ci.yml` line pinned by `qa-linters.sh`, the `.github/tests` contracts and a
    showroom coverage group

  Both cost more than the wall-clock a split would save.
- **Size.** About 89 mostly mechanical call sites plus one script section: one Codex
  run.

**Merge order:** none, a single branch.

**Post-merge cross-checks.**

1. On `origin/dev` after landing, `scripts/check-crate-layers.sh`, run through
   `check-architecture.sh`, names all nine crates exactly once, with `reprise-gnome`
   as `pending`.
2. The first completed dev CI run that contains the commit has a green
   `base-contracts` job. A cancelled run proves nothing.
3. The AGENTS.md panic-policy bullet names the same crates as the table's `deny` rows.
