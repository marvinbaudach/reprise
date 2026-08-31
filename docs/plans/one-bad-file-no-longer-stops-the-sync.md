---
slug: one-bad-file-no-longer-stops-the-sync
worktree:
branch:
phase: planned
codex_session:
created: 2026-08-30
strands: a,b
merge_order: a,b
---
# One bad file no longer stops the sync

Mother plan. Two strands: `-a` (platform-linux) and `-b` (core + gnome +
runtime + docs). This file is frozen — the strands carry the tasks.

Three defects that share one victim: a device sync run that cannot finish
because of a single file it cannot write. Investigation of record:
`docs/plans/device-sync-mtp-phantom-objects-findings.md` (2026-08-29/30, live
Pixel 10 Pro XL).

---

## The evidence

`sync_runs` in the live database (`~/.local/share/reprise/reprise.db`) holds the
runs the findings doc describes. Its `deleted` column is a control arm and a fix
arm side by side:

| run | time | planned | copied | failed | deleted |
|-----|------|---------|--------|--------|---------|
| 77 | 2026-08-29 21:13Z | 2 | 0 | 0 | 0 |
| 78 | 2026-08-30 04:47Z | 13 | 12 | 1 | **0** |
| 79 | 2026-08-30 05:01Z | 12 | 11 | 1 | **0** |
| 80 | 2026-08-30 05:04Z | 12 | 11 | 1 | **0** |
| 81 | 2026-08-30 05:07Z | 1 | 1 | 0 | **63** |

- **Run 77 is defect 1.** Its `detail` column, verbatim: `could not clean
  partial sync files: device I/O failed: libmtp error: could not delete
  object.` Two files planned, none copied — the run ended before its first
  transfer.
- **Runs 78–80 are defect 2.** One failed track each, and not one deletion,
  while removals were pending. Run 81 is the same device three minutes later
  with nothing failing: 63 deletions. The removals are stopped by a gate, not
  by the device.
- The same rows settle what the findings doc left open. Its "13 of 70 units" is
  run 78's `planned: 13` — a file count, not the unit ledger. And because runs
  78–80 each copied 11–12 of their 12–13 planned files, **the transfers ran to
  the end**: there is no early exit, so the gate is the whole mechanism and this
  plan is not missing a second one.

## The three mechanisms, from the code

### 1. `CleanPartials` is fatal

`cleanup_partials_in` (`crates/reprise-platform-linux/src/device_sync.rs:406`)
deletes every `*.part` under the target folder with `child.delete_future(…).await?`.
The `?` propagates. `CleanPartials` is the first effect after `Start`
(`machine.rs:315`), and `PartialsCleaned(Err(_))` sets `terminal_error` and calls
`finish()` (`machine.rs:320-322`). One undeletable `.part` ends every run at
zero units. Pinned by `a_failed_partial_cleanup_ends_the_run_before_any_transfer`
(`machine_tests.rs:301`).

### 2. A failed track strands every removal

Not by stopping the run — `Event::TrackCopied(Err(_))` calls `fail_transfer`
then `advance_past_transfer` (`machine.rs:344-350`), and the remaining transfers
all run. The removals are lost three steps later:

- `fail_transfer` records the failed **device path** in `failed_device_paths`
  (`machine.rs:753-758`).
- `enter_playlist_writes` holds back any planned playlist naming one of those
  paths — `complete_unit(0); continue`, never entering
  `successful_playlist_sources` (`machine.rs:588-606`).
- `begin_removals` runs the removals only when **every** planned playlist source
  was republished and no obsolete playlist survived deletion
  (`machine.rs:648-659`). A held-back playlist fails that test; the gate closes
  and `finish()` runs with every removal untouched.

And because `self.failures` is non-empty, `finish()` returns `Failed`, so
`finish_sync` never reaches `refresh_contents_after_sync`
(`device_sync_planned.rs:272-276`) — the only path to `last_synced_at`. Both
halves of the findings doc's second observation are this one mechanism.

The chain that never runs, end to end: `Completed { verified_sources }` →
`refresh_contents_after_sync` → `RefreshPurpose::VerifySync(sources)`
(`device_sync_runtime.rs:502-513`) → `mark_device_playlists_synced`
(`device_sync_runtime.rs:616`) → `UPDATE device_playlists SET last_synced_at`
(`settings.rs:559`).

### 3. A failed managed write names no step

`replace_managed` (`device_sync.rs:472-528`) has five distinguishable failure
points and reports four through the same `DeviceIoError::Io(glib::Error)` →
`"device I/O failed: {error}"`: resolve storage, create directories, copy into
`<name>.part`, verify the partial's size, publish. `"Could not send object
info"` can come from three of them and the log cannot say which.

The findings doc proposed adding the relative path. It is already there — the
deviation note below the warning records `entry.device_path`
(`device_sync_effects.rs:206-213`). Only the step is missing.

---

## Decisions taken in the grill

1. **MTP-19 is amended, not circumvented.** Its rule text
   (`docs/ux-rules.md:660`) says a playlist that would name a track that never
   arrived is left unwritten. It becomes: such a playlist is published
   **without that entry**. The stated purpose of MTP-19 — a published playlist
   must not point at a track that never arrived — is *better* served that way
   than by leaving the previous file in place, whose contents nobody knows and
   which may itself name files the run is about to delete. Sentence 3's gate
   survives for the two genuinely unknown cases: a playlist whose write failed,
   and an obsolete playlist that could not be deleted.

