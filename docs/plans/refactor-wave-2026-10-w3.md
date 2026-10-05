---
slug: refactor-wave-2026-10-w3
worktree:
branch:
phase: planned
codex_session:
created: 2026-10-05
strands: a,b,c
merge_order: a,b,c
---
# Refactor wave 2026-10 — wave 3: architecture

Mother plan of the program: `docs/plans/refactor-wave-2026-10.md`. Its "Standing rules for every
strand" and "Decisions" bind every strand here; this file adds the wave-3 cut. Waves 1 and 2 landed
as #1068, #1070, #1071, #1082 and #1084. The user approved wave 3 on 2026-10-05.

Strand plans: `refactor-wave-2026-10-w3a.md` (CoreError), `refactor-wave-2026-10-w3b.md` (one HTTP
boundary), `refactor-wave-2026-10-w3c.md` (shared add-dialog scaffold). Each strand runs in its own
worktree, headless, with the plan file as its only channel. Implementation is done by Sonnet
`worker` agents (Codex is out of quota); the plans are written for a literal reader.

## Shared context

- Base: `origin/dev @ 0322ae01df` (2026-10-05). Every number below was measured there.
- Everything is behaviour-preserving: no user-visible change, no schema change, no SQL text change
  beyond plumbing, no change on the wire (timeouts, user agents, redirects, request spacing), no
  change to error message text, exit codes or accessibility roles. A strand that cannot meet that
  stops the step and reports.
- Edition 2024 is not part of this program (program decision 2).
- Toolchain: CI runs a newer clippy (1.99) than local (1.97). `allow_attributes_without_reason` and
  `significant_drop_in_scrutinee` are workspace lints (`Cargo.toml:46-47`). Every strand also passes
  `cargo clippy --all-targets --workspace --all-features -- -D warnings` (cli features `mpris`,
  `worker`; mcp feature `mpris`) and `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`.
- Budgets that must equal reality after every landing: `http_boundary_budget` and
  `too_many_arguments_budget` in `scripts/check-architecture.sh`, the four budgets and `view_floor` in
  `scripts/check-frontend-thinness.sh`.
- `docs/plans/README.md` deletes wave plans on landing. **No code or script may cite a
  `docs/plans/refactor-wave-…` path**: the "Documentation references from code" gate in
  `scripts/check-architecture.sh` fails on the first merge after the plan is gone. Each strand plan
  repeats this.

## The cut

| Strand | Delivers | Owns (summary; the strand plan has the exact list) |
| --- | --- | --- |
| **A — `CoreError`, additive slice** (consolidation 3.1/3.2) | `reprise_core::CoreError` (`Busy`/`Conflict`/`Storage`, transparent Display, private SQLite payload), `From<rusqlite::Error>` plus a transitional `From<CoreError> for rusqlite::Error`; the 78 core facades cli/mcp call return `Result<_, CoreError>`; `rusqlite` leaves `[dependencies]` of `reprise-cli` and `reprise-mcp` (stays a dev-dependency for the SQL fixtures under `tests/`); the architecture gate bans it in both crates' manifests and `src/` | new `reprise-core/src/error{,_tests}.rs`; `lib.rs` (two new lines); `library/stats.rs`; the 25 facade files; `From<CoreError>` impls on domain enums where the compiler asks; `reprise-cli/{Cargo.toml,src/error.rs,src/retry.rs,src/commands/{playlist,instrumental,worker}.rs}`; `reprise-mcp/{Cargo.toml,src/{capability,startup,data,doctor_actions}.rs}`; one-line `Ok(..?)` adapters outside core where a converted facade is a tail expression (known: `reprise-gnome/src/ui/podcasts/add_dialog_subscription.rs:48-54`); `scripts/check-architecture.sh` headless block (lines 388-405 and a new block after it); `scripts/check-frontend-thinness.sh` `[rusqlite]` number only if the measured count moves |
| **B — one HTTP boundary** (consolidation 2.1) | `reprise_core::net::{client,rate,breaker,fixtures}`: one `ureq::Agent::config_builder` for the eight in-scope providers behind an `AgentPolicy` that reproduces each provider's exact config; one request spacer keyed by provider budget (eight keys, today's intervals, reserve-then-sleep); the lyrics breaker lifted unchanged; the fixture seam moved unchanged; `http_boundary_budget` 12 → 5 plus an allowlist of agent-constructing files | new `reprise-core/src/net/**`; deleted `sources_http.rs`, `lyrics/breaker{,_tests}.rs`; `lib.rs` (`mod sources_http;` line only); `musicbrainz.rs`, `cover_download.rs`, `lyrics/{mod,lrclib,netease}.rs` and the import lines of four lyrics test files, `artist_portrait/deezer.rs`, `podcasts/{http,source_artwork}.rs`, `radio/{http,servers}.rs`, `concerts/http.rs`, `library/library_doctor/remote/{network,network_tests}.rs`; `scripts/check-architecture.sh` lines 240-270 only |
| **C — shared add-dialog scaffold** (shrunk from consolidation 2.3) | `reprise-gnome/src/ui/source_add_dialog/{chrome,generation,test_support}.rs`: the identical dialog chrome built once behind a spec (title, hint, footnote, labels, size, margins, status wrap), a `Generation` newtype replacing both `u64` counters, one `find_scroller` test helper; two new display tests pin each dialog's widget tree before the move | new `ui/source_add_dialog/**`; `ui/mod.rs` (one line); `ui/podcasts/{add_dialog,add_dialog_followers,add_dialog_tests}.rs` and new `add_dialog_chrome_tests.rs`; `ui/radio/{add_dialog,add_dialog_tests}.rs` and new `add_dialog_chrome_tests.rs` |

