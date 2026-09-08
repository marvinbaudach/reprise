---
slug: one-filename-for-the-listen-report
worktree: /home/marvin/Projects/reprise-one-filename-for-the-listen-report
branch: feature/one-filename-for-the-listen-report
phase: shipped
codex_session:
created: 2026-09-07
---
# One filename for the listen report

## Why

The phone-to-desktop listening handshake names two files, and both names are
spelled twice — once in Rust, once in Kotlin, with nothing holding them
together:

| Role | Rust | Kotlin |
| --- | --- | --- |
| report | `crates/reprise-core/src/device_sync/listen_report.rs:20` `REPORT_FILE_NAME = "reprise-listens-back.rpl"` | `android/app/src/main/java/io/github/marvinbaudach/reprise/ListenReportWriter.kt:10` `LISTEN_REPORT_FILE_NAME = "reprise-listens-back.rpl"` |
| acknowledgement | `listen_report.rs:21` `ACKNOWLEDGEMENT_FILE_NAME = "reprise-listens-back-ack.rpl"` | `ListenReportWriter.kt:11` `LISTEN_REPORT_ACKNOWLEDGEMENT_FILE_NAME = "reprise-listens-back-ack.rpl"` |

No compiler sees both. No test compares them. There is not even a comment on
either side pointing at the other. A rename on one side does not fail: the
writing side writes a file the reading side never looks for, the sync reports
success, and the listens simply stop coming back. This is §4.1 of
`docs/plans/refactoring-survey-2026-09-07.findings.md` — the survey calls it the
most dangerous of the three remaining duplications precisely because it fails
**silently**, unlike the MPRIS bus name, which fails loudly and immediately.

The recorded lesson of this repository is that a comment asking two places to
stay in sync is a plea, not a mechanism. The mechanism already exists in one
place — `scripts/check-duration-format-parity.sh`, which holds two hand-written
copies of the duration rule together by comparing what each side asserts.

## Scope

Two constants, one gate. Explicitly **not** in scope:

- `crates/reprise-core/src/device_sync/track_metadata_list.rs`
  (`reprise-track-metadata.rpl`) — measured: it has no Kotlin counterpart, so
  there is nothing to drift against.
- `crates/reprise-android-ffi/src/listen_export_journal.rs`
  (`android-listens-back-export.journal`) — Rust-only, never crosses the
  boundary.
- §4.2 `is_absent_player` and §4.3 `BUS_NAME`/`OBJECT_PATH`. Separate decisions
  about where a shared thing lives; §4.3 is already done on
  `chore/cleanup-2026-09-07`.

## The decision: a gate, not an FFI export

The survey left two roads open. Measured, the gate is the right one.

**Exporting the constants through UniFFI** would give one true source, and it is
the answer the duration case could not use (an FFI hop per row per frame). A
filename is read once per sync, so that cost argument does not transfer. But
`ListenReportWriter.kt` today imports nothing but `android.*` and `java.io.*` —
it never touches the FFI. Making the constant an FFI call turns two `const val`
into runtime values, drags the native library into `ListenReportWriterTest`
(which is a plain JVM unit test today), and adds a native dependency to the one
Kotlin file whose whole job is the storage boundary. That is a bigger change to
the shipping code than the problem justifies.

**A parity gate** costs one script and three registration lines, changes no
shipping code at all, and catches the rename on either side — including a rename
made in a pure-Kotlin refactor that never opens a Rust file. It is also the
mechanism this repository already chose for the same class of problem three days
ago, so it is one idiom rather than two.

Chosen: the gate. Recorded here so the FFI road is a decision that was made,
not one that was missed.

## Tasks

### 1 — `scripts/check-listen-report-parity.sh`

New executable script, modelled on `scripts/check-duration-format-parity.sh`
(read it first; copy its shape, its `set -euo pipefail`, its `repo_root`
resolution, its embedded `python3` extractor and its failure-message style).

It extracts, by **role**, not by position:

- from `crates/reprise-core/src/device_sync/listen_report.rs`:
  `pub const REPORT_FILE_NAME: &str = "…";` and
  `pub const ACKNOWLEDGEMENT_FILE_NAME: &str = "…";`
- from `android/app/src/main/java/io/github/marvinbaudach/reprise/ListenReportWriter.kt`:
  `internal const val LISTEN_REPORT_FILE_NAME = "…"` and
  `internal const val LISTEN_REPORT_ACKNOWLEDGEMENT_FILE_NAME = "…"`

It fails when a constant is missing on either side, when either file is missing,
and when the two values for one role differ. On success it prints one line
naming both agreed filenames, the way the duration gate prints its case count.

