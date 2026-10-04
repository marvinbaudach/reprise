---
slug: refactor-wave-2026-10
worktree:
branch:
phase: planned
codex_session:
created: 2026-10-04
strands: a,b,c
merge_order: a,b,c
---
# Refactor wave 2026-10 — best-practice consolidation

Behaviour-preserving restructuring. It pays down debts that the consolidation review
(`docs/plans/architecture-consolidation.md`, `docs/plans/consolidation-plan.md`) named and that
current churn and size data confirm. There are three waves. Wave 1 is written task by task;
waves 2 and 3 are fixed at package level only and get broken into tasks when they start.

## Evidence (origin/dev @ 1384703cce, 2026-10-04)

- **Too many arguments.** There are 56 `#[allow(clippy::too_many_arguments)]` lines under
  `crates/`, 14 of them in `reprise-core/src/queries/`. `queries/mod.rs` carries a family of nine
  `query_track_{window,count,ids}[_browsed[_ai]]` overloads with up to 11 positional parameters.
- **Files pressed against the cap.** About 150 `.rs`/`.kt` files sit between 700 and 799 lines.
  The ones in scope for wave 1:

  | File | Lines | Limit |
  | --- | --- | --- |
  | `ui/playback/player_controller.rs` | 798 | 800 |
  | `ui/preferences/preferences.rs` | 782 | 800 |
  | `ui/playback/queue_transport.rs` | 772 | 800 |
  | `queries/mod.rs` | 766 | 800 |
  | `db.rs` | 757 | 800 |
  | `library/settings.rs` | 733 | 800 |
  | `ui/window/library_shell.rs` | 728 | 800 |
  | `ui/style/mod.rs` | 707 | 800 |
  | `ui/window/window.rs` | 581 | 600 |

- **Hand-maintained migration list.** `db.rs` runs migrations v19–v87 from a hand-written list
  of 69 calls and keeps `SUPPORTED_SCHEMA_VERSION` as a separate number. The inline v1–v18
  baseline takes about 570 lines.
- **Consolidation-plan status, measured.**
  - Done: 2.2 (one filter bar).
  - Partial: 2.1 (one HTTP boundary). 12 `ureq` agents remain, matching `http_boundary_budget`.
  - Open: 2.3, 2.4a–i, 3.1 (`CoreError`), 3.2, 3.3 (parameter objects) and 3.4.

## Standing rules for every strand

1. **Behaviour-preserving.** No user-visible change, no schema change, no new migration, and no
   SQL text change beyond parameter plumbing. If a task can only be done by changing behaviour,
   stop and report.
2. **Stale ownership tables.** Three `AGENTS.md` sections are stale and released:
   - "Active file ownership — list geometry service"
   - "Active file ownership — multi-surface frontends"
   - "Active file ownership — Flathub readiness"

   Their plans were deleted on landing, which `docs/plans/README.md` defines as shipped, and none
   of their branches exists on the remote (checked 2026-10-04). Treat every file they list as
   unowned. This explicitly overrides AGENTS.md's "do not edit files owned by another strand"
   and "rebase onto the owning branch first" for those three sections. Strand B records the
   release in AGENTS.md.
3. **File size.** Every touched code file ends below 800 lines, and `window.rs` below 600. Never
   trim comments to fit. Extract a cohesive sibling instead.
4. **Source-scanning tests.** Many tests `include_str!` a production file and assert on its text.
   Before moving code out of a file, grep the crate for `include_str!("…<file>")` and handle
   every hit:
   - **Positive assertions** (needle must exist). Update the path or split point, or leave the
     asserted code in place.
   - **Negative assertions** (needle must not exist). Add the new sibling to the scanned set.
     Otherwise the guard silently weakens and still passes.

   Each task below lists the known sites. The grep is still mandatory.
5. **Language and commits.** English everywhere. Focused commits, one per task. No agent
   attribution lines.

## Decisions (grill, 2026-10-04)

1. **Strand A replaces the positional overloads with parameter objects.** This includes the
   per-source helpers. The doc comment that argued against a parameter object is replaced.
