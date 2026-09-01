---
slug: device-sync-fits-the-file-budget
worktree: /home/marvin/Projects/reprise-device-sync-fits-the-file-budget
branch: feature/device-sync-fits-the-file-budget
phase: reviewed
codex_session:
created: 2026-09-01
---
# The device-sync module fits the file budget

`dev` is red. Run [`33482598611`](https://github.com/marvinbaudach/reprise/actions/runs/33482598611)
fails `Base and contract checks`, and `Quality gate` fails behind it:

```
crates/reprise-platform-linux/src/device_sync.rs has 831 lines;
Rust source files must stay below 800
```

Measured locally on this worktree (`2ab2a44509`, = `origin/dev`):
`scripts/check-architecture.sh` exits **1** with **exactly that one line** — no
second offender, no `window.rs` or orchestrator violation hiding behind it. The
script collects all violations before exiting, so that list is complete.

The gate tests `(( lines >= 800 ))` on `wc -l < "$file"`, so the target is
**<= 799 lines**, not 800.

## Why the file grew

`device_sync.rs` is the last flat module left in a family that is otherwise
already split: `identity`, `projection`, `read`, `inspection` and
`target_browser` live in sibling files wired with
`#[path = "device_sync_<name>.rs"] mod <name>;`. #781 (*The cleanup walk
survives one bad file*) pushed the remainder over the budget.

This strand keeps that established shape and moves two cohesive blocks out. It
is a pure move — **no behaviour change, no signature change**.

## The two cuts

1. **`device_sync_errors.rs`** — the outcome and error vocabulary, currently
   lines ~232-325: `CopyOutcome`, `WriteStep` (+ its `Display`),
   `DeviceIoError` (+ its `Display`, `std::error::Error`, inherent `impl`, and
   `From<gio::glib::Error>`). ~94 lines, no logic beyond formatting.
2. **`device_sync_paths.rs`** — the pure path and name helpers, currently lines
   ~695-819: `safe_target_components`, `safe_relative_components`,
   `safe_components`, `join_relative`, `is_audio_file`. ~125 lines, all pure
   functions.

`warn_cleanup_failure` and `choose_storage_volume` (~664-694) stay: they are
coupled to `DeviceStorage`, not to the path vocabulary.

Expected result: **831 - ~219 + module wiring ~= 615 lines**, comfortably inside
the budget with headroom for the next device-sync change.

## The three ways a mechanical split breaks here

1. **Public paths must not move.** `CopyOutcome`, `WriteStep` and
   `DeviceIoError` are used from 12 files across `reprise-gnome` and
   `reprise-platform-linux`. `pub use errors::{...};` in `device_sync.rs` must
   keep `reprise_platform_linux::device_sync::DeviceIoError` resolving exactly
   as today. No call site outside these two new files may change.
2. **Test-only visibility is already threaded here.** Line 25 has
   `#[cfg(test)] pub(crate) use identity::{...}`. Anything moving out gets
   `pub(super)` or `pub(crate)` deliberately — never a blanket `pub`.
3. **The test modules move with their subjects' imports.**
   `device_sync_tests.rs` and `device_sync_browser_tests.rs` exercise
   `safe_components`, `join_relative` and `is_audio_file` by bare path. Those
   imports must be updated, and the suite must *pass*, not merely compile.

## Verification

- `scripts/check-architecture.sh` exits **0**; `wc -l < crates/reprise-platform-linux/src/device_sync.rs` is **<= 799**. Record the observed number.
- `cargo test -p reprise-platform-linux` passes.
- `cargo build -p reprise-gnome` passes — proves the re-exports still resolve for the 7 consuming files there.
- `cargo fmt --check` and the strict workspace clippy pass.
- `git diff --stat` shows only `device_sync.rs`, the two new files, and the test files whose imports moved.

## Not in this strand

Touching `DeviceStorage` itself, splitting any other module, changing any
behaviour, or "improving" the moved code. Fixing the other red gates — those
are owned by `feature/fix-the-red-dev-gates`, which has already made them green
and is waiting to land.
