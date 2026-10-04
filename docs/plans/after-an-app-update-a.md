---
slug: after-an-app-update-a
worktree: /home/marvin/Projects/reprise-after-an-app-update-a
branch: feature/after-an-app-update-a
phase: coded
codex_session:
created: 2026-10-04
---
# Strand a — the phone computes without a registered tree and never inherits a cancel

Mother plan: `docs/plans/after-an-app-update.md`. Read its "Why" and decisions D1 and D2 first.

## File ownership

Touch only these files:
- `crates/reprise-android-ffi/src/mobile_sync.rs`
- `crates/reprise-android-ffi/src/track_analysis/compute.rs`
- `crates/reprise-android-ffi/src/track_analysis/compute_tests.rs`
- new `crates/reprise-android-ffi/src/track_analysis/*_tests.rs` files, wired through a
  `#[cfg(test)] #[path = ...] mod` in `compute.rs`
- this strand file

No FFI signature changes, so the UniFFI surface stays identical.

## Tasks (test first: watch each new test fail before the fix)

1. **NAV-15c, no tree.**
   - Test: a one-track library like `library_with_one_track()` in `compute_tests.rs`, but
     WITHOUT `set_tree_uri`, and with a succeeding stub decoder.
   - `import_track_analysis(track_id)` returns `Ok(Computed)` and stores the render data
     (peaks and spectrogram present).
   - Name the test after the rule, for example
     `nav_15c_a_track_is_computed_before_the_tree_is_registered`.
   - Fix: in `import_via_sidecar`, map `LibraryError::TreeNotConfigured` from
     `configured_tree()` to `Ok(AnalysisImportOutcome::Missing)`. Every other error still
     propagates. Update the doc comments.
2. **NAV-15c, inherited cancel.**
   - Test: a decoder that blocks on a gate (see `wait_flag`, with bounded waits only).
   - The backfill path claims track X with a `current_slot`, so its decode is cancellable. A
     foreground `import_track_analysis(X)` on another thread joins it. Then cancel the backfill
     decode and open the gate.
   - The foreground call returns `Computed`, not `Cancelled`, the decoder ran twice, and the
     data is stored.
   - If driving the real backfill is impractical, reach the same state through
     `AnalysisContext::compute` with a `CurrentDecodeSlot` whose sink the test cancels.
   - Fix: in `AnalysisContext::compute`, when `background == false` and `join_or_claim`
     yields `Claim::Done(AndroidAnalysisOutcome::Cancelled)`, try again. Allow at most 3
     rounds in total; after that, return `Cancelled`. Background callers are unchanged.
   - Keep `render_data_already_valid` as the first check of every round.
3. **Gates.** All must pass:
   - `cargo fmt --check`
   - `cargo clippy --all-targets --workspace -- -D warnings`
   - `cargo test -p reprise-android-ffi`
   - `cargo test --workspace`

   Keep every edited code file under 800 lines. `compute_tests.rs` is about 548 lines today;
   move the new tests to a sibling if needed.

## Not here

The Kotlin retry logic, the cover pass and the UX rules belong to strand b. Do not compare
anything against strand b's files.