2. **Edition 2024 is not part of this program.**
3. **Wave 3 cuts `CoreError` additively.** It covers the type, `From<rusqlite::Error>`, and the
   facades that cli and mcp use, so that `rusqlite` leaves both crates.
4. **Waves 1 and 2 land autonomously.** Each strand goes through Codex, the review, adversarial
   verification of the findings, Codex applying the survivors, the local gate including
   `scripts/check-merge-readiness.sh`, and then `land.sh` into `dev`. The session asks before
   wave 3 starts. There is no `dev`->`main` promotion in this program.

---

## Parallelität

| Strand | Owns (globs) | Tasks |
| --- | --- | --- |
| A | `reprise-core/src/queries/**`, the call-site files listed in A, the budget block of `scripts/check-architecture.sh` | A1–A6 |
| B | `reprise-core/src/{db,db_schema_baseline,db_migrations}.rs`, `reprise-core/src/library/settings*.rs`, `reprise-core/src/lib.rs` (mod lines), `AGENTS.md` (three sections) | B1–B5 |
| C | the `reprise-gnome/src/ui/{playback,preferences,window,style}` files listed in C | C1–C6 |

- **Disjointness.** Checked by grep. The C files do not call the query API. B's `settings*` glob
  excludes `device_sync/settings.rs`. Only A edits `scripts/check-architecture.sh`.
- **Merge order.** The strands are independent, so they land in completion order. Each later
  strand rebases onto the `dev` the previous landing produced.
- **Post-merge cross-checks.** These read files no single strand owns, so they run after the
  last landing:
  1. `scripts/check-architecture.sh` on merged `dev`. A's `too_many_arguments_budget` must equal
     the merged count. B and C are not expected to add or remove such allows. If they do, the
     last-landing PR adjusts the budget.
  2. `cargo clippy --all-targets --workspace -- -D warnings` and `cargo test --workspace` on
     merged `dev`.
  3. Grep for the deleted names `query_track_(window|count|ids)_browsed` in live references:
     code, AGENTS.md and active docs. Completed plans stay as written.

---

## Later waves (package level)

**Wave 2 — lint discipline.** One strand, workspace-wide, runs alone after wave 1 has landed.

- **2.1 Suppressions get reasons.**
  - Where the lint fires in every configuration `clippy --all-targets` builds, every
    `#[allow(lint)]` becomes `#[expect(lint, reason = "…")]`.
  - Where it fires in only some (the cfg(test)-only `dead_code`, and the `unused_imports` alias
    groups in `ui/mod.rs`), it becomes `#[allow(lint, reason = "…")]`. An `expect` there would go
    unfulfilled under `-D warnings`.
  - An expectation that is never fulfilled deletes the suppression and the dead code it hid. This
    includes the stale `#![allow(dead_code)]` at the top of `ui/browse/filter_bar.rs`.
  - Enable `clippy::allow_attributes_without_reason`.
- **2.2 `significant_drop_in_scrutinee`.** Measure first. Enable it only if the site count is
  small enough to fix in the same PR; otherwise record the count.

**Wave 3 — architecture.** Up to three strands.

- **3.1 `CoreError`, additive slice** (consolidation 3.1/3.2). Add the type, a
  `From<rusqlite::Error>`, and the core facades that `reprise-cli` and `reprise-mcp` call. Then
  drop `rusqlite` from both crates, and have `check-architecture.sh` ban it there. The remaining
  ~700 internal signatures stay for later.
- **3.2 One HTTP boundary** (consolidation 2.1). It waits until the foreign issue-sweep branch
  that is editing `cover_download*.rs` has landed.
- **3.3 One add dialog** for podcasts and radio (consolidation 2.3).
- **3.4 Retire the `ui/mod.rs` alias layer** feature by feature, and fold the flat module
  families into folders.

**Not in this program: Edition 2024** (decided in the grill). It changes drop order for `if let` scrutinees and tail
expressions, which is exactly the RefCell-borrow panic class. It makes `env::set_var` unsafe
(26 sites). It may also reformat the workspace through rustfmt's style edition. If it ever runs,
it runs solo in a quiet window.
