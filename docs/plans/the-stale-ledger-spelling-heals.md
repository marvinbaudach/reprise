---
slug: the-stale-ledger-spelling-heals
worktree: /home/marvin/Projects/reprise-mtp-ledger
branch: fix/the-sync-records-the-folder-it-used
phase: refactored
codex_session:
created: 2026-09-03
---
# The stale ledger spelling heals

Closes the last open item of `mtp-directory-error-2026-09-02.findings.md`. Read
that document's sections "Measured on the device, 2026-09-02 23:15" and
"Follow-up worth its own plan" first — the numbers below come from the real
phone and are not hypothetical.

## The problem, precisely

77 of 785 ledger rows across 11 directories record a `device_path` that does not
exist on the device: `Emmure/Speaker of the Dead/…` where the phone has
`Emmure/Speaker Of The Dead/…`. Measured with `gio info`, which resolves the
resident spelling and answers `File not found` for the recorded one.

Those rows never heal, and the reason is a short-circuit:

```rust
// device_case.rs:30-32
if let Some(path) = own_inventory_path.filter(|path| folds_equal(path, desired_path)) {
    return ResidentSpelling::Adopt(path.to_owned());
}
```

`own_inventory_path` is the track's **own ledger row**. It folds equal to the
freshly computed `desired_path` because both come from the same canonical
`device_track_path` naming, so the planner adopts the stale spelling and returns
before `directory_spellings` — which does hold the resident truth from the device
scan — is ever consulted. `plan_file_changes` then compares that adopted path
against the identical inventory value and plans nothing. Both sides are
consistently wrong, so the delta is empty and the row is never rewritten.

The user-visible harm is not the spelling. It is that `delete_managed` takes the
ledger path, gets `NotFound`, returns `Ok(false)`, and leaves the file on the
phone forever when the track is deselected.

## The contract

**A recorded path that the device does not have loses to the device.**
`own_inventory_path` may only win when it is actually resident.

## §1 — Make the short-circuit resident-aware

`adopt_resident_spelling` (`crates/reprise-core/src/device_sync/device_case.rs:25`)
needs to know which paths the device really has. The resident scan is already in
this module's reach: `rewrite_desired_paths` is called from `mirror.rs:313` with
both the ledger inventory and the device scan, and `build_directory_spellings`
already chains the two.

Give `adopt_resident_spelling` the set of resident paths (folded, built once by
the caller — do not rebuild it per track) and take the own-inventory branch only
when that path is in it. When it is not, fall through to the existing directory
vote, which is exactly the machinery that knows the resident spelling.

Keep every other behaviour of the function unchanged: the `Keep` on a path
without a directory, the `Keep` on an unknown folded directory, and the
`Ambiguous` refusal all stay as they are.

## §2 — Let the existing transfer path do the healing

Do **not** add a reconcile effect and do not write the ledger from the planner.
Once §1 lands, the planner emits the resident spelling as `desired.device_path`,
it differs from the stale ledger row, and `plan_file_changes` plans an ordinary
replacement. The copy overwrites the bytes that are already there under that
name, `Effect::RecordFile` writes the corrected path, and the row is healed by
the machinery this branch already fixed and tested.

This costs one re-copy of the affected tracks — 77 on the measured device, about
3.5 minutes at the measured 2.79 s per unit — and then never again. That price
buys the use of tested paths instead of new machinery.

The one hazard is already covered: the replacement schedules
`RemoveReplacedFile` on the stale path, which does not exist, so it no-ops on
`NotFound`. That is exactly what
`machine_replacement_tests.rs`'s upgrade-migration test pins. Do not weaken it.

## §3 — The ancestor gap closes with it, or it does not

`build_directory_spellings` (`device_case.rs:95`) folds only the **full parent
directory** of each known path; intermediate ancestors never enter the map. With
§1 in place, check whether a case-drifted *artist* folder above an album folder
that the device does not have yet is now adopted. If it is not, record
intermediate ancestors as well — the map is built from paths that are already in
hand, so this is a loop change, not new I/O.

Decide it with a test, not with reasoning.

## Tests

The file list is a starting point, not a fence.

1. **A stale recorded path loses to the device.** Ledger row
   `Artist/Album of Things/01 x.opus`, device scan reporting
   `Artist/Album Of Things/01 x.opus`, desired path computed as the ledger's
   spelling. `rewrite_desired_paths` must emit the **resident** spelling.
   Control arm: without §1 it emits the ledger's, because `own_inventory_path`
   short-circuits. This is the test that carries the change.
2. **A resident recorded path still wins.** The ordinary case — the ledger row
   exists on the device — must keep taking the fast path and must not be
   disturbed by the directory vote. Without this, §1 could "fix" the bug by
   breaking every healthy row.
3. **The healed track is planned exactly once.** After the rewrite the track is
   planned as a replacement; after its copy is recorded with the resident path,
   a second planning pass over the same state plans **nothing**. Build this
   through `plan_mirror` / `plan_file_changes`, never through `compute_delta` —
   that function has no production call sites and a test built on it proves
   nothing (measured 2026-09-03).
4. **§3's question**, whichever way it falls.

State for every test which assertion falls when the change is reverted, and how.
Two tests on the predecessor branch were found to pass in both arms; do not add
a third.

## Verification scope

Rust only: `crates/reprise-core`, and `crates/reprise-gnome` if a call site moves.
Run `cargo fmt`, then
`cargo clippy --all-targets -p reprise-core -p reprise-gnome -- -D warnings`,
then `cargo test -p reprise-core -p reprise-gnome`.

Do NOT run: `gradlew` in any form, the Android suite, `uniffi-bindgen`, a release
build of `reprise-android-ffi`, `cargo audit`, or any repo-wide gate script. No
Kotlin, no Android, no packaging is touched. If AGENTS.md or a gate document
tells you to run the full gate before committing, that instruction does not apply
to this run — this exception is deliberate and stated here.

## Out of scope

- The device. No phone is attached to this machine. The 77-row healing run is a
  measurement for a later session; do not fake it and do not claim it.
- `delete_managed`'s `Ok(false)` on `NotFound`. It is the correct contract for a
  file that is genuinely gone; the bug was the path handed to it, not the
  tolerance.
