# Package B — /check findings, 2026-09-07 23:5x

Branch `refactor/the-scanner-stops-being-one-function`, commits `470dd6e9cb`
(step 1) + `07764213b9` (step 2) on `origin/dev` `fe89dc51ad`.
Reviewers: `rust-reviewer` (behaviour parity) and a conformance worker
(acceptance criteria). Session:
https://claude.ai/code/session_01Nd776geNNUYjeQxwEfxeSr

## Verdict

No behaviour change found. Every acceptance criterion met. All findings below
are MEDIUM at most: stale doc self-references and style. Nothing blocks landing.

## Independently verified (this session, not Codex's report)

- `cargo test --locked -p reprise-core`: 2709 passed, 0 failed, 3 ignored —
  identical to the baseline count on `origin/dev`.
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`: clean.
- `scripts/check-architecture.sh`, `scripts/check-frontend-thinness.sh`: pass.
- Changed files vs. dev: exactly `scanner.rs`, `scanner_entry.rs`,
  `scanner_vanish.rs`. No `*_tests.rs`; the `#[cfg(test)]` module inside
  `scanner_vanish.rs` is byte-identical.
- SQL string literals: 20 before, 20 after, diff empty (literal-only compare
  against `fe89dc51ad`; the prompt's own grep-based check is unusable — it is
  case-insensitive and matches Rust identifiers like `insert`, `updated`,
  `Deleted`, so it reports a difference for any code change and wrongly blocked
  step 1's commit).

## Measured shape

| | base | now |
|---|---|---|
| `scan_folder_inner` body | 433 lines | 29 (36 with signature) |
| `scanner.rs` | 799 | 693 |
| `scanner_entry.rs` | – | 462 |
| `scanner_vanish.rs` | 782 | 796 (379 production) |

Largest extracted functions: `decide_outcome` 47, `upsert_track` 47. All others
≤ 38.

## Findings

### F1 — `scanner_vanish.rs` has 4 lines of headroom (was 18)

796 of the 800 at which `check-architecture.sh` fails. The new module doc cost
14 lines. Green today; the next added line in that file turns the gate red.
Not caused by a defect — worth deciding deliberately whether to buy headroom
back (the doc can be shorter) or to accept it.

### F2 — doc comments point at sections that moved

`scanner.rs:186` (`WalkState::record`), `scanner_entry.rs:221-222`, `:248` say
"see this function's `## Root guard` / `## Hint coexistence` doc section", but
those sections live on `scan_folder_inner`. `scanner.rs:548` still says
"in the walk loop below" and `:489` "Root-Guard case (a) in this function's
test suite" — the walk loop is no longer in that function. Copied verbatim
during the split.

### F3 — "seven" is now ten

`scanner_entry.rs:3-5` and `scanner.rs:188` speak of "the seven early returns" /
"seven ways an entry can end"; `EntryOutcome` has ten variants. The number was
true of the old body, and the refactor's premise is faithful transcription.

### F4 — the reconcile module doc overstates its seam

The new `scanner_vanish.rs` doc says the module is "the only place allowed to
conclude something about what it did not [see]", but `poison_walk_failure` and
`evidence_after_walk` collect evidence *during* the walk. Substantially
accurate, imprecise in one clause. Note the interaction with F1: fixing this
adds lines to the file with 4 to spare.

### F5 — `known_row` returns `Result` but cannot fail

`scanner_entry.rs:114`. The query error is swallowed with `.ok()` internally —
faithful to the old behaviour, but the signature invites a future caller to
believe it can fail.

### F6 — style

`scanner.rs:418` `let mut report = report;` inside `decide_outcome` (take
`mut report: ScanReport` in the signature instead). And `scanner_entry.rs`
reaches `exclusions::matches_file` through the parent's private `use` in
`scanner.rs:12` rather than importing it directly — legal, but that `use` now
looks unused to a reader while being load-bearing for the child module.

## Not done, by design

B.4 resolved as the plan's fallback: the split stays. 610 + 365 production lines
= 975, so `scanner_vanish.rs` cannot move back into `scanner.rs`. Its module doc
now names the reconcile responsibility instead of the 800-line rule, and
`use super::*` is gone.
