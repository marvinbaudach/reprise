---
slug: night-b-the-scanner-stops-being-one-function
worktree: .worktrees/night-b-scanner
branch: refactor/the-scanner-stops-being-one-function
phase: refactored
created: 2026-09-07
base: origin/dev
owns: crates/reprise-core/src/library/{scanner.rs,scanner_vanish.rs}
---
# Night package B — the scanner stops being one 433-line function

## Autonomy

**Run this end to end without asking.** Plan → code → check → refactor → land,
including the merge. Autonomous landing is authorised. Do not ask for `/check`,
`/refactor` or `/ship` between phases. Stop only for a listed **stop
condition**; then leave the worktree, add a `## Findings` section to this file,
set `phase: blocked`, and stop.

Own worktree `.worktrees/night-b-scanner`, branch
`refactor/the-scanner-stops-being-one-function`, off `origin/dev`. Four sibling
packages run tonight. **Touch only the two files in `owns:`.**

**This is the highest-risk package of the night.** The scanner writes the user's
library. A behaviour change here corrupts data rather than breaking a screen. If
you are unsure at any point, stop and record it — a package that stops with a
finding is a success; one that lands a subtly wrong scanner is not.

## Why

`scan_folder_inner` in `crates/reprise-core/src/library/scanner.rs` runs from
line 266 to line 698 — **433 lines** against a house rule of under 50. Its
sibling `scanner_vanish.rs` says in its own module doc that it exists "purely to
keep `scanner.rs` itself under the project's 800-line rule", and it imports its
parent with `use super::*`.

That is the finding: the file split is a *symptom*. The helper steps were exiled
to a sibling to buy line headroom, while the function they came from kept
growing. Merging the two files back would exceed 800 lines and fix nothing. The
work is to give `scan_folder_inner` a body a reader can hold in their head.

Across the tree, 53 production files cite the 800-line rule as their reason for
existing. A sample of them split along real responsibility seams and re-export
transparently — that is disciplined work under a constraint. `scanner_vanish.rs`
is the outlier, because its boundary cuts through one function's control flow.

## The structure you are working with

Measured on `origin/dev` @ `fe89dc51ad`. Seven phases, with the data each one
needs from the last — this is the part you must not re-derive:

| # | Lines | What it does | Reads | Produces |
| --- | --- | --- | --- | --- |
| 1 | 281–291 | Root guard: does the root exist in the source? | `source`, `root` | early return |
| 2 | 293–300 | Set up accumulators and open the transaction | `conn` | `report`, `audio_files_seen`, `observed_paths`, `dirs`, `failed`, `mobile_sync`, `mount_cache`, `tx`, `walk_failure` |
| 3 | **302–633** | Walk entries: exclusions, metadata, move detection, upsert | all of the above | mutates `report`, `observed_paths`, `dirs`, `failed`, `audio_files_seen`, `walk_failure` |
| 4 | 638–642 | Apply mobile-sync metadata and register sidecars | `mobile_sync`, `source`, `tx` | mutates `report.updated` |
| 5 | 651–660 | Gather present candidates and guard evidence | `tx`, `root`, `audio_files_seen`, `observed_paths`, `dirs`, `failed` | `candidates`, `evidence`, `guard_evidence` |
| 6 | 661–695 | Root-guard decision, mark phase, choose the outcome | `guard_evidence`, `audio_files_seen`, `evidence`, `candidates`, `source`, `root`, `report` | `outcome` |
| 7 | 696–697 | Commit and return | `tx`, `outcome` | return |

Phase 3 is 331 of the 433 lines. It is the real target.

Calls into `scanner_vanish.rs` (all `pub(super)`):
`poison_walk_failure` (line 310), `present_candidates_under_root` (651),
`evidence_after_walk` (655), `guard_evidence_under_root` (657),
`any_candidate_confirms_root_with` (662), `reclassify_missing_with` (681),
`mark_vanished_with` (683).

Early returns inside the walk: 288 (root absent), 325 (walk error), 333
(directory), 337 (not audio), 370 (excluded), 429 (unchanged mtime), 442
(dismissed). Plus 665–678, the root-unavailable branch after the walk.

