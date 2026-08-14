# Handover — strand B, B-0 landed and measured, 14.08.2026 19:30

**Status: B-0 is on `dev`. The measurement it existed for is done and it moved
the target for the rest of the strand. B-1 through B-5 are unstarted.**

| | |
|---|---|
| Landed | `f24366b269` — "The doctor review refresh can be measured, and the numbers move strand B's target" (#487) |
| Mother plan | `docs/plans/doctor-review-selection-and-refresh.md` |
| Strand A | merged `604677322e` (#478) |
| Strand B plan | `docs/plans/doctor-review-selection-and-refresh-b.md` — `phase: coded`, **refers to B-0 only**, the status block at the top says so |
| Worktree / branch B | removed; branch deleted |

---

## The measurement — this is the part that matters

Real library, release build of B-0, isolated XDG profile on a copy of the 243 MB
database, scan 3, 330 visible rows, 28 recorded refreshes. The user drove the
GUI; the session read the log.

| Stage | Median | Range over 20 refreshes | Trend |
|---|---|---|---|
| `grouped_rows_for` | 9,150 µs | 5–17 ms | flat |
| `store.splice` | 241,276 µs | **167 ms → 2,313 ms** | rises every refresh |
| `refresh_conflicts` | 424 µs | 0–3 ms | flat |
| `aggregate` | 1,987 µs | 2–6 ms | flat |

**R-19's rule does not decide this.** It asked whether `grouped_rows_for` or the
aggregate passes plus the conflicts panel dominate. Neither: 3.6 %, 0.8 % and
0.2 % against `store.splice` at ~96 %. **B-2/B-3/B-4 is mandatory** — but on a
stronger footing than predicted, because the stage the incremental path removes
*is* the cost. B-1 alone would have addressed under 1 %.

**The cost is not constant.** Twenty toggles of the same album against an
unchanged 330-row list: 175 → 2,328 ms. Eight further toggles: 2,090–4,600 ms.
Only `store.splice` grows; every Rust stage stays flat. Two confounders were
checked and neither explains it:

- *Host load* fell from 10.5 to 5.8 while the cost rose — the trend runs against
  the load.
- *Retained heap* — RSS sampled twice a second swung 880 → 588 → **264** → 714 MB.
  It recovers, so this is not a simple unbounded object leak in the process.

**It is not only latency.** Two screenshots taken before and after one toggle,
identical except for the click, show the list displaced by one row (~65 px) with
no scrolling. The full splice loses the scroll anchor. The user also reported
scrolling itself becoming slow in the same session. This belongs in B's
user-visible case, not only in its numbers.

---

## The open question, and why the obvious tools cannot answer it

The remaining suspect for the growth is the **accessibility tree**: every splice
replaces all row accessibles, and accumulation there would show as rising cost
without rising RSS. `GTK_A11Y=none` is the one-variable control arm.

**cua-driver cannot run it.** cua drives the app *through* the AT-SPI tree that
the control arm removes — method and control exclude each other. (Also: this
app's ColumnView rows report flattened AT-SPI frames, pixel clicks have killed
the driver before, and a cua session locks the desktop scope for other sessions.)

**The synthetic probe cannot run it either, and this is the finding to carry
forward.** `review_selection_toggle_wall_clock_probe` was run headless under
`xvfb-run` + `dbus-run-session` with `REPRISE_DOCTOR_PERF_ALBUMS=28` and a debug
`REPRISE_LOG`. Result: the test passed **in 0.17 s**, printed no `PERFORMANCE`
line, and the log held **zero** `DOCTOR_REVIEW_REFRESH` lines. Two separate
causes, both worth knowing before someone retries:

1. **B-0's `tracing::debug!` instrumentation is invisible in tests.** The
   subscriber is installed in the app's `main()`; a test binary never calls it.
   The per-stage numbers therefore only exist in a **real app run**. Any plan
   step that expects to read stage timings out of a test run is built on sand.
2. The probe returned early, i.e. it did not see `REPRISE_DOCTOR_PERF_ALBUMS`
   through the `xvfb-run -a dbus-run-session -- cargo test` chain. Unresolved —
   check the env propagation through that chain before blaming the probe.

Scratch state, if anyone wants to pick this up: worktree
`/home/marvin/Projects/reprise-b0-control` (detached on `f24366b269`), logs under
this session's scratchpad as `ctl-run.log` / `probe-result.log`.

---

## The measurement harness, hardened — reuse it, do not rebuild it

```
~/.cache/reprise-doctor-b0-harness/doctor-b0-run.sh      # launch + isolated profile
~/.cache/reprise-doctor-b0-harness/doctor-b0-medians.sh  # evaluation
```

Four changes were made on 14.08.2026 and all four have a reason:

- **The probe binary is copied to `~/.cache/doctor-b0/bin/dcheck`** and launched
  from there. An earlier run was SIGKILLed 20 s into startup; the user confirms
  they did not kill it. A command line containing no "reprise" defeats both
  `pkill -x reprise` and `pkill -f reprise`, which is what other sessions'
  acceptance harnesses run in their cleanup. Environment variables are not part
  of `/proc/PID/cmdline`, so the `REPRISE_*` variables do not give it away.
- **`setsid --wait`** — own session and process group, and the exit status still
  propagates whether or not setsid forks.
- **The real session bus, not a private one.** A private bus was tried and
  rejected by measurement: it logged
  `Gtk-CRITICAL: Unable to register the application … 'org.a11y.atspi.Registry':
  unit failed` and a 19 s `org.freedesktop.secrets` timeout. An unreachable a11y
  registry puts failing D-Bus calls inside widget construction — inside the very
  stages the run times. `DOCTOR_PRIVATE_BUS=1` still switches it on.
- **A witness log** samples neighbouring processes every 2 s.

---

## Traps found in this run

**The tracing writer colours field *names*, in files too.** The raw log holds
`\e[3mstage\e[0m\e[2m=\e[0m"store.splice"`, so `grep -F 'stage="store.splice"'`
and `grep -cF 'path="full"'` both return zero against a log with 55 stage lines
in it. This bit three times in one afternoon: the inherited `doctor-b0-medians.sh`
reported "no samples" for every stage, the harness's own completion check
reported zero refreshes, and a live log watcher stayed silent through 28
refreshes. Both scripts are fixed (they strip escapes first); the general rule is
to `sed 's/\x1b\[[0-9;]*m//g'` before evaluating, and to count the **message
text** (`DOCTOR_REVIEW_REFRESH path`) rather than a field, because the message
carries no escapes.

**A GUI measurement run needs the user's own Reprise closed — and their build
checked.** `~/.local/bin/reprise` was built 14.08. 08:47, strand A landed 13:34.
The user's "I still cannot set any toggle" was A-1 in their own build, five hours
older than the fix. Check the installed binary's mtime against the fix commit
before diagnosing a report as a live bug.

**An album with nothing selectable is not a broken checkbox.** Scan 3 has 825
review rows over 122 albums; 433 rows are Ready and **38 albums have no
selectable row at all**. On those, A-1's fix correctly renders the header
checkbox insensitive. The whole split is derivable from the database without the
app — `library_doctor_scan_tracks` joined against `tracks` on
path/mtime/size/device/inode reproduces `store::stale_flags` exactly, and its
Ready count matched the UI's "433 fixes ready" on the nose. Use that to pick a
test album instead of hunting in the UI.

**`gh pr merge --delete-branch` fails after a successful merge** when another
worktree holds `dev`: `fatal: 'dev' is already used by worktree at …`. The merge
itself is already done at that point — check `gh pr view --json state` before
believing the error.

---

## Loose ends

- **The review page has no search.** No `SearchEntry`, no `<primary>f` binding —
  433 fixes across 106 albums with three category tabs and no way to jump to a
  name. Not this strand's business; worth its own small plan.
- The a11y control arm, unrun — see above for why both obvious routes fail.
- `git rm --cached .pipeline-codex.md` — still tracked in `dev` despite
  `.gitignore`, still riding into every fresh worktree. Third handover in a row
  that mentions it.
- `check-display-tests.sh` and `check-ux-traceability.sh` still appear in no
  GitHub workflow. Both ran green here by hand (689/689, 0 unmatched).
- The seven remaining §J cross-checks, due after the rest of B lands.
