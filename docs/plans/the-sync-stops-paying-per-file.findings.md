# Review findings — the sync stops paying per file

Branch `feature/the-sync-stops-paying-per-file`, base `origin/dev` = `8a5c36227c`.
Four reviewers (3× rust-reviewer, 1× security-reviewer), Sonnet/high, each scoped
to one change plus the shared path/deletion surface.

## HIGH

### H1 — a failed lyrics write is reported as a successful sync
`crates/reprise-core/src/device_sync/machine.rs:395-398`

```rust
(Awaiting::WriteLyrics(index), Event::LyricsWritten(result)) => {
    self.ledger.complete_unit(result.unwrap_or_default());
    self.enter_lyrics_writes(index + 1)
}
```

The sibling analysis arm (`machine.rs:385-393`) sets `self.terminal_error` on
`Err`. The lyrics arm throws the error away. `finish()` (`machine.rs:714`) reads
`terminal_error` to decide `Completed` vs `Failed`, so a device that fills up
mid-run, or one transient MTP write error, ends as `SyncOutcome::Completed` with
the ledger at 100 %. `Effect::WriteLyrics` (`device_sync_effects.rs:210-213`) is
also the only failure arm in `perform` that logs no `DeviationKind`, so the run
log is silent too.

Not merely inherited from the old behaviour: before this change lyrics were not
work units and the sync's success signal did not claim to cover them. Now it
does, and it is wrong.

Self-healing over time (no `DeviceFileRecord` is written for lyrics, so the next
run re-plans the write), but the run that failed says it succeeded.

### H2 — the cancellation-cleanup test cannot fail
`crates/reprise-gnome/src/ui/device_sync/device_sync_transcode_prefetch_tests.rs:612`

`cancelling_with_prefetches_outstanding_discards_every_staged_output` is the test
guarding what the plan calls "the whole risk of this change". It does not guard
it: `FakeBackend`'s own transcode loop
(`device_sync_fake_backend.rs:158-164`) watches the same shared
`Arc<AtomicBool>` and calls `staging::discard` itself before `cancel_all` ever
runs.

Verified empirically by the reviewer: commenting out
`staging::discard(&transcode.staged_path)` in `cancel_all`
(`device_sync_transcode_prefetch.rs:86`) leaves the test green. A refactor that
drops that call ships without a red test.

## MEDIUM

### M2 — one bad path aborts the rest of the `.part` sweep
`crates/reprise-platform-linux/src/device_sync.rs:337-341`

`safe_relative_components(relative_path)?` inside the loop propagates on the
first invalid entry, so every remaining listed `.part` path in that run is
skipped. The delete-failure branch three lines below deliberately continues past
a single failure and has its own test. New failure mode: the old walk-based
sweep built `gio::File` children from enumerated names and never parsed a path
string, so this abort could not happen. Untested in either direction.

### M1 — the containment assertion in the rewritten cleanup test is vacuous
`crates/reprise-platform-linux/src/device_sync_tests.rs:484-522`

`Music/outside.part` survives only because it was never put in the `listed`
array. The assertion passes even if the `safe_relative_components` containment
check were removed entirely. No test feeds a listed `../../outside.part` and
asserts it is rejected rather than deleted outside the managed root.

### M3 — lyrics writes are reported to the user and to agents as "analysis"
`crates/reprise-core/src/device_sync/machine_sidecars.rs:26-30`, `:66-69`

Both reuse `SyncStep::WritingAnalysis` for the lyrics phase; there is no
`SyncStep::WritingLyrics` and nothing marks the reuse as deliberate. It surfaces
as `"↑ analysis ·"` (`device_sync_strings.rs:167`) and as
`AgentDeviceSyncPhase::WritingAnalysis` (`device_sync_agent.rs:292`).

The reuse is partly load-bearing: `device_sync_planned.rs:237` gates the
transfer-rate-meter baseline on `Copying | WritingAnalysis`, so it does keep the
rate meter running through lyrics writes. That explains the side effect, not the
mislabelling.

### M4 — the plan's headline scenario has no machine-level test
"Lyrics now update without the audio changing" runs through
`machine_sidecars.rs:22-32`. `mirror_lyrics_tests.rs` stops at planning;
`device_sync_lyrics_gate_tests.rs` always has an audio transfer in both syncs, so
the lyrics-only `opening_phase` branch is never entered. `lyrics_writes` appears
nowhere in `machine_tests.rs`. If that branch broke, a lyrics-only run would open
on a blank "removing" label (`phase_transitions.rs:75`) and nothing would catch it.

