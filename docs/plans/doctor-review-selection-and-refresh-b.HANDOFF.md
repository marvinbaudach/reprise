# Handover — strand B, B-0 coded and verified, measurement still owed, 14.08.2026 16:21

**Status: B-0 is committed on its branch and independently verified. Nothing is
pushed, there is no PR, and the measurement that unlocks the rest of B has not
happened.**

| | |
|---|---|
| Strand A | merged — `604677322e` (#478), worktree and branch gone |
| Strand B worktree | `/home/marvin/Projects/reprise-doctor-review-selection-and-refresh-b` |
| Strand B branch | `feature/doctor-review-selection-and-refresh-b`, **local only** — `git ls-remote` returns nothing |
| Commits | `82af9bf88a` B-0, `22fc1877e6` the plan status block |
| Branch point | `57ff0bfc74` (`origin/dev` at 14:43, contains A) |
| Plan phase | `coded` |
| Tree | clean |

`origin/dev` has since moved to `8b87ae8ada` (#481 sidebar, #482 Android
portraits). Neither touches `review_page.rs` or the new probe file, so the rebase
is expected to be trivial — but it has not been done.

---

## What B-0 contains

Only the two files B-0 is allowed to touch, plus the plan.

**`review_page.rs` (+36).** Five `tracing::debug!` lines in `ReviewState::refresh()`
(R-14): one per stage for `grouped_rows_for`, `store.splice`, `refresh_conflicts`
and the aggregate pass, each with its own `elapsed_us`, plus a whole-path line
carrying `path="full"`, `rows` and the total. The crate reads `REPRISE_LOG`, never
`RUST_LOG`. Also the `#[cfg(test)] #[path] mod review_page_perf_tests;`
declaration next to the existing `mod tests;`.

**`review_page_perf_tests.rs` (+215, new).**

- `review_selection_toggle_touches_only_the_toggled_album` — V-4(a). 16 albums ×
  12 rows plus the conflicts panel, sums `removed + added` from
  `store.connect_items_changed` across one album toggle, and asserts the toggled
  album's counts changed while every other album's did not. `MAX_TOGGLE_CHURN`
  is **386**, measured, not predicted (R-21); the probe `eprintln!`s the observed
  number on every run so it stays recoverable from any log.
- `review_selection_toggle_wall_clock_probe` — V-4(b). Returns immediately unless
  `REPRISE_DOCTOR_PERF_ALBUMS` is set, so it cannot join the merge gate, which
  since #463 runs every `--ignored` test in the crate unfiltered.

Both carry exactly `#[ignore = "requires a display; run via xvfb-run"]` and
**no** rule-name prefix — a `doc_*` name would drag them into
`check-ux-traceability.sh` (R-16).

The plan file gained a `### Measured profile` section holding the churn number
and the synthetic wall-clock numbers, with the real-library medians explicitly
marked pending.

---

## Evidence — separate the two columns before trusting anything

| Check | Result | Who ran it |
|---|---|---|
| Churn probe, one exact test in its own process | `observed_items=386`, `1 passed`, `0 failed`, `2526 filtered out`, zero `^test result: FAILED` | **this session, independently** |
| `check-architecture.sh` | RC 0 | **this session** |
| `check-frontend-thinness.sh` | RC 0 | **this session** |
| `check-ux-traceability.sh` | RC 0 | **this session** |
| `check-accessibility-semantics.sh` | RC 0 | **this session** |
| `check-input-parity.sh` | RC 0 | **this session** |
| `cargo fmt --all`, `clippy --workspace --all-targets -D warnings` | clean | Codex's claim only |
| Full workspace suite, `cargo audit` | clean | Codex's claim only |
| Wall-clock probe, 16 albums | median 6 516 µs, max 6 690 µs | Codex's claim only |
| `check-display-tests.sh` (full herd) | **not run at all** | — |

The independent churn run used the same environment
`scripts/check-display-tests.sh` builds for a single display test (private XDG
roots, `TMPDIR`, `dbus-run-session`, `xvfb-run`, `GSK_RENDERER=cairo`,
`GDK_BACKEND=x11`, `REPRISE_AUDIO_SINK=fakesink`). It compiled both changed
files, so "it builds" is proven even where clippy is not.

386 is exactly the draft's predicted `2 × 193`. That agreement is the reason it
was re-measured rather than accepted — and it held.

---

## The measurement that is still owed, and why it gates everything

`### B-0` of the plan ends with V-4(c): the real library, the user at the GUI,
the session at the log. **R-19's rule then decides the depth of the rest of B,
and it was fixed in advance:**

- `grouped_rows_for` dominates → the incremental path **B-2/B-3/B-4 is
  mandatory**;
- the aggregate passes and the conflicts panel dominate → **B-1 alone is the
  fix**, the incremental path is dropped from this round, and that is a result,
  not a failure;
- either way **B-1 lands**.

Nothing below B-1 may be coded before that profile is recorded in the plan file.

### The attempt on 14.08.2026 at 15:44 failed

The app was **SIGKILLed about 20 seconds after launch**, still in startup — the
last line is the MTP mount check, the Review page was never opened, and the log
holds no `DOCTOR_REVIEW_REFRESH` record at all.

It was **not** memory: no kernel OOM entry, `systemd-oomd` inactive, 16 GiB
available, `/tmp` at 32 %. The sender could not be identified from the journal.
On the machine at that moment: three Codex runs and another session's visual
acceptance harness (`acceptance/deezer-placeholder-portraits/run-accept.sh`,
started 15:40), which launches its own Reprise instance, reads the **same** source
database, and calls `kill -KILL` in its cleanup — on its own PID only, so it
should have missed us. This is unresolved. **Ask the user whether they killed it
before assuming a technical cause.**

### Three defects in the first harness, all fixed

1. **The profile lived in `/tmp`** — a 16 GB tmpfs, i.e. RAM — holding a 253 MB
   database copy while five agent runs were in flight. It now lives in
   `~/.cache/reprise-doctor-b0`.
2. **The run used a debug build.** R-19's question is whether `grouped_rows_for`
   dominates, and that compares Rust code against GTK's C code; an unoptimised
   build inflates the Rust half and would have manufactured the expensive answer
   by itself. The run is now `--release`. Verified before switching: no
   `release_max_level_*` feature exists anywhere in the workspace, so
   `tracing::debug!` survives a release build.
3. **`| tee` swallowed the app's exit status**, which is why the first failure
   read only as "Killed". The script now reports the status and names signal 137
   explicitly.

### How to run it

Harness, outside the repo so it survives the session scratchpad:

```
~/.cache/reprise-doctor-b0-harness/doctor-b0-run.sh
~/.cache/reprise-doctor-b0-harness/doctor-b0-medians.sh
```

If those are gone, the recipe is: build `--release` in the worktree; copy
`/home/marvin/.local/share/reprise/reprise.db` **including any `-wal`/`-shm`
sidecars, with Reprise fully shut down**, into an isolated profile; run

```
XDG_DATA_HOME=<p>/data XDG_CACHE_HOME=<p>/cache XDG_CONFIG_HOME=<p>/config \
XDG_STATE_HOME=<p>/state \
REPRISE_LOG=info,reprise::ui::library_doctor::review_page=debug \
REPRISE_AUDIO_SINK=fakesink ./target/release/reprise > <log> 2>&1
```

The targeted filter is deliberate: a blanket `REPRISE_LOG=debug` puts other
modules' stderr writes inside the very stages this run times. If the log ends up
without a single `DOCTOR_REVIEW_REFRESH` line, re-run with `REPRISE_LOG=debug`
and treat the timings as upper bounds.

In the window: Library Doctor → Review, pick one album with several changed rows,
uncheck and re-check its header checkbox **five times** (ten clicks, same album,
same window size, no scrolling), scan nothing, apply nothing, quit normally.

Then: per-stage medians over the last ten refreshes, and `grep -c 'DOC-9b'` for
V-4(d). The control half does not exist yet — B-0 has only one path, and
`REPRISE_DOCTOR_FULL_REFRESH` arrives with B-3. Per V-4(c) step 5, this
measurement must later be cross-checked against the finished branch's
`REPRISE_DOCTOR_FULL_REFRESH=1` arm; if the two medians differ by more than
session noise, the control arm is not the pre-fix path and the ratio must not be
reported.

**The real library will not need a new scan.** Scan 3 of 11.08.2026 is in the
database, unacknowledged, and the Review page opens straight onto it: 2 193
checked tracks, 825 proposals across 121 distinct albums, one unresolved group.
`library_doctor_state` reads `1|3|` — latest scan 3, nothing acknowledged.

---

## Decisions left open for the user

- **Land B-0 on its own?** It is correct and harmless in isolation — five debug
  lines and two ignored probes, no behaviour change — and landing it would put
  the instrumentation into `dev` so the measurement can run against any ordinary
  dev build. It was deliberately not done, because B-0 measures rather than
  fixes, and the plan expects B-0 and the rest of B to land as one strand. This
  is a judgement call, not a formality.
- **Wait for the other sessions.** While another harness drives Reprise against
  the same source database and kills Reprise processes, a GUI measurement run on
  this machine is not trustworthy even when it survives.

---

## Traps worth carrying forward

**A measurement's build profile is part of its methodology.** Handing a plan's
"time the refresh" step to a debug build would have decided R-19 by itself, in
favour of the expensive branch. Check what the comparison is *between* before
choosing the profile.

**`/tmp` here is 16 GB of RAM.** Anything that copies a real library database
into it competes with every parallel agent run for physical memory.

**`| tee` hides the exit status** of the command that matters. A run whose only
failure signature is the word "Killed" cannot tell you which signal it was.

**Codex reported the display-test measurement honestly this time** — the number
survived an independent re-run. That does not retire the rule from the previous
handover: check which harness produced a number, not just the number.

**`.pipeline-codex.md` is still tracked in `dev` despite `.gitignore`.** It rode
into this worktree with a previous run's content and a checkout-time mtime, and
Codex dirtied it again. The one-line `git rm --cached` PR from the last handover
is still not done.
