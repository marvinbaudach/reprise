---
slug: the-tests-stop-waiting-a
worktree: /home/marvin/Projects/reprise-the-tests-stop-waiting-a
branch: feature/the-tests-stop-waiting-a
phase: planned
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

