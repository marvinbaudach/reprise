---
slug: auto-sync-default-off
worktree: /home/marvin/Projects/reprise-auto-sync-default-off
branch: feature/auto-sync-default-off
phase: reviewed
codex_session:
created: 2026-10-02
---
# A phone no longer syncs by itself unless asked to

## Problem

On 2026-10-02 a desktop `reprise` launched while the Pixel was attached over MTP and started a
device sync with nobody pressing a button. The mirror pass (MTP-17, unchanged by this plan)
removed every file under `Music/Reprise` that was not in the sync plan, which wiped test files in
the middle of a device run.

Why it started by itself (verified in code on `origin/dev`):

- MTP-30's switch "Sync automatically when this phone connects"
  (`DeviceSettings::sync_automatically`) defaults to **on**.
  - Schema: `crates/reprise-core/src/db_device_sync.rs` `ADD_SYNC_AUTOMATICALLY` (v44) adds the
    column with `DEFAULT 1`.
  - Code: `crates/reprise-core/src/device_sync/settings.rs` `load_or_create_settings` inserts a
    new device with only `(device_serial, device_name)`, so the row takes the column default 1. It
    then returns `DeviceSettings::transient(..)` with `settings.sync_automatically = true`.
- `SyncBalance::has_work` counts rewritten playlists, and every selected playlist is rewritten on
  every plan. So a phone with any selected playlist always has "work", and every connect or app
  launch with the phone attached auto-syncs.

## Decision (grilled with the user, 2026-10-02)

1. **The switch stays and defaults to off.** A new phone never auto-syncs until the user turns
   the switch on for that device. MTP-30 stays `[active]`, and its text changes from
   "default **on**" to "default **off**". The auto-start logic in `auto_start.rs` is unchanged.
2. **Already remembered devices are switched off too.** A new migration **v87** sets
   `sync_automatically = 0` for every existing `device_settings` row. There are no foreign
   installations (AGENTS.md, "Not released yet"), so resetting this choice once is correct.
3. **MTP-17 stays as it is.** `Music/Reprise` remains fully Reprise-owned. With auto-sync off, a
   mirror pass only runs when the user presses Sync. Do not touch the mirror or orphan-removal
   code.

## Tasks (test-first)

### Task 1 — new devices start with the switch off

- Failing test first, in the existing settings test module for `device_sync/settings.rs` (or its
  sibling test file):
  - `load_or_create_settings` for an unknown serial returns `sync_automatically == false`.
  - Loading the same serial again returns `false` from the **stored row**. This proves the
    insert wrote 0 and did not inherit the column's `DEFAULT 1`.
- Implementation in `settings.rs` `load_or_create_settings`:
  - The INSERT for a new device writes `sync_automatically` **explicitly** (0, taken from the
    returned settings value). It must not rely on the column default, which stays 1 in databases
    migrated through v44.
  - Drop the `settings.sync_automatically = true;` override.
  - Adjust the comment on `DeviceSettings::transient` so it no longer reads as the one exception
    to an "on" default.
- `settings.rs` is 749 lines. It must stay under 800. If it would not, extract a cohesive sibling
  module; do not trim comments.

### Task 2 — migration v87 switches remembered devices off

- Failing migration test first, in the same style as the existing `migrate_v44`/v65 tests:
  - A v86 database with two `device_settings` rows, one with `sync_automatically = 1` and one with
    `= 0`.
  - After `migrate_v87`, both rows are 0 and `user_version` is 87.
  - Running `migrate_v87` twice is a no-op, the same idempotency pattern as `migrate_v86`.
  - A database without a `device_settings` table, or without the column, if the existing
    migration helpers can produce that state, does not fail.
- Implementation:
  - `pub(crate) fn migrate_v87` in `crates/reprise-core/src/db_device_sync.rs`. Use a sibling
    module if that file would pass 800 lines; it is 611 lines now.
  - Shape: `if version >= 87 return`, then a transaction with
    `UPDATE device_settings SET sync_automatically = 0`, then `user_version = 87`.
  - Register it after `migrate_v86` in `crates/reprise-core/src/db.rs`, and raise
    `SUPPORTED_SCHEMA_VERSION` to 87.
  - Update every test or fixture that pins the latest schema version (search for `86` next to
    `user_version` / `SUPPORTED_SCHEMA_VERSION`).

### Task 3 — tests that relied on the old default

- Run the device-sync tests in `reprise-core` and `reprise-gnome`
  (`crates/reprise-gnome/src/ui/device_sync/*_tests.rs`). Some tests may implicitly rely on a
  freshly created device auto-starting, which was the old default.
- Make those tests set `sync_automatically = true` **explicitly** where auto-start is what they
  test, as `device_sync_auto_start_tests.rs` already does.
- Add one GNOME-level regression in `device_sync_auto_start_tests.rs`:
  - A freshly connected, never-seen device with a selected playlist (so `has_work` is true) does
    **not** auto-start.
  - Seed it through `load_or_create_settings`, not by writing the flag by hand.

### Task 4 — rulebook

`docs/ux-rules.md` MTP-30 (~line 773):

- Change "default **on**" to "default **off**".
- Add one sentence: a newly remembered device starts with the switch off, and schema v87 switched
  already remembered devices off once.
- No status change. Rule-named tests: if a test name maps to MTP-30, keep it and make sure the
  new regression is findable under that rule (follow the file's existing traceability
  convention).

## Verification

Run from the worktree root. Log every long output to a file and grep it.

- `cargo fmt --check`
- `cargo clippy --all-targets --workspace -- -D warnings`
- `cargo test --workspace`
- `cargo audit` (only accepted advisory: RUSTSEC-2024-0436)
- Core purity: `cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'` must be
  empty.
- Do not run the app. Do not touch `~/.local/share/reprise/reprise.db`. Do not touch any Android
  device.

## Files

`crates/reprise-core/src/device_sync/settings.rs` (and its tests),
`crates/reprise-core/src/db_device_sync.rs` (or a new sibling for v87),
`crates/reprise-core/src/db.rs`, schema-version fixtures, the device-sync tests under
`crates/reprise-gnome/src/ui/device_sync/`, `docs/ux-rules.md` (MTP-30 only), and this plan. The
list is a starting point, not a fence: stop only if the contract itself turns out wrong.

## Parallelität

Cannot be cut usefully. Task 1 and Task 2 touch disjoint Rust files, but Task 3's test
adjustments depend on both, and the whole change is small. One strand, no post-merge
cross-checks.
