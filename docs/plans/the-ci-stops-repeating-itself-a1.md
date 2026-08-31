---
slug: the-ci-stops-repeating-itself-a1
worktree: /home/marvin/Projects/reprise-the-ci-stops-repeating-itself-a1
branch: feature/the-ci-stops-repeating-itself-a1
phase: planned
codex_session:
created: 2026-08-31
---
# Strand A1 — `gate()` learns to skip, and six gates move off the critical path

Mother plan: [`the-ci-stops-repeating-itself.md`](the-ci-stops-repeating-itself.md).
**Read its "27 gate lines that must not move" section before touching anything.**

**Wave 1**, concurrent with strand `b`. Lands second, after `b`.

## File ownership

```
scripts/check-merge-readiness.sh
scripts/ci-quality.sh
.github/workflows/ci.yml
scripts/tests/qa-linters.sh
```

Strand `a2` inherits these same four files in wave 2 — that is why the two
cannot run concurrently. Touch nothing outside this list; in particular leave
`.github/scripts/**` and `scripts/check-display-tests.sh` to `a2`.

## A1.1 — teach `gate()` to skip

Implement `skipped_here`, `is_skipped()` and the modified `gate()` exactly as
given in the mother plan, plus a summary at the end of the script so a partial
run can never read as a complete one:

```bash
if (( ${#skipped_here[@]} > 0 )); then
  echo "Skipped here, covered by another CI job: ${skipped_here[*]}"
fi
```

**Hard constraint: not one line matching `^gate "` may change.** Not its text,
not its indentation. Five consumers parse these 27 lines and two of them use
regexes that disagree about leading whitespace — the mother plan has the table.

**Verify:** `git diff -- scripts/check-merge-readiness.sh | grep -c '^[-+]gate "'`
must print `0`.

## A1.2 — move six gates to `base-contracts`

`scripts/ci-quality.sh` exports the skip list before calling the gate:

```
MERGE_READINESS_SKIP_GATES=$'Shell\nProject quality\nWorktree GC\nWorktree GC schedule\nScript self-tests\nArchitecture'
```

`Shell`, `Project quality` and `Architecture` **already run in `base-contracts`
today** — pure duplication, no new step needed. Add only the two that do not:

```yaml
      - name: Verify worktree hygiene
        run: |
          scripts/tests/worktree-gc.sh
          scripts/tests/worktree-gc-schedule.sh

      - name: Run the script self-tests
        run: scripts/tests/qa-linters.sh
```

These six were chosen because **none needs a dependency `base-contracts`
lacks**: `worktree-gc*.sh` and `qa-linters.sh` need only `rg` (already
apt-installed there), and `check-architecture.sh` uses `cargo tree` — metadata
only, and cargo ships on `ubuntu-24.04`.

Deliberately **not** moved, each needing a dependency the plain runner lacks:
`AppStream` (appstreamcli, desktop-file-validate, xmllint), `Flatpak manifest`
(flatpak-builder-lint), `Runtime service install` (meson), `Device-sync
GStreamer` (gst-inspect-1.0), `Frontend thinness` (cargo-machete). Together ~51 s
— not worth an apt install each.

**Expected: −308 s (~5.1 min) from `core-suite`; `base-contracts` 1.6 → ~5.1 min
against a 15 min timeout.**

### The self-reference to get right

`qa-linters.sh` now runs *in* `base-contracts` **and** asserts the content of
`ci.yml` (lines 111, 118, 119, 139) and of `check-merge-readiness.sh` (lines
91-122). Adding the two steps above must not break its own assertions. In
particular `qa-linters.sh:139` requires the literal `DISPLAY_TEST_JOBS: 1` in
`ci.yml` — that is the Android job's setting; leave it alone.

Consider adding one assertion pinning the new mechanism, so a future edit cannot
silently delete the skip list: `require_pattern 'MERGE_READINESS_SKIP_GATES'`
against both `scripts/ci-quality.sh` and `scripts/check-merge-readiness.sh`.

## Verification

Locally, before any push:

1. `git diff -- scripts/check-merge-readiness.sh | grep -c '^[-+]gate "'` → `0`.
2. `MERGE_READINESS_SKIP_GATES=$'Shell\nArchitecture' scripts/check-merge-readiness.sh --no-fetch`
   — both announce themselves as skipped, the summary line lists exactly those
   two, and the run is otherwise identical.
3. The same script with the variable **unset** runs all 27 gates and prints no
   summary line. This is the pre-push hook's path; it must be untouched.
4. `scripts/tests/qa-linters.sh` green.
5. `cd showroom && npm test` — `gate-derivation.test.mjs`,
   `chapter-two.test.mjs` and `chapter-design.test.mjs` must all stay green;
   they are what catches a moved gate line.
6. `scripts/check-shell.sh` green (shellcheck on the modified scripts).

Before landing:

7. `gh workflow run ci.yml --ref <branch>` — a PR runs no suites here. The
   dispatch run must show `core-suite` logging the six gates as skipped, and
   `base-contracts` running worktree-gc and qa-linters, still under 8 min.

## Not in this strand

The display-tests job, the `display` route, `require-ci-results.sh`,
`check-display-tests.sh`, any cache — all of that is `a2`. Do not add
`Rule-owned display tests` to the skip list here; `a2` does that together with
the job that takes it over. Adding it now would drop 583 tests with nothing
running them.
