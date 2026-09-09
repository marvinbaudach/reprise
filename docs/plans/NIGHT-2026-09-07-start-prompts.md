---
slug: NIGHT-2026-09-07-start-prompts
created: 2026-09-07
---
# Start prompts — one per session

Open five sessions. Paste one block into each. Nothing else is needed; every
block is self-contained and points at its own work order.

The work orders live on the unpushed branch `chore/cleanup-2026-09-07`, so the
paths below are absolute and read straight off disk. Do not `cd` into that
worktree to work — it is a library, not a workshop.

Base for every package: `origin/dev` @ `fe89dc51ad`.

---

## Session A — Android reads leave the main thread

```
Read /home/marvin/Projects/reprise/.worktrees/cleanup-2026-09-07/docs/plans/night-a-android-reads-leave-the-main-thread.md
and execute it end to end, autonomously.

This overrides the standing rule in my CLAUDE.md that /plan, /code and /ship are
my decision. For this package you own the whole cycle: plan, code, check,
refactor, and the squash merge into dev. Do not stop to ask me whether to
proceed between phases — I will not be awake. Stop only for a condition listed
under "Stop conditions" in that document; then write your findings into it, set
phase: blocked, and stop.

Set up: git -C ~/Projects/reprise worktree add .worktrees/night-a-android-reads \
  -b feature/android-reads-leave-the-main-thread origin/dev
Then copy android/local.properties into it from ~/Projects/reprise/android/ —
it is gitignored and Gradle aborts without it.

Take a wake lock for the whole run: wake-lock acquire night-a "package A, overnight"
and release it when you finish or stop.

Four sibling packages run in parallel tonight. Touch only the files in the
document's owns: field. Do not touch the phone; nothing here needs adb.
```

---

## Session B — the scanner stops being one function

```
Read /home/marvin/Projects/reprise/.worktrees/cleanup-2026-09-07/docs/plans/night-b-the-scanner-stops-being-one-function.md
and execute it end to end, autonomously.

This overrides the standing rule in my CLAUDE.md that /plan, /code and /ship are
my decision. For this package you own the whole cycle: plan, code, check,
refactor, and the squash merge into dev. Do not stop to ask me whether to
proceed between phases — I will not be awake. Stop only for a condition listed
under "Stop conditions" in that document; then write your findings into it, set
phase: blocked, and stop.

This is the highest-risk package of the night: it touches the code that writes
my library. Stopping with a finding is a success. Landing a subtly wrong scanner
is not. If half of it decomposes cleanly and half resists, land the half and say
why the rest resisted.

Set up: git -C ~/Projects/reprise worktree add .worktrees/night-b-scanner \
  -b refactor/the-scanner-stops-being-one-function origin/dev

Take a wake lock for the whole run: wake-lock acquire night-b "package B, overnight"
and release it when you finish or stop.

Four sibling packages run in parallel tonight. Touch only the two files in the
document's owns: field — the test files are not yours, and a test that needs
editing means the refactor changed behaviour.
```

---

## Session C — the source clients share their boundary

```
Read /home/marvin/Projects/reprise/.worktrees/cleanup-2026-09-07/docs/plans/night-c-the-source-clients-share-their-boundary.md
and execute it end to end, autonomously.

This overrides the standing rule in my CLAUDE.md that /plan, /code and /ship are
my decision. For this package you own the whole cycle: plan, code, check,
refactor, and the squash merge into dev. Do not stop to ask me whether to
proceed between phases — I will not be awake. Stop only for a condition listed
under "Stop conditions" in that document; then write your findings into it, set
phase: blocked, and stop.

Read the section "Why, and what this package is NOT" before anything else. This
package is deliberately smaller than its name suggests: four identical pieces,
about 90 lines. The rate limiter, the status mapping and the fixture routing
look duplicated and are not. Widening the scope is a stop condition, not
initiative.

Set up: git -C ~/Projects/reprise worktree add .worktrees/night-c-sources-http \
  -b refactor/the-source-clients-share-their-boundary origin/dev

Take a wake lock for the whole run: wake-lock acquire night-c "package C, overnight"
and release it when you finish or stop.

Four sibling packages run in parallel tonight. Touch only the files in the
document's owns: field.
```

---

## Session D — the loaders become coroutines

```
Read /home/marvin/Projects/reprise/.worktrees/cleanup-2026-09-07/docs/plans/night-d-the-loaders-become-coroutines.md
and execute it end to end, autonomously.

This overrides the standing rule in my CLAUDE.md that /plan, /code and /ship are
my decision. For this package you own the whole cycle: plan, code, check,
refactor, and the squash merge into dev. Do not stop to ask me whether to
proceed between phases — I will not be awake. Stop only for a condition listed
under "Stop conditions" in that document; then write your findings into it, set
phase: blocked, and stop.

One hard constraint for parallel safety: every existing call site must keep
compiling untouched. A new dispatcher parameter goes in with a default, never as
a required argument — LibraryTrackRows.kt calls TrackCover and belongs to
session A tonight. ActivityPlaybackControls.kt is excluded on purpose.

Set up: git -C ~/Projects/reprise worktree add .worktrees/night-d-loaders-coroutines \
  -b refactor/the-loaders-become-coroutines origin/dev
Then copy android/local.properties into it from ~/Projects/reprise/android/ —
it is gitignored and Gradle aborts without it.

Take a wake lock for the whole run: wake-lock acquire night-d "package D, overnight"
and release it when you finish or stop.

Four sibling packages run in parallel tonight. Touch only the files in the
document's owns: field. Do not touch the phone; nothing here needs adb.
```

---

## Session E — the portrait file earns its size

```
Read /home/marvin/Projects/reprise/.worktrees/cleanup-2026-09-07/docs/plans/night-e-the-portrait-file-earns-its-size.md
and execute it end to end, autonomously.

This overrides the standing rule in my CLAUDE.md that /plan, /code and /ship are
my decision. For this package you own the whole cycle: plan, code, check,
refactor, and the squash merge into dev. Do not stop to ask me whether to
proceed between phases — I will not be awake. Stop only for a condition listed
under "Stop conditions" in that document; then write your findings into it, set
phase: blocked, and stop.

This is the smallest package of the five and it is a file move, not a
decomposition. 575 of the file's 793 lines are its test module. Move the tests to
a sibling via the #[path] idiom this repo already uses 46 times, and leave the
217 lines of production code whole. If you finish early, stop — do not go
looking for more.

Set up: git -C ~/Projects/reprise worktree add .worktrees/night-e-portrait-file \
  -b refactor/the-portrait-file-earns-its-size origin/dev
Then copy android/local.properties into it from ~/Projects/reprise/android/ —
it is gitignored and Gradle aborts without it.

Take a wake lock for the whole run: wake-lock acquire night-e "package E, overnight"
and release it when you finish or stop.

Four sibling packages run in parallel tonight. Touch only the files in the
document's owns: field.
```

---

## If you would rather they read the shared context too

`NIGHT-2026-09-07-parallel-packages.md`, in the same directory, holds the traps
common to all five: why the Android suite runs only through
`check-android-suite.sh`, which gates are two-directional ratchets, why a red
base is not theirs to fix, and why a verdict must never be read through a pipe.
Each package repeats the traps that apply to it, so the shared file is optional —
but pointing a session at it costs one line and removes a class of wasted hours.
