# The device measurement — the sync stops paying per file

Measured 2026-09-02 against the connected Pixel 10 Pro XL, closing the open
"Proving it" item of `the-sync-stops-paying-per-file.md` and the open checkbox in
PR #816.

## The arms

- **Control**: the installed `reprise` **0.1.127**, the build that was already
  running before the merge. Not a rebuild — the pre-merge dev binary itself.
- **Fix**: **0.1.128** built from `57c4b19ef7` (the merge commit) at
  `--release`.

Both arms drove the same phone, the same library and the same DB. Only the
binary differed.

## The plan's own arm no longer works — what replaced it

The plan's design was to select the deselected smart playlist "Recently played"
(50 entries, 325.1 MiB) and let that be the bounded body of work. On this device
that no longer produces any: **deselecting it removes zero files**, because every
one of its 50 tracks is also in "Top rated" or "Like Lorna Shore". Measured, not
assumed — a deselect-then-sync run planned and deleted nothing.

The replacement is stricter than the original. A fixed list of **70 tracks with
FLAC sources** was taken from `device_files`; for each, the `.opus`, the `.lrc`
and the `.reprise-analysis` were removed from the device with `gio remove`
(185–186 files). The sync then re-creates exactly those. The same list was used
for both arms, so both planned **140 units / 414,441,739 bytes** — identical to
the byte, which is a stronger match than the original arm could have given, since
a smart playlist's membership drifts between runs.

Timing comes from the app's own `sync_runs` table (`finished_at - started_at`),
not from a stopwatch around the process, so app startup is outside the number.
`bytes_per_second` was never used, per the plan.

## Results

| Run | Control 0.1.127 | Fix 0.1.128 | Change |
|---|---|---|---|
| Fresh copy, 140 units, 414.4 MB | **900 s** | **391 s** | **2.30× faster** |
| — per unit | 6.43 s | 2.79 s | |
| — per MB | 2.17 s | 0.94 s | |
| Steady state, nothing to do | **28 s** | **12 s** | **2.33× faster**, −16 s |

Machine load during the runs: control 1.8, fix 2.6–3.6. **The fix arm ran under
the heavier load and still won**, so both numbers are conservative rather than
flattering.

The steady-state saving of 16 s is larger than the ~8 s the plan predicted for
dropping the second tree walk. The remainder is change 1: 552 resident `.lrc`
that the old code rewrote unconditionally and the gate now skips.

## Where this lands against the plan's projection

The plan projected ~4.67 s → ~0.62 s per track, i.e. ~7×. The measured result is
2.3×. The gap is not a failure of the changes; it is the difference between a
per-track budget and a whole run:

- The projection counted only transcode, audio copy and the two sidecars. A real
  run also inspects the device, writes three playlists, records inventory and
  reconciles the library.
- The 140 units are not 140 audio files. Each fresh track brings its lyrics and
  its analysis sidecar, and those are now counted units (change 1) — cheap ones
  in the fix arm, ~0.93 s each in the control arm.

A separate, earlier control run makes the same point from a different body of
work: 70 units / 253.8 MB in 480 s, i.e. 1.89 s/MB against the fix arm's 0.94.

## What is proven and what is not

Proven: the sync is materially faster on real hardware, in both regimes, and the
steady-state saving specifically confirms changes 4 and 1.

Not isolated: this measures all four changes together. The plan shipped them
together on purpose ("the four changes interact"), so no per-change attribution
was attempted, and the numbers above should not be quoted as evidence for any one
of them alone — except the steady-state run, which contains no audio work and
therefore no change 2 or 3.

## Incidental findings

- A sync that is interrupted mid-run (the app dying) leaves `sync_runs` with
  `outcome=running` and `copied=0` even though files did land on the device. The
  next run re-plans the remainder correctly, but the abandoned row's counters are
  not a record of what happened.
- `music_device_sync`'s write actions (`configure`, `start`) returned
  `internal server error` for the whole session after the app was restarted,
  while `music_get_device_sync_state` kept working. The long-lived `reprise-mcp`
  processes appear to hold a connection to the app instance that was replaced.
  Restarting the app (with `sync_automatically = 1`) was used as the trigger
  instead.