### Why strand C is smaller than package 2.3

Package 2.3 says: "the phase machine, the generation counter and the result list move into the shared
dialog". Measured on the current tree, only the generation counter exists in both dialogs:

- The podcast dialog has **no runtime phase machine**. Its `AddDialogPhase` is `#[cfg(test)]` and used
  by one test that counts its variants. Giving it one is new behaviour, not a move.
- The result lists are different widget classes — a `gtk4::Box` of row boxes versus a `gtk4::ListBox`
  inside a `gtk4::Stack` — with different accessibility roles. Unifying them changes the accessibility
  tree, which the brief forbids.
- The async delivery differs in timing: Podcasts reports a spawn failure inside the main-loop future,
  Radio synchronously. One helper would change one of them.
- Lifecycles differ (a fresh dialog per open versus one persistent `Rc`), as do commit semantics (the
  podcast preview keeps the dialog open; the radio confirm closes it) and the `on_added` signatures.

So C extracts exactly what is identical today and pins the rest with tests. The "one add dialog" needs
a product decision (podcast phase model, unified result container under an ACC rule, radio's missing
dialog title) and goes to wave 4 as such.

### Decisions a reviewer may dispute (one line each, with the reason)

1. A: `From<CoreError> for rusqlite::Error` exists as a transitional seam — it keeps hundreds of
   internal `?` sites and every frontend `?` compiling without edits; lossless for the three variants;
   leaves with the last internal `rusqlite::Error` signature.
2. A: `CoreError` has no `NotFound`/`Invalid`/`Backend(String)` yet — no converted facade produces
   them; `#[non_exhaustive]` leaves the door open.
3. A: Display is transparent (the SQLite message) — CLI stderr and MCP messages stay byte-identical;
   a prefix would be a user-visible change.
4. A: `rusqlite` stays a dev-dependency of cli and mcp — the integration tests forge `user_version`,
   hold `BEGIN IMMEDIATE` and seed doctor/concert/release rows that no core API can produce; the gate
   bans `[dependencies]` and the word under `src/`, exactly the cut the SQL gate already makes.
5. A: `DbError`, `ScanError` and the other domain enums keep their `rusqlite::Error` variants — cli and
   mcp never name the type through them; folding them is the ~700-signature remainder.
6. B: the spacer is keyed by provider budget, not by host — CAA already shares MusicBrainz's slot,
   radio-browser is many mirror hosts under one slot, fixture-mode requests have no host; a host key
   would change timings.
7. B: one algorithm (reserve under the lock, sleep outside) for all eight — identical single-caller
   timings, the shape the tests pin, no lock held across a sleep; cancellation semantics preserved per
   caller (`wait_for_slot` rolls back, `reserve_slot` keeps the reservation).
8. B: fixture variables keep their names — `scripts/check-lyrics-smoke.sh`, `scripts/ptr-e2e/run.sh`,
   `scripts/cua-e2e/*.sh` and the mcp integration tests set them.
9. B: no `SourceTransportError` — `TransportError` already exists for scrobbling and is matched in five
   GTK files; `RadioError::Parse`/`PodcastError`/`FetchError` are constructed or matched outside core.
