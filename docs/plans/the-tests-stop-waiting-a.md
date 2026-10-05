---
slug: the-tests-stop-waiting-a
worktree: /home/marvin/Projects/reprise-the-tests-stop-waiting-a
branch: feature/the-tests-stop-waiting-a
phase: refactored
codex_session:
created: 2026-10-05
---
# The tests stop waiting — strand a: display runner

Mother plan: `docs/plans/the-tests-stop-waiting.md`. Read its "Why", "The cut"
and "Decisions from the grill" before starting. Touch only the files this
strand owns.

## Strand a — display runner

**Owns:** `scripts/check-display-tests.sh`, `scripts/tests/qa-linters.sh`,
`TESTING.md`.

Target: one cargo build per script invocation, then per test one `Xvfb` that
reports its own readiness and one direct test-binary exec. Locally the
rule-named merge-gate stage drops from ~40 min (`DISPLAY_TEST_JOBS=1`) to an
estimated 8–12 min **without raising `DISPLAY_TEST_JOBS`** (more workers make
animation tests falsely red under load — project memory).

Tasks:

1. **Build once.** Replace the listing at lines 55-59 and the per-test
   `cargo test` with one
   `cargo test --locked --workspace --exclude reprise-platform-linux --no-run --message-format=json`.
   - The selection is **exactly** the merge gate's "Workspace tests" line
     (`check-merge-readiness.sh:131-133`), so in the gate it is a no-op build,
     and the 74-crate feature-unification rebuild disappears. Keep it in one
     array in the script with a comment naming that gate line.
   - Extract the `executable` of the `reprise-gnome` package's `reprise` bin
     test target (`profile.test == true`) with `jq` (installed in every CI
     container and locally). Fail loudly (exit 1, named message) if zero or
     more than one executable matches, or if the build fails — the current
     process substitution swallows a failed listing.
   - List with `"$test_bin" --ignored --list` and keep the existing
     `sed -n 's/: test$//p' | sort`.
2. **Run the binary directly.** Per test, exec
   `"$test_bin" --ignored --exact "$DISPLAY_TEST"` with
   - cwd `crates/reprise-gnome` (cargo runs unit tests from the package root);
   - `CARGO_MANIFEST_DIR` and the other `CARGO_PKG_*` variables cargo sets at
     run time, if any test reads them via `std::env::var` (grep the gnome crate
     first; set only what is read);
   - the existing `passed_lines` check (`test result: ok. 1 passed;`)
     unchanged — it is what catches a stale name that runs nothing.
3. **Own Xvfb with `-displayfd`.** Replace `xvfb-run --server-num=…`:
   - start `Xvfb -displayfd <fd> -screen 0 640x480x24 -nolisten tcp` (the
     screen is `xvfb-run`'s default and geometry-dependent tests rely on it,
     e.g. `preferences_chrome_placement_tests.rs:14`);
   - read the display number from the fd with a timeout (10 s); export
     `DISPLAY=:<n>`; no Xauthority (local socket only, as today in effect);
   - Xvfb is killed and reaped in `cleanup_worker_roots` on
     `EXIT INT TERM HUP`;
   - the retry loop (`attempts=3`) retries only an Xvfb that did not report a
     display in time. A "Failed to initialize GTK" after a reported display is
     a real failure now — noisy over blind.
   - The `server_num`/`run_display_offset` band logic goes (`-displayfd` picks a
     free display itself).
   - `dbus-run-session`, the isolated XDG roots, `GDK_BACKEND=x11`,
     `WAYLAND_DISPLAY=`, `GSK_RENDERER=cairo`, `REPRISE_AUDIO_SINK=fakesink` stay
     exactly as they are.
4. **Pins.** Update `scripts/tests/qa-linters.sh:189-213`:
   - L211 `server-num` → require `-displayfd`; add `640x480x24`;
   - L212 order `'if env'` before `'dbus-run-session -- xvfb-run'` → order
     `'if env'` before `'dbus-run-session --'`;
   - add: the script's cargo selection equals the "Workspace tests" gate line's
     selection (drift guard);
   - keep every other pin; the `gate "…" --` lines of
     `check-merge-readiness.sh` are not touched (27 pinned gate lines, see the
     predecessor plan).
5. **Docs.** `TESTING.md:239-252, 309-310`: describe the runner (one build,
   own Xvfb, one process per test). `RELEASING.md:150`'s manual
   `xvfb-run -a` recipe stays valid and is not touched.
6. **Measure** before/after on the same machine and record it in the strand
   file: three rule-named tests × 5 reps, and one full `--rule-named` run
   (count, wall, `failed: 0 of N`). Run under `heavy-run`.

