---
slug: HANDOVER-2026-09-07-refactoring-und-aufraeumen
created: 2026-09-07
branch: chore/cleanup-2026-09-07
worktree: .worktrees/cleanup-2026-09-07
base: fe89dc51ad (origin/dev)
phase: reviewed
---
# Handover — refactoring survey and cleanup, 2026-09-07

## State

Branch `chore/cleanup-2026-09-07` in `.worktrees/cleanup-2026-09-07`, based on
`origin/dev` @ `fe89dc51ad`. **Committed, reviewed, not pushed.** No PR exists.
Pushing and opening one is the next human decision.

Three documents came out of this session, each with a different job:

- **This file** — what happened, what is green, what was deliberately skipped.
- `refactoring-survey-2026-09-07.findings.md` — the survey and its reasoning,
  including what it argues *against* doing.
- `NIGHT-2026-09-07-parallel-packages.md` — five self-contained work orders for
  agents to run in parallel, each authorised to land by itself.

## Green as of the last run

| Check | Result |
| --- | --- |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean |
| `cargo test` (core, view, mcp) | 2,946 passed, 0 failed |
| `cargo test` (runtime-protocol, cli+mpris, mcp) | 209 passed, 0 failed |
| `scripts/check-android-suite.sh` | 99 suites, 605 tests, 0 failures |
| `scripts/check-architecture.sh` | passed |
| `scripts/check-frontend-thinness.sh` | passed |
| `scripts/check-shell.sh` | passed |
| `scripts/check-project-quality.sh` | passed |

## What landed

**Two user-visible Android bugs**, both in `formatDuration`. It had no hour
component, so a 74-minute album read `74:00` and a 1:02:33 episode read `62:33` —
and the same function formats album totals. It also formatted through the
default locale, so an Arabic-locale phone showed `٣:٠١` where the desktop showed
ASCII. The correct rule had been in Rust the whole time.

**A gate that silently skipped a shipped crate.** `bump-version.sh --base` had
no case arm for `crates/reprise-cli/*`, which Meson installs under `libexecdir`,
so changes there reached users under an unchanged version. It also still carried
arms for the two crates ADR 003 deleted.

**Duplicated decisions collapsed.** `fnv1a_64` in the podcast downloader (that
hash names directories on disk — drift orphans files). The MPRIS bus name,
object path and absent-player rule, which were spelled out in three places. A
bare `500` standing beside `MAX_TRACK_IDS`.

**Eleven unreachable `pub` items removed.** A `pub` item inside a `pub mod` chain
is a reachability root, so the dead-code lint never fires on it. Each was traced
to zero call sites across the whole repository first.

**`AGENTS.md` made true.** Its crate list described `reprise-runtime` and
`reprise-runtime-client`, deleted by ADR 003, and never mentioned `reprise-view`
or `reprise-android-ffi` — the two crates carrying the multi-frontend work. The
count "nine" stayed right while two of the nine were fiction.

## Three new mechanisms, and why each exists

Each replaces a comment that asked a human to keep two things equal. Each was
proven to fail before it was trusted: the guard was broken deliberately and
observed going red.

- `scripts/check-duration-format-parity.sh` — every case the Rust duration tests
  assert must also be asserted on the Kotlin side. Extra Kotlin coverage is fine;
  divergence is not.
- `scripts/check-shared-literals.sh` — declares, per contract string, exactly
  which files may contain it. Fails on a rename on one side, and on a new copy
  appearing anywhere. The second direction is the point: it is how consolidated
  duplication grows back. It covers the listen-report filenames, which the
  desktop writes and the phone looks up over SAF with nothing between them.
- `reprise_runtime_protocol::mpris` — one definition of the MPRIS address and the
  absent-player rule, used by the server, the CLI and MCP.

All three run from `scripts/check-architecture.sh`, which CI runs
(`.github/workflows/ci.yml:111` region) and `check-merge-readiness.sh` runs.

## The one design decision worth knowing

`reprise-cli` gained a dependency on `reprise-runtime-protocol`, behind its
existing `mpris` feature. That widens a grilled exception, so:

- The feature comment in `crates/reprise-cli/Cargo.toml` was updated in the same
  commit to describe the current shape.
- The boundary was **verified, not inferred**. The architecture gate probes the
  default build; running its exact command,
  `cargo tree -p reprise-cli -e normal --prefix none --target all`, still returns
  only `reprise-cli` and `reprise-core`.
- `reprise-mcp` already used this exact shape for the same crate.

If that trade is unwanted, the fallback is to drop the shared module and let
`check-shared-literals.sh` cover the MPRIS constants alone — its table already
has entries for them, so only the Rust `use` lines revert.

## Deliberately not done

- **`reprise-mcp` in `bump-version.sh`.** Same missing arm as `reprise-cli` had,
  but it ships in neither the Meson install nor the Flatpak manifest. Whether an
  unpackaged crate should move a version is a policy question, not a gap to close
  silently.
- **A size budget for `reprise-android-ffi`.** A new gate, not cleanup.
- **Merging `column_header_dnd.rs` and `column_layout_editor.rs`.** Both export
  `css()`; merging forces a rename in the style aggregator. Churn beyond the gain.
- **The large `sources_http` consolidation.** Measured: about 90 of 1,297 lines
  would collapse. The memory note asking for it has been corrected. Night package
  C takes exactly the identical parts and refuses the rest.

## Traps met on the way

**The Android suite runs only through `scripts/check-android-suite.sh`.** It
builds the FFI for the *host* and exports `LD_LIBRARY_PATH`.
`scripts/android-build.sh` looks like the same setup but builds for the device; a
raw `gradlew` after it fails 28 Robolectric tests at `NativeLibrary.java:325`,
none of them real. A fresh worktree also needs `android/local.properties` copied
in — gitignored, and Gradle aborts before compiling without it.

**Several gates are ratchets that fail in both directions.**
`check-frontend-thinness.sh` holds a `view_floor` and a dead-code allowlist;
`check-architecture.sh` holds `http_boundary_budget=16`. Removing a use lowers
the number, and the number must come down in the same commit with the reason
recorded. There is precedent in the comment blocks for exactly this.

**The review caught a fresh false claim I had introduced into `AGENTS.md`** —
about which crate produces the `.reprise-analysis` sidecars, in the very file
being corrected for false claims. `reprise-platform-linux` extracts the render
data; `reprise-core`'s device sync encodes the sidecar. Fixed, and worth
remembering: correcting a document is exactly when a new error slips in.

## Next actions

1. **Push and open a PR for this branch**, or drop it. Nothing is pushed.
2. **Start the night packages** — see `NIGHT-2026-09-07-parallel-packages.md`.
   Five agents, five worktrees off `origin/dev`, each landing on its own.
3. Everything else worth doing is ranked in the survey's §6.