10. B: `stream_proxy.rs` stays outside the boundary — it deliberately sends ureq's default identity;
    a "no user agent" knob with one user is not a boundary.
11. B: the breaker lifts but is not connected to more providers — connecting it would make them skip
    requests they make today.
12. C: radio's `adw::Dialog` keeps no title (hence no accessible name) and podcasts' status label keeps
    not wrapping — both are visible/assistive differences, fixed under an ACC rule in wave 4, not here.
13. C: the two pinning tests carry no UX-rule prefix — this strand changes no rule and the traceability
    gate would otherwise have to be fed a rule.

## Disjointness

- **Core files.** A edits the facade files (settings, queries, playlists, events, concerts
  config/query, artist news, ai_jobs, podcasts `config/store/query/downloads`, radio `config/station`,
  online_sources, modules, doctor preferences). B edits the provider HTTP files (musicbrainz,
  cover_download, lyrics, deezer, podcasts `http/source_artwork`, radio `http/servers`, concerts `http`,
  doctor `remote/network`). Checked by name: no file appears in both lists. `podcasts/store.rs` is A's,
  `podcasts/http.rs` is B's; `radio/station.rs` is A's, `radio/http.rs` is B's; `concerts/config.rs`
  and `concerts/query.rs` are A's, `concerts/http.rs` is B's.
- **`crates/reprise-core/src/lib.rs`.** A adds `pub mod error;` and `pub use error::CoreError;`; B
  replaces `mod sources_http;` with `pub(crate) mod net;`. Different lines, non-adjacent in the
  alphabetical `mod` list; git merges them. If they do collide, the later landing takes both lines.
- **`scripts/check-architecture.sh`.** A owns the headless block (lines 388-405 plus a new block right
  after it: direct-dependency probe and `rg -w rusqlite` over `src/`); B owns the `== Engine HTTP
  boundaries ==` block (lines 240-270: budget 5, comment, allowlist). Disjoint hunks; the merge order
  resolves the rebase. Neither touches `too_many_arguments_budget`.
- **GTK files.** C owns `ui/podcasts/{add_dialog,add_dialog_followers,add_dialog_tests}.rs`,
  `ui/radio/{add_dialog,add_dialog_tests}.rs`, the new folder and one `ui/mod.rs` line. A touches GTK
  only with one-line `Ok(..?)` adapters where a converted facade is returned in tail position; the one
  known site, `ui/podcasts/add_dialog_subscription.rs:48-54`, is not in C's list. Should A's compile
  reveal an adapter inside a C-owned file, A lands first and C rebases; C moves code verbatim, so the
  adapter travels with it.
- **Manifests.** Only A edits `Cargo.toml` files (cli and mcp). B and C edit none.
- **Tests.** A moves two predicate tests from `reprise-cli/src/retry.rs` to core; B moves spacing and
  breaker tests inside core; C adds display tests in new files. No test file is touched by two strands.
- **Strings and catalogs.** No strand adds a user-visible string; `po/` is untouched;
  `scripts/tests/gettext-catalogs.sh` must stay green after every landing.

## Merge order

`a`, then `b`, then `c`.

- A first, because it is the only strand that may touch another strand's files (GTK adapters) and it
  edits the most files; later rebases then carry its one-liners forward instead of A re-applying them
  onto moved code.
- B is independent of A and C; if A is late, B may land before it (disjoint files, disjoint script
  hunks). Each later strand rebases onto the `dev` the previous landing produced.
- C lands after A in every case (shared GTK directory).
- Worktrees live under `.worktrees/<slug>` or `/home/marvin/Projects/reprise-<slug>`, never under
  `/tmp`; branch `feature/<slug>`; one squashed pull request into `dev` per strand per
  `docs/agents/branching.md`. No `dev` → `main` promotion in this program (program decision 4).

## Post-merge cross-checks (on merged `dev`, after the last landing)

1. `scripts/check-architecture.sh`: `http_boundary_budget` equals the measured count (expected 5);
   `too_many_arguments_budget` still equals 29; the new rusqlite probe and allowlist are green; the
   size caps hold (`radio/add_dialog.rs`, both `add_dialog_tests.rs`, `podcasts/add_dialog.rs`,
   `library_doctor/scan.rs` 783 untouched, mcp `source_actions.rs` 776).
