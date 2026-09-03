# Handover 2026-09-03 — the sync records the folder it used

Branch `fix/the-sync-records-the-folder-it-used`, worktree
`/home/marvin/Projects/reprise-mtp-ledger`, 17 commits over `origin/dev`.
**Not pushed, no PR, not landed — landing is the user's call.**

## What is on the branch

| Part | Commits | State |
|---|---|---|
| Adopt the resident folder spelling when creating directories | `7957606586` | reviewed |
| Return + record the path the copy actually used | `d360df61e9`, `8bc854e96c` | reviewed |
| Attribute agent-started syncs in the log | `7fc6366c70` | reviewed |
| Accepted review findings 1–4, 5–9, 11 | `610c220d6e` … `3b1083a148` | reviewed, approved |
| Heal stale ledger spellings | `1c73819e81`, `1206e37603` | reviewed, both defects verified gone |

Two plans (`the-sync-records-the-folder-it-used.md`,
`the-stale-ledger-spelling-heals.md`) and the findings document carry the
detail. All three are on the branch.

## The one thing that cannot be reconstructed later

The main checkout `/home/marvin/Projects/reprise` still has four files dirty on
`song-visuals-ask-the-stored-category`: `device_sync.rs` and `device_case.rs`
in `reprise-core`, `device_sync.rs` and `device_sync_tests.rs` in
`reprise-platform-linux`. That is the **pre-#816** variant of this work, written
before the `ManagedWalk` rework and superseded by `7957606586`, which carries it
forward with a three-way apply (the conflict in `replace_managed` was resolved in
favour of dev's copy-to-final-name). **Discard them; do not merge them forward.**
This sentence belongs in the PR description, because `land.sh` removes the
worktree and takes the plan files with it.

## What was measured, and what was not

Measured on the phone (attached, lock held, target `/Music/Reprise`, 2158 files,
785 ledger rows):

- `gio info` resolves `Emmure/Speaker Of The Dead` and answers **File not found**
  for `Emmure/Speaker of the Dead` — a live ledger row. gvfs does not fold case,
  which is the premise the whole change rests on.
- **77 rows across 11 directories** name a path gio cannot resolve. The orphan on
  deselect is live, not theoretical.
- **0 of 2158** files sit in a directory that exists under two spellings at once.
  Run 81's pair was an MTP scan artifact.
- Run 115 planned **0** — the drift causes no re-planning, because
  `own_inventory_path` short-circuits in `adopt_resident_spelling`.

**Not measured: anything about the fix on the device.** No build of this branch
was ever installed. The phone was unplugged before finding 3 could be checked.

## The next session's first move

Finding 3's control arm is still available and cheap, but it **expires**: the
phone runs 0.1.127, so `/sdcard/Music/Reprise/reprise-track-metadata.rpl` must
still contain the planned, mis-cased spellings. Read it before installing
anything from this branch — afterwards the pre-state is gone.

```
adb shell "cat '/sdcard/Music/Reprise/reprise-track-metadata.rpl'" | grep -c "Speaker of the Dead"
```

Non-zero proves the defect finding 3 fixed. Take the device lock first.

## Traps this session paid for

- **The `/tmp` scratchpad was wiped twice mid-session.** Once it killed a Codex
  launch (the redirect failed, so the run never started and looked like a clean
  exit), once it killed a run in flight. Pipeline logs now go to
  `~/.cache/reprise-pipeline-logs/`. Evidence belongs in a committed document,
  not in the scratchpad. Saved as memory
  `the-scratchpad-can-vanish-mid-session`.
- **Harness background watchers die with the session; `setsid nohup` survives.**
  Three watchers were torn down while their Codex runs kept going.
- **Four tests on this branch passed in both arms.** Two were caught by review
  (`matching_adopted_ledger_and_scan_votes_…` drove `compute_delta`, which has
  **zero production call sites**), two more were fixed in the same pass. Demand
  the falling assertion by name in every commit message.
- **A plan sentence that is almost right produces a CRITICAL.** "A path the
  device does not have loses" became, in code, "loses even when the device was
  never asked" — the empty scan on every connect outvoted healthy rows from
  ledger spellings alone. The correct rule is *the device may correct the ledger
  only where it was actually observed*, and the flag for that already existed
  (`managed_files_scanned`).
- **Two review agents reverted code in the same worktree at the same time.** It
  happened to come out clean, but a measurement taken in a worktree a second
  session is writing to is worthless. Serialise control-arm work.

## Open, deliberately

- The device verification above.
- `compute_delta`/`SyncCandidate` have no production callers at all. Either they
  are dead code or something was meant to call them; nobody has decided.
- The transiently failing GNOME lyrics test — second sighting of the fragility
  class already noted in the 2026-09-02 handover, still uninvestigated.
