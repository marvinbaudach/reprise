---
slug: frontend-performance-sweep
worktree:
branch:
phase: planned
codex_session:
created: 2026-08-24
strands: a,b,c
merge_order: a,b,c
---
# Frontend performance sweep — three surfaces, nine findings

Mother plan. The work is cut into three strands, one per frontend; each strand
file carries its own tasks, its own file ownership and its own status block.

Line numbers are against `origin/dev` at `7eaf16e4d3` (re-checked after dev moved; no Android or showroom file changed in between). The findings were first
read in a checkout 40 commits behind that and re-verified against `origin/dev`
one by one; all of them still stand there.

## Why this exists

Nothing here came from a reported problem. It came from reading the three
frontends for waste: work done for things nobody is looking at, renders nobody
asked for, caches that miss what they were built to hold.

The three surfaces are, by and large, carefully optimised already — the showroom
runs one scroll listener with rAF throttling, the Android artwork loader has
request gates and separate worker lanes, the GNOME track list has a real delta
model and the preferences dialog builds its pages on first sight. What follows
are the places where a lesson this project already learned somewhere else has
not arrived yet. In five of the nine cases the correct pattern exists in this
repository, and copying it beats inventing one.

## The rule this plan runs under

**The big five must produce a number: A1, A2, A3, B1, B2, B3 and C1.** Each
carries the measurement that decides it. A task whose measurement comes back
flat is reverted, not shipped, and the branch says so.

**C2 and C3 are hygiene** and exempt: they land if they are correct and break
nothing. Demanding a measurement campaign for hoisting `getContext` out of a
frame loop is theatre.

## Non-goals

- No new caching layers, dependencies or abstractions where an existing one can
  be pointed at a second key.
- No behaviour change anywhere. All of this is meant to be invisible outside a
  profiler.
- No opportunistic cleanup. A strand touches what its tasks name and stops.

## The cut

| Strand | Surface | Owns | Findings |
|--------|---------|------|----------|
| A | Android | `android/**` | A1, A1b, A2, A3 |
| B | GNOME | `crates/reprise-gnome/**` | B1.0, B1.1, B2, B3 |
| C | Showroom | `showroom/**` | C1, C2, C3 |

The cut is along the three frontends, which is also the file ownership: no file
and no build target is touched by two strands. There is no shared type, no
shared helper and no shared test between them.

## Merge order and scheduling

**Merge order: A, B, C.** No strand is another's precondition; the order is the
requested priority.

**Scheduling is not the same as the merge order.** Strand A runs alone first —
it holds the two findings this sweep leads with, and it is the largest single
piece of work. B and C start together once A's diff has been reviewed. The
machine carried a load of 9.7 on 8 cores with a foreign Android release build
running when this plan was written; three concurrent Codex runs with Gradle and
Cargo builds under them would make all three slower. Run B and C under
`heavy-run`.

## Cross-plan ownership

Two planned-but-unwritten plans claim files strand A changes:
`docs/plans/android-now-playing-desync-throttles-the-scene-b.md` and `-c.md`
(both `phase: planned`; `-b`'s status block points at a worktree that no longer
exists, and neither has a remote branch).

The overlap is small by construction: strand A leaves `NowPlayingSheet`,
`NowPlayingScene` and `SceneDriver` on the full `PlaybackUiState`, because those
three genuinely animate against the clock. What it adds to `PlaybackUiState.kt`
is additive.

**Strand A must write one ownership note into both plans** saying that
`PlaybackUiState.kt` now carries a second, position-free record which the
library tree reads, and that a later scene rebuild must not merge the two back
together. The repository has already paid for an ownership agreement that lived
only in a session plan: a parallel agent never read it, wrote a competing file
and merged first.

## Post-merge cross-checks

Nothing in this plan compares a file across strand boundaries — that is what
makes the cut clean. Two checks belong after the last merge:

1. **The full merge gate on the merged `dev`**, not per strand. Three
   independent green gates do not add up to one; the Rust and Android stages
   share a workspace.
2. **`docs/measurements/` gains one row per shipped finding** — before, after,
   delta, commit, date, method, in the shape that file already uses. A finding
   whose row would read "no change measured" gets reverted instead of
   documented.

## Verification stays inside each strand

Strand A runs the Android JVM suite (JDK 21; the script sets `LD_LIBRARY_PATH`
itself — setting it by hand invalidates the evidence). Strand B runs the Rust
workspace tests plus the display-owned tests under Xvfb. Strand C runs the
showroom's own suite. None of them can run the others' and none should try.