### M5 — no test for the `!is_current_run` supersession cleanup
The plan names this early return explicitly. Nothing exercises a prefetch that
completed into `work.transcoded` on a run that is then superseded — i.e. the
`transcoded.drain()` line in the `Drop` impl
(`device_sync_transcode_prefetch.rs:92-94`).

### M6 — the genuine `inline()` fallback is never exercised
`device_sync_transcode_effect.rs:15`. `first_candidate` returns the current
index too, so even the first transcode bootstraps through the prefetch branch.
A real miss is reachable — `fill` scans the whole remaining transfer list rather
than a positional window, so far-ahead transcodables can consume all three slots
and starve a nearer one — but no test constructs it. The plan requires that
"correctness never depends on the prefetch having happened".

### M7 — speculative: `discard` can now race a live encoder thread
`GioDeviceBackend::transcode_track` (`device_sync_backend.rs:131-154`) cleans up
its staged output only via the channel-drop path. With up to three prefetches in
flight, `cancel_all` can `discard` a staged path while the real encoder thread is
provably still running — an ordering that could not previously arise. Reviewer
could not confirm or rule out, as `GioDeviceBackend` and `glib::JoinHandle::abort`
drop-timing were outside the reviewed file set.

## LOW

- **L1** — the lyrics residency gate does not case-fold
  (`mirror_lyrics.rs:12-13`), unlike `plan_orphan_removals`, which folds via
  `device_case::fold_path`. A resident `Song.LRC` misses the gate and is rewritten
  every sync. Not a new overwrite capability — the old code copied unconditionally
  anyway — just a gate that misses.
- **L2** — the `cancellable.is_cancelled()` guard in `finish_managed_copy`
  (`device_sync.rs:568-572`) has no coverage; the one cancellation test hits the
  `Err(Cancelled)` branch above it instead.
- **L3** — stale error taxonomy: `WriteStep::VerifyPartial`
  (`device_sync_errors.rs:14`) is now constructed nowhere, and the strings
  "copying the partial file failed" / "partial device file has N bytes" now render
  for the direct-to-target track path where no `.part` exists. Locked in by
  exact-string test assertions.
- **L4** — duplicated unchecked `transfers()[index]` indexing across the two new
  prefetch files instead of a shared helper.
- **L5** — `PendingTranscode.cancellation` is always a clone of the run-wide flag;
  the field name implies a per-entry granularity that does not exist.
- **L6** — `impl Drop for PlannedWork` lives in
  `device_sync_transcode_prefetch.rs` while the struct is declared in
  `device_sync_planned.rs`, with no cross-reference comment.

## Confirmed sound — no finding

- The `.part` ordering trap the plan warned about: classification happens
  structurally before `accept()` is consulted
  (`device_sync_inspection.rs:143-160`), stronger than the plan asked, and a real
  test asserts a `.part` and a `.lrc` land in neither `managed_files` nor any
  orphan removal.
- `managed_files` content and sort order are byte-identical; the MCP surface maps
  only `DeviceStorageSnapshot` and never sees the new per-file lists.
- `replace_playlist` still uses `.part` + `publish`.
- Change 2's error surface: `SizeMismatch` / `PublishNotApplied` unchanged, and
  copy error, cancellation and size mismatch all delete the validated target.
- Delete-by-path validates every entry through `safe_relative_components` before
  it becomes a `gio::File`; the strings come only from the walk of the same target.
- `is_removable_managed_path` still exempts `.lrc`, doubly so since `.lrc` never
  enters `managed_files`.
- The wrong-file corruption mode of change 3 is structurally excluded:
  `work.transcoded` is a per-index map, staged names carry a process-global
  sequence counter, and a test traces each copy back to its staged path.
- Cleanup is `Drop`-based, so a future early return cannot bypass it.
- The machine remains the sole authority on phase and progress; prefetch never
  dispatches, never moves the ledger.
- Lyrics byte accounting is added for planned writes only, no double counting.

## Verification actually run by the reviewers

- `reprise-platform-linux`: check, clippy `-D warnings`, fmt, 159 tests — green.
- `reprise-core -p reprise-gnome -p reprise-platform-linux`: check + clippy
  `-D warnings` all targets — green. `device_sync::mirror` 44 tests, `lyrics_gate`
  1 test — green.
- Change-3 reviewer ran the three new prefetch tests and one mutation experiment
  (worktree restored afterwards).

Still not done, and it is the plan's own bar: the Pixel control/fix arm.