Returns `ScanOutcome`: `Completed(ScanReport { added, updated,
skipped_unchanged, excluded, errors, moved, vanished, healed })` or
`RootUnavailable { root }`.

## Tasks

### B.1 — a characterisation net, before any change

The existing suites are the net, and they are substantial:

| File | `#[test]` count |
| --- | ---: |
| `scanner_tests.rs` | 15 |
| `scanner_vanished_tests.rs` | 17 |
| `scanner_untagged_tests.rs` | 7 |
| `scanner_import_errors_tests.rs` | 4 |
| `scanner_exclusion_tests.rs` | 2 |
| `scanner_metadata_persistence_tests.rs` | 2 |

Run them first and record the count. **Every one must still pass at the end,
unedited.** You may not touch a test file — none is in `owns:`. If a test needs
editing, the refactor changed behaviour and that is a stop condition.

Before extracting anything, check whether the seven early-return conditions in
phase 3 are each covered. If one is not, say so in your findings; do not add
coverage in this package (the test files are not yours tonight) — but do note it,
because an uncovered branch is where a silent behaviour change hides.

### B.2 — extract phase 3's per-entry body first

The single highest-value cut. Inside the walk, the work done for **one entry**
is a self-contained decision: classify it, and either skip it with a reason or
write it. Give that its own function returning an explicit outcome — an enum of
the reasons (excluded, unchanged, dismissed, not audio, directory, error) plus
the write it decided on — instead of mutating six accumulators in place.

The loop then becomes: for each entry, decide, then fold the decision into the
report. That fold is where the counters live, in one place, instead of scattered
across seven early returns.

This is also the immutability rule in the house style: prefer producing a value
over mutating an accumulator in the middle of a branch.

### B.3 — name the remaining phases

Phases 1, 4, 5, 6 and 7 each become a function whose signature is the row in the
table above. Their names should say what they decide, not when they run:
`guard_root_before_walk`, `apply_mobile_sync`, `gather_vanish_evidence`,
`decide_outcome`. Avoid `phase_two`.

### B.4 — dissolve the size-driven split, if it dissolves

Once phase 3's body is extracted, re-measure both files. If `scanner.rs` now has
room, move the `vanish` functions back beside the code that calls them and
delete `scanner_vanish.rs` — the split existed only for line count. If it still
does not fit, **leave the split alone and say so**: a second size-driven split is
not an improvement, and the module doc should then be corrected to describe a
real seam rather than a line limit.

Do not create a new file purely to stay under 800 lines. That is the pattern this
package exists to stop repeating.

## Acceptance

- `scan_folder_inner`'s body is under 80 lines. Under 50 is the house rule and
  the target; 80 is the point past which you should stop and explain.
- Every extracted function is under 50 lines.
- All 47 scanner tests pass, **unedited**.
- No test file, and no file outside `owns:`, is modified.
- `scanner_vanish.rs` is either gone, or its module doc now names a real
  responsibility boundary instead of the 800-line rule.

## Gates

```
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p reprise-core
scripts/check-architecture.sh
scripts/check-frontend-thinness.sh
```

Redirect output to a file and check `$?`. Never read a verdict through a pipe —
`script | tail` reports `tail`'s status, which is always 0.

## Stop conditions

- A test needs editing to pass. Stop. That is a behaviour change.
- An extraction requires changing what `ScanOutcome` or `ScanReport` contain.
  Out of scope; those are the scanner's contract with everything upstream.
- The transaction boundary would move, or a phase would run outside `tx`. Stop —
  a partial write to the user's library is the worst outcome available here.
- `reprise-core` is already red on unmodified `origin/dev`. Check the control arm
  first.
- You reach B.2's extraction and it does not decompose cleanly. Land what is
  genuinely better, record why the rest resisted, and stop. Half of this package
  done well beats all of it done nervously.

## Landing

Squash-merge into `dev`. The title is taken verbatim; write prose about the
scanner, not about line counts. Something like *"The scanner decides one file at
a time"*. In the body, state plainly whether `scanner_vanish.rs` survived and
why.
