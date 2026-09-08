---
slug: the-concerts-view-fits-the-file-limit
worktree: /home/marvin/Projects/reprise-the-concerts-view-fits-the-file-limit
branch: feature/the-concerts-view-fits-the-file-limit
phase: shipped
codex_session:
created: 2026-09-03
---
# The concerts view fits the file limit

## Problem

`crates/reprise-gnome/src/ui/concerts/concerts_view.rs` is 803 lines. AGENTS.md
caps Rust source files at 800, and `scripts/check-architecture.sh` enforces it.
Because that check runs in CI's "Base and contract checks" job, and that job
gates the rest of the pipeline, every push run on `dev` currently fails and all
downstream jobs are skipped. Verified failing on `dev` push runs at least since
2026-09-03 07:01.

This is a pure file-organisation change. No behaviour may change.

## The cut

`impl ConcertsView` ends at line 423. Everything after it is free functions in
three cohesive groups, all taking `&Shared` / `&Rc<Shared>`:

- rendering the current state: `render_cache`, `apply_empty_state`,
  `render_current_failure`, `apply_row_connectivity`, `apply_footer`
- the fetch/refresh lifecycle: `maybe_background_refresh`, `request_fetch`,
  `finish_fetch`, `enabled_changed`, `start_refresh_timer`, `stop_refresh_timer`
- sorting: `wire_sorting`, `apply_sort`

`Shared` (line 53) and `notify_filter_changed` (line 49) are private to the
file, so any extraction needs them reachable from a sibling module.

## Constraints

- Follow the module's existing convention: small `concerts_*.rs` siblings
  declared in `concerts/mod.rs`. There are already 21 of them.
- Every resulting file stays below 800 lines, `concerts_view.rs` included.
- Do not trim or delete doc comments to fit — AGENTS.md forbids that explicitly.
  Extract a cohesive sibling instead.
- No behaviour change, no signature change visible outside the `concerts`
  module, no test rewritten to match a new shape.
- `concerts_view_tests.rs` (725 lines) is attached via `mod tests;` at the end
  of `concerts_view.rs` and reaches into these functions. Keep it working;
  widen visibility to `pub(super)` where the cut requires it.

## Known, out of scope

`conc_4c_settings_changes_re_evaluate_credentials_and_refresh_dependents`
(`concerts_view_tests.rs:655`) is a pre-existing flake on `dev` — it panics with
`left: NoCredentials, right: NeverFetched` in some runs and passes in others on
identical code. Do not chase it and do not "fix" it by weakening the assertion.
If it fails during verification, note it and continue.