2. **A failed run stamps `last_synced_at` on every playlist it republished**,
   incomplete ones included. The timestamp means "the file on the device was
   written by that run", which stays true. Safe because planning never reads it:
   `plan.playlist_writes` is populated unconditionally (`mirror.rs:592`) and
   `last_synced_at` appears nowhere in `mirror.rs`, so the next run rewrites the
   playlist in full and the lost track returns. Its only consumers are the page
   projection (`page.rs:138`) and the read-only MCP surface
   (`agent_device_sync.rs`); nothing acts on it. This is why B1 and B2 are one
   decision and must land together.

3. **Two strands, cut on crate boundaries.** Verified disjoint rather than
   assumed — see the cut below.

4. **No phantom probe.** An earlier draft re-queried the child after a failed
   delete to label the log line "phantom" or "still there". That is an extra
   `await` for one adjective whose `NotFound` branch no test would reach, and
   making *fatality* depend on an unmeasured probe would be worse still: gvfs
   served the phantom from a cached listing, and nothing establishes that
   `query_info` misses while the enumerator still shows it. Dropped.

5. **Live verification without re-creating a phantom.** Track 485 still fails
   every transfer, so the next real run is the experiment for both B and A2. No
   deliberate damage to the phone.

**Deferred, deliberately:** an undeletable partial reaches the `tracing::warn!`
but not the run log's deviation list, where the user would actually look.
Surfacing it needs `cleanup_partials_in` to report a count of what it left
behind, which changes a signature whose callers live in the other strand's crate
— it would cost the cut. Worth a follow-up once both strands have landed.

**Out of scope:** retry policy, back-off, or an "undeliverable track" flag.
Those are features; this is a defect fix. The unexplained cause of 485's
`Could not send object info` is not addressed either — A2 exists so the *next*
occurrence names its own step.

---

## Parallelität

Two strands. Cap is 3; a third would split `replace_managed`'s neighbourhood
inside one 800-line file and guarantee a conflict for no wall-clock gain.

### Strand A — `docs/plans/one-bad-file-no-longer-stops-the-sync-a.md`

Partial cleanup survives an undeletable file; a managed write names the step
that failed. Defects 1 and 3, which live in the same file.

*Owns:*
- `crates/reprise-platform-linux/src/device_sync.rs`
- `crates/reprise-platform-linux/src/device_sync_tests.rs`

### Strand B — `docs/plans/one-bad-file-no-longer-stops-the-sync-b.md`

A lost track no longer strands the run. Defect 2, both halves, plus the MTP-19
rule text.

*Owns:*
- `crates/reprise-core/src/device_sync/machine.rs`
- `crates/reprise-core/src/device_sync/machine_tests.rs`
- `crates/reprise-core/src/device_sync/ledger.rs`
- `crates/reprise-core/src/device_sync/sync_log.rs`
- `crates/reprise-core/src/device_sync/sync_log_tests.rs`
- `crates/reprise-gnome/src/ui/device_sync/device_sync_effects.rs`
- `crates/reprise-gnome/src/ui/device_sync/device_sync_planned.rs`
- `crates/reprise-gnome/src/ui/device_sync/device_sync_run_log_tests.rs`
- `crates/reprise-gnome/src/ui/sidebar/sidebar_device_card_mirror_tests.rs`
- `crates/reprise-runtime/src/devices.rs`
- `crates/reprise-runtime/src/devices_tests.rs`
- `crates/reprise-runtime/src/runtime_tests.rs`
- `docs/ux-rules.md`

### Why the cut holds — measured, not asserted

- `grep -rn "DeviceIoError" crates/reprise-gnome/` returns **nothing**. No match
  outside strand A's crate is exhaustive over that enum, so A2's new variant
  cannot break B's build. This is the one cross-strand hazard a new enum variant
  creates, and it is ruled out before the cut rather than after.
- `cleanup_partials_in` keeps its `Result<u32, DeviceIoError>` signature
  precisely so its two callers in the other crate
  (`device_sync_backend.rs:105`, `device_sync_smoke.rs:134`) need no edit.
- Both of B's changes widen an enum the whole workspace matches on, so every
  site was enumerated before the cut rather than discovered during it.
  `grep -rn "WritePlaylist {" --include='*.rs' crates/` gives nine sites, four
  of them beyond the obvious two and one in a third crate: `ledger.rs`,
  `sidebar_device_card_mirror_tests.rs`, `devices_tests.rs`, `runtime_tests.rs`.
  `grep -rn "SyncOutcome::Failed" --include='*.rs' crates/` adds four more that
  the task text does not suggest at all — including
  `reprise-runtime/src/devices.rs:218`, production code that *constructs* the
  variant, and `device_sync/sync_log.rs:138`, which matches it without a `..`
  rest pattern. All eleven files are in strand B's ownership above. Strand A
  touches none of them.

### Merge order

`a`, then `b`. Not a compile dependency — each builds alone — but A is the
smaller, lower-risk diff and the one that lets the device sync at all, so it
should not wait behind a change that amends a UX rule.

### Post-merge cross-checks

None of these may run inside a strand: each needs both diffs in one build, and
a strand's verification may only read files it owns.

1. `cargo build` for the whole workspace. `Effect::WritePlaylist` gained a field
   in B and `DeviceIoError` gained a variant in A; the build is where both
   claims are proven rather than discovered.
2. A real sync run against the Pixel with track 485 still in the plan. Expect:
   `sync_runs.deleted > 0` for a run whose `failed` is 1 — today those runs
   delete nothing (rows 78–80 above are the control arm).
3. The same run's warning line names a step, e.g. `creating the destination
   directory failed: device I/O failed: libmtp error: …` rather than the bare
   libmtp string.
4. `docs/ux-rules.md` MTP-19 reads as amended and the three `mtp_19_*` tests in
   `machine_tests.rs` match it.
