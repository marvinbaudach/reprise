---
slug: the-repo-is-ready-to-show
worktree:
branch:
phase: planned
codex_session:
created: 2026-08-31
strands: a,b,c
merge_order: a,b,c
---
# The repo is ready to show

Mother plan. Remediation of the 2026-08-31 repo audit
(`docs/plans/repo-audit-2026-08-31.findings.md`) under one goal: **the
repository is presentable to the GNOME community.**

Base: `origin/dev`. Audit pin: `9012b3de5dd875147046e0d8f9a442d3c13ed204`.
Strand files: `the-repo-is-ready-to-show-a.md`, `-b.md`, `-c.md`.

## Decisions taken before this plan

- **Task 0.10 — shelve.** ADR 003 records the verdict; `reprise-runtime` and
  `reprise-runtime-client` are deleted, `reprise-runtime-protocol` stays as the
  DTO layer for the direct-path D-Bus interfaces and for `reprise-mcp`.
  Consequence: the workspace drops from eleven crates to **nine**, which is what
  `AGENTS.md` and the consolidation documents already claim — the stale count
  fixes itself.
- **docs/plans compaction — the full 82 files.** E1's "unangetastet" is read
  narrowly: E1 protects the git history and the README's disclosure paragraph,
  not the directory's contents. Precedent: 130 plan files have already been
  deleted historically.
- **Commit convention — the documentation follows the practice.**
  `docs/agents/branching.md` and `CONTRIBUTING.md` are rewritten to describe the
  narrative-title convention actually in use. No hook, no history rewrite.
- **Retention rule — the specific document wins.** `docs/plans/README.md`
  governs `docs/plans/`; `AGENTS.md:53-54` shrinks to a pointer.
- **Release metadata — catch up, do not cut a release.** The metainfo's
  `<release>` list is brought up to 0.1.111 from the CHANGELOG and gated; the
  Flatpak source moves to the pinned `type: archive` form with the concrete
  `sha256` filled in at tag time.

## What this plan does not do, and why

- **No history rewrite.** The `Claude-Session:` trailers on 29 existing commits
  stay.
- **The `Claude-Session:` trailer cannot be stopped from inside this repo.**
  Verified: it appears in neither `~/.claude/settings.json` nor `.githooks/`; it
  originates in the Claude Code harness. Out of scope here — worth a separate
  look at the harness configuration.
- **No REUSE per-file headers.** A root `REUSE.toml` with path globs is
  REUSE-3.0-compliant, is one file instead of 2,646, and keeps the strand cut
  disjoint.
- **No release 0.1.111.** The plan makes the repo submission-ready; cutting the
  tag is a separate act.
- **No performance claim without a measurement.** Strand b's index task carries
  the measurement procedure from `consolidation-plan.md`; a number not produced
  by it does not go in the commit message.
- **No structural refactors in this run.** See "Second run" below.

## Facts established during the grill

These were looked up, not assumed, and several correct the audit:

- The three files missing from `po/POTFILES.in` are
  `crates/reprise-gnome/src/ui/strings_location.rs` (23 `N_!`),
  `strings_scan.rs` (8) and `crates/reprise-view/src/device_sync.rs` (33) — 64
  strings, matching the audit.
- The metainfo carries hand-written `xml:lang="de"` translations inline and none
  for `es`, although `es.po` is complete and `es` is in `po/LINGUAS`.
- Its newest entry is `<release version="0.1.84" date="2026-08-27">` against
  `Cargo.toml` 0.1.111.
- `SUPPORTED_SCHEMA_VERSION` is **80**, so the index migration is **81** — the
  consolidation plan's "50 → 51" is stale.
- **The GP detectors count test code.** Running the gates:
  GP-2 reports 43 blocking calls, nearly all in `*_tests.rs` plus a legitimate
  `sleep` in `lyrics_worker.rs:146`; GP-4 reports **3064** `unwrap()` where
  production has ~24, its examples all `gtk4::init().unwrap()` under
  `#[cfg(test)]`. GP-3 (2), GP-19 (12) and GP-20 (19) are real and small. This
  is why no GP rule could be flipped — the numbers are artefacts.
- GP-2's detector looks for `sleep`/`block_on` and therefore does not even find
  the real GP-2 violation (the synchronous query loop in `spawn_local`). GP-2
  stays `[planned]` after this run.
- The Android package rename needs **no** JNI symbol changes: zero
  `Java_de_reprise_spike` occurrences in `crates/reprise-android-ffi/src`.

## The cut

| Strand | Theme | Audit blocks |
|---|---|---|
| **a** | What ships — Flatpak, install targets, Android | 2, 8, 9 (packaging half) |
| **b** | The Rust code — correctness, performance, the runtime's crates, the GP detectors | 1, 6, 9 (code half) |
| **c** | The project surface — licensing, i18n, metadata, docs | 3, 4, 5 |

### File ownership as globs

- **a** — `meson.build`, `meson_options.txt`, `data/**` *except*
  `data/*.metainfo.xml` and `data/*.desktop`,
  `io.github.marvinbaudach.Reprise.yml`, `flatpak/**`, `RELEASING.md`,
  `scripts/check-merge-readiness.sh`,
  `scripts/check-runtime-service-install.sh`, `scripts/check-flatpak-*.sh`,
  `scripts/android-*.sh`, `scripts/check-android-*.sh`, `.github/scripts/**`,
  `android/**`, `crates/reprise-android-ffi/**`
- **b** — `crates/**` *except* `crates/reprise-android-ffi/**`, `Cargo.toml`,
  `Cargo.lock`, `acceptance/**`, `scripts/check-architecture.sh`,
  `scripts/check-gnome-idioms.sh`, `scripts/check-ai-hygiene.sh`,
  `docs/adr/**`