Verification (strand-local): `scripts/check-shell.sh`,
`scripts/tests/qa-linters.sh`, `scripts/check-display-tests.sh --list` (same
list as before, byte-identical after sort), a full
`scripts/check-display-tests.sh --rule-named` with `failed: 0`, and a
deliberately broken test name / deliberately failing test that must turn the
script red (mutation proof).

## Measurement record (task 6, 2026-10-05)

Machine shared with other agent sessions (loadavg 5–16 during the runs). Before
and after were therefore **interleaved**: per rep and per test, the BASE runner
(`e0598026a1`, a temporary copy beside the script, never committed) and the new
runner ran back to back, order alternating by rep, each with its 1-minute
loadavg logged. Each sample is one invocation of the runner on exactly one test
(`--rule-named --shard K/642`, K = 100, 300, 500), i.e. the fixed cost (cargo
check, listing) plus one test. 5 reps × 3 tests per runner. No sample compiled
anything (`Compiling` count 0 in every log); the build was warm for both.

| Sample (median of 15, loadavg median 7.8) | BASE | new |
|---|---|---|
| one test, whole runner invocation | **3.98 s** (min 3.66) | **0.78 s** (min 0.59, max 1.56) |
| `--list` of one test (fixed cost) | 0.42 s | 0.49 s |
| per test, K = 100 / 300 / 500 | 3.88 / 3.79 / 4.35 s | 0.75 / 0.78 / 0.84 s |

Two of the 15 BASE samples took **143 s** instead of 4 s (rep 3 of K = 300, rep 2
of K = 500). In both logs the first attempt sat 135 s in GTK initialisation and ended in
`Failed to initialize GTK` (a display number that did not serve), after which
the old runner's retry on `:1099` passed in 0.4 s. The new runner has no such
sample: it waits for the display number Xvfb itself reports. The median ignores them; the BASE mean is 22.5 s because of them,
the new mean is 0.87 s.

Full run, new runner only, `--rule-named`: **642 tests, 331 s wall (5 min 31 s)**,
`passed: 642`, `failed: 0 of 642`, no `Xvfb reported no display` retry, loadavg
5.32 at the start and 5.93 at the end. The before figure is the plan's ~40 min
(`DISPLAY_TEST_JOBS=1`), not re-measured. `/tmp` did not grow over the run
(70 % before, 69 % after; the count of `tmp.*` directories unchanged), so the
per-worker `TMPDIR` cleanup still reaches every directory, `tmp_home` now
included in the explicit tidy-up.

The first `cargo test --workspace --no-run` in a fresh worktree took 24 min
under that load (cold `target/`, pre-seeded copy notwithstanding); it is paid
once, as in the plan.

### Mutation and fault proofs

| What was broken | Result |
|---|---|
| `--exact` name made stale (`${DISPLAY_TEST}_stale`) | `running 0 tests`, `display test matched no executing test binary`, `failed: 1 of 1`, exit 1 |
| a `panic!` at the top of `doc_2c_the_running_page_offers_cancel_and_nothing_else` | `FAILED`, `failed: 1 of 1`, exit 1 |
| `Xvfb` replaced (via `PATH`) by one that never writes to `-displayfd` | three attempts (`Xvfb reported no display within 10s (attempt n of 3)`), `failed: 1 of 1`, exit 1, 3 starts, no process left behind |
| `Xvfb` that reports display `:54321`, which nobody serves, on a test that panics on GTK init failure | one start, `Failed to initialize GTK` in the test's own output, `failed: 1 of 1`, exit 1, **no retry** |
| `cargo` replaced by one that fails | `building the workspace test binaries failed`, exit 1 |
| `jq` finding no binary / two binaries | `expected exactly one reprise-gnome test binary, found 0` / `found 2`, exit 1 |
| the "Workspace tests" gate line given one more `--exclude` | `scripts/tests/qa-linters.sh` exit 1: the selections must stay equal |

A control run with the real Xvfb on the same test passed.

### Deviations from the plan text

- The build uses `--message-format=json-render-diagnostics`, not
  `--message-format=json`: the latter buries compiler errors in the JSON stream
  on stdout, so a failed build would show nothing. The artifact messages `jq`
  reads are identical.
- `TESTING.md:239-252` needed no change (it lists the commands to run, which are
  unchanged); the runner is described in "Isolated GTK and desktop tests".
- The old runner's explicit cleanup omitted `tmp_home`; the rewritten line now
  includes it.
- The `reprise` test binary is found by package manifest path, bin name,
  `profile.test`, not by package name alone.
- No `CARGO_*` variable is set at run time: every use under `crates/` is a
  compile-time `env!` (and `build.rs`'s `OUT_DIR`).