2. `scripts/check-frontend-thinness.sh`: `[rusqlite]` equals the measured gnome count (114 unless A
   lowered it); `threads`, `filesystem`, `workers`, `view_floor` unchanged.
3. `cargo clippy --all-targets --workspace -- -D warnings`, the same with `--all-features`,
   `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`, `cargo test --workspace --exclude reprise-platform-linux`
   under the headless env, then `scripts/check-merge-readiness.sh` (includes
   `scripts/check-display-tests.sh --rule-named` and `scripts/tests/gettext-catalogs.sh`).
4. Greps that must print nothing: `rg -n -w rusqlite crates/reprise-cli/src crates/reprise-mcp/src`;
   `rg -n 'sources_http|LAST_REQUEST|LAST_ACOUSTID|MIN_REQUEST_INTERVAL|REQUEST_INTERVAL' crates/reprise-core/src`;
   `rg -n 'docs/plans/refactor-wave' crates scripts`; `rg -n 'rusqlite_is_busy|is_constraint_violation' crates/reprise-cli crates/reprise-mcp`.
5. `cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'` prints nothing;
   `cargo tree -p reprise-cli -e normal --depth 1 --prefix none | grep '^rusqlite'` and the mcp
   equivalent print nothing.
6. `scripts/check-lyrics-smoke.sh` (lrclib fixture seam and request log) is green.
7. The two C pinning display tests pass on merged `dev` under xvfb, one process each.
8. Scan `AGENTS.md`: no ownership section of this wave is left behind; the strand plans are deleted on
   landing per `docs/plans/README.md`.

## What stays for wave 4

- **Consolidation 3.4 — retire the `ui/mod.rs` alias layer** feature by feature and fold the flat
  module families into folders (wave 2 L4 deleted the unused aliases; the used ones remain).
- **Parameter objects, remainder.** 13 `too_many_arguments` suppressions under `crates/` carry a
  reason that says the function "should take a parameter object" (measured 2026-10-05; the brief
  estimated ~17). Budget is 29.
- **Gettext catalogs.** 547 obsolete `#~ msgid` lines across the seven catalogs (de 210, es 210,
  fr 41, hi 28, ar 27, zh_CN 27, bn 25; measured 2026-10-05, the brief estimated ~378). Regenerate
  with `msgattrib --no-obsolete` in one commit, keep `scripts/tests/gettext-catalogs.sh` green.
- **`trash_boundary_tests` flake** in `crates/reprise-android-ffi/src/playback.rs:47-48`
  (`#[path = "trash_boundary_tests.rs"] mod trash_boundary_tests;`) — the trash-callback writer probe
  races the queue persister; needs a condition, not a wall-clock budget.
- **Motion-token gate** (`scripts/check-motion-tokens.sh`) is blind to sibling `*_tests.rs` files; it
  strips only inline `#[cfg(test)] mod … {}` blocks.
- **From strand A:** the ~700 internal `rusqlite::Error` signatures; folding `DbError::Sqlite`,
  `ScanError::Sqlite`, `DoctorError::Database` and the other twelve domain variants into `CoreError`;
  deleting the transitional `From<CoreError> for rusqlite::Error`; `NotFound`/`Invalid` variants once a
  facade needs them; `reprise-gnome`'s own `rusqlite` dependency (114 budgeted lines).
- **From strand B:** one fixture variable with a subdirectory per provider (needs
  `scripts/check-lyrics-smoke.sh`, `scripts/ptr-e2e/run.sh`, `scripts/cua-e2e/*.sh` and the mcp tests
  in the same change); `SourceTransportError` across the provider enums (ripples into GTK matching);
  connecting the breaker to the other providers; keying the spacer by host as a policy change; gating
  the lrclib/netease fixture seams behind `test-fixtures` (today compiled in release builds);
  `stream_proxy.rs`'s own agent; folding `http_body.rs` into `net`; `reprise-stems`' own `ureq::get`
  (`crates/reprise-stems/src/provision.rs:368`), which the budget does not count.
- **From strand C:** the real "one add dialog" — a runtime phase model for Podcasts, one result
  container (an accessibility-tree change under an ACC rule), one delivery helper (after unifying the
  spawn-failure timing), radio's missing `adw::Dialog` title (no accessible name today), and
  `preview_name_claim` returning the untranslated `RADIO_STREAM_DETECTED` msgid
  (`ui/radio/add_dialog_network.rs:30`, pinned by `rad_8_placeholder_is_display_only_not_a_name_claim`).