Failure messages name the file, the constant and both values, and say what to
do: *change both sides together*.

**Extraction must not silently find nothing.** The duration gate's own guard is
the model: if the Rust side yields no constants, that is a hard failure with a
message saying what shape the extractor expects. A regex that quietly matches
zero constants is a gate that is green forever — the exact decoration this
repository has ruled against.

### 2 — register the gate

Three sites, matching how `check-input-parity.sh` is wired today:

- `scripts/check-architecture.sh` — add the call next to
  `scripts/check-input-parity.sh` / `scripts/check-android-theme.sh` near the
  end of the file (currently lines 430–432).
- `scripts/check-merge-readiness.sh` — add a `gate "Listen report parity" --
  scripts/check-listen-report-parity.sh` line next to
  `gate "Input parity"` (currently line 114).
- `scripts/tests/qa-linters.sh` — add the two assertions that already exist for
  `check-input-parity.sh`: `require_executable` for the new script, and a
  `require_pattern` that `check-architecture.sh` calls it. Read lines 97, 167 and
  226 and follow that pattern exactly.

### 3 — prove the guard falls

A guard that has only ever been green is decoration. In the worktree:

1. Change `reprise-listens-back.rpl` to `reprise-listens-back-x.rpl` in
   `ListenReportWriter.kt` only. Run the script, redirecting to a log, and
   confirm a **non-zero** exit and a message naming both values.
2. Do the same on the Rust side alone.
3. Delete the Kotlin constant entirely. Confirm the "missing constant" branch
   fires rather than the script passing on an empty match.
4. Revert all three. Confirm green.

Record the observed output of at least the first case in the PR body. Never read
the verdict through a pipe — `script | tail` reports `tail`'s exit status, which
is always 0. Redirect to a file, check `$?`, then read the file.

### 4 — a pointer on each side

Now that the mechanism exists, one line above each pair of constants naming the
gate — not a plea to keep them in sync, but a pointer to the thing that enforces
it, so the next reader knows a rename is checked and where.

## Acceptance

- `scripts/check-listen-report-parity.sh` exists, is executable, and passes on
  the unmodified tree.
- It fails, with a message naming both values, for each of the three break cases
  in task 3.
- `scripts/check-architecture.sh` passes and calls it.
- `scripts/tests/qa-linters.sh` passes.
- No shipping code changes: the diff touches `scripts/**`, plus at most the two
  comment lines from task 4 in `listen_report.rs` and `ListenReportWriter.kt`.
- No Rust or Kotlin behaviour changes, so no test may need editing.

## Gates

```
scripts/check-listen-report-parity.sh
scripts/check-architecture.sh
scripts/tests/qa-linters.sh
scripts/check-shell.sh
```

`check-shell.sh` matters because this adds a shell script — shellcheck runs
there. The Rust and Android suites are not required: nothing they compile
changes. If task 4's comments are the only source-file edit, a
`cargo check --workspace` is enough to prove the comments did not break a build.

## Landing — and the collision to wait out

**Do not land this before package C is in.** Package C
(`refactor/the-source-clients-share-their-boundary`) currently has
`scripts/check-architecture.sh` modified in its worktree — it lowers the
`http_boundary_budget`. Two branches adding lines to the same file conflict, and
the cheaper resolution is to be the second one. `chore/cleanup-2026-09-07` is a
third writer to that file if it ever lands.

Order: land C, rebase this branch onto the dev C produced, re-run the gates,
then land.

Squash-merge into `dev`; the title is taken verbatim, so it must read as prose
about what changed. Something like *"The phone and the desktop keep one name for
the listen report"*.

## Parallelität

**No cut.** Tasks 1–4 are one script plus its registration; task 3 tests the
artefact task 1 produces and task 2 wires. Splitting them would hand two agents
the same three files for no wall-clock gain — the whole package is smaller than
the cheapest useful strand.

The one external dependency is sequencing, not parallelism: package C owns
`scripts/check-architecture.sh` tonight, so this branch is written now and landed
after C.

## Known limits

For each exact constant name, the gate compares a loose declaration count with
the strict extractor count. This catches a production declaration that changes
to an unrecognised shape, such as `pub(crate) const`, while a stale declaration
of the same name still satisfies the strict extractor; the differing counts
hard-fail before the stale value can make the parity check green.

The gate deliberately does not fuzzy-match renamed identifiers. If the real
Rust constant is renamed while a stale declaration keeps the old exact name,
both counts remain one. Rust call sites such as `mirror.rs` already make a real
rename fail at compile time, and defeating that signal would require adding a
compiling decoy deliberately; guessing at near-name identifiers would make this
precise gate a heuristic instead.