- **c** — `README.md`, `CONTRIBUTING.md`, `AGENTS.md`, `TESTING.md`,
  `CODE_OF_CONDUCT.md`, `docs/**` *except* `docs/adr/**`, `po/**`,
  `LICENSES/**`, `REUSE.toml`, `.gitignore`, `.superpowers/**`, `.github/**`,
  `artifacts/**`, `scripts/check-project-quality.sh`,
  `.github/**` *except* `.github/scripts/**`,
  `data/io.github.marvinbaudach.Reprise.metainfo.xml`,
  `data/io.github.marvinbaudach.Reprise.desktop`

Five carve-outs are load-bearing and none is cosmetic:

1. `crates/reprise-android-ffi/**` sits inside b's glob but belongs to **a** —
   the Android package rename and the database-handle work span Kotlin and the
   FFI crate together.
2. `docs/adr/**` sits inside c's glob but belongs to **b**, which writes ADR 003
   in the same commit that deletes the crates.
3. `data/*.metainfo.xml` and `data/*.desktop` sit inside a's glob but belong to
   **c** — they are metadata a visitor reads, and c needs them for the `es`
   translation and the release list.
4. `scripts/check-merge-readiness.sh` belongs to **a**, which makes *both* gate
   edits: removing `check-runtime-service-install.sh` and adding
   `check-release-metadata.sh` in full mode. c writes only the metainfo content
   that the second gate then checks.
5. `.github/scripts/**` sits inside c's glob but belongs to **a**. Found by grep
   during the grill: `.github/scripts/check-gnome-ci.sh:17` invokes
   `scripts/check-runtime-service-install.sh`, which a3 deletes. Without this
   carve-out strand a deletes a script that a file it does not own still calls,
   and CI breaks on a branch that looks green locally. c keeps
   `.github/workflows/**` and `.github/ISSUE_TEMPLATE/**`.

### Tasks split across strands on purpose

- **Five hardcoded `.set_label(...)` calls** live in b's files; the missing
  `po/POTFILES.in` entries are c's. b marks the strings, c gives them a
  catalogue. Neither can do both.
- **The release-metadata gate**: c writes the `<release>` entries, a wires the
  gate that checks them.

### Merge order: a → b → c

Two real dependencies, not preferences:

1. **a before b.** a removes the meson and `data/` targets that build and
   install the runtime binary; b then deletes the crates behind them. In that
   order each branch is green on its own — after a the crates exist and are
   merely not installed, after b they are gone. The reverse leaves a's branch
   referencing a binary whose source b already deleted.
2. **b before c.** c writes the corrected crate list into `README.md` and
   `AGENTS.md`. After b the workspace is nine crates — c can only write the
   truth once b has landed.

### Post-merge cross-checks

Every comparison that reads a file its strand does not own. None of these may be
a task inside a strand.

1. **GP-14 flip.** a fixes the manifest, c owns `docs/ux-rules.md`. Flip only
   after a lands and `flatpak-builder-lint` passes against the pinned source.
2. **The gate count.** a removes one gate and adds another, so the total stays
   at 27 by coincidence — confirm the showroom's displayed count is still
   derived from the script's own `gate()` calls and not from a constant, and
   that it reflects the new list rather than the old one.
3. **The citation scan against the compaction.** b owns
   `check-architecture.sh`, c deletes 82 plan files. Run the scan over the
   compacted tree. The pre-flight sweep found the only three out-of-tree
   citations (`showroom/vite.config.ts:12`,
   `showroom/src/data/measurements.ts:186`, both naming a *kept* file, and
   `acceptance/deezer-placeholder-portraits/run-accept.sh:23`, which b fixes),
   but the sweep and the gate are different code.
4. **The crate count in prose.** After b lands, grep the tree for "nine",
   "eleven" and enumerated crate lists and confirm c's rewrite matches
   `Cargo.toml` — including `docs/plans/architecture-consolidation.md:60-71`,
   which c owns and which carries the same stale count.
5. **Full `scripts/check-merge-readiness.sh`** on the merged result, plus
   `cargo test --workspace -- --test-threads=1`. Serial is not optional: the
   workspace suite is known flaky in parallel in `reprise-platform-linux`,
   `reprise-core::podcasts::ytdlp` and `reprise-android-ffi`, for reasons
   unrelated to this work. A red display-test set is not a regression until each
   named test has been re-run alone against clean `dev`.

## Second run — the structural work

Deliberately not in this plan:

- one shared FilterBar instead of four (2,656 lines, eleven identical
  signatures, a rule-named test copy-pasted verbatim three times);
- one cover pipeline instead of two (`ui/cover/cover_loader.rs` against
  `ui/podcasts/source_image*.rs` — two decoders, two eviction policies, two
  threading mechanisms);
- a file-length gate that measures cohesion rather than line count, plus the
  one live violation it currently misses (`device_sync_runtime.rs`, 824 lines);
- the `suite_skip` gap on the `base-contracts` job, which is why that violation
  goes uncaught.

**Why separate:** the gate change and the code it measures cannot land in the
same run. If `check-architecture.sh` is relaxed while the files it judges are
restructured, a green gate no longer distinguishes better code from a laxer
rule. There is no control arm. The gate work goes second, against a tree the
first run has already settled.

## Acceptance

On the merged result of run 1: the app shows a dialog instead of a backtrace for
an unopenable database; the Flatpak build contains no `reprise-worker` and
installs no runtime service; the workspace has nine crates; `reuse lint` passes;
`msgfmt` sees all seven catalogues with the three formerly orphaned files
included; the metainfo lists releases up to 0.1.111; `docs/plans/` holds ~115
files; `origin/main` no longer carries `.superpowers/sdd/progress.md`; six GP
rules are `[active]`; and the five post-merge cross-checks are green.
