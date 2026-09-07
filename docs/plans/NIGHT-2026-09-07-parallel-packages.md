---
slug: NIGHT-2026-09-07-parallel-packages
created: 2026-09-07
base: origin/dev
phase: planned
---
# Night run 2026-09-07 — five packages, five agents, one night

Each package below is a complete work order. An agent picks one, works it end to
end without checking back, and lands it. They are cut so they can run at the
same time without fighting over files.

## How to start them

Copy-pasteable kickoff prompts, one per session, are in
`NIGHT-2026-09-07-start-prompts.md`. Each is self-contained and grants the
autonomy the packages assume — a session that starts without that grant will
stop and ask, because the standing house rule reserves `/code` and `/ship`
for the owner.

One agent per package, each in its own worktree off `origin/dev`:

```
git -C ~/Projects/reprise fetch origin dev
git -C ~/Projects/reprise worktree add .worktrees/<name> -b <branch> origin/dev
```

The `worktree:` and `branch:` fields in each package's frontmatter give the
names. Never share a worktree between two packages, and never share
`CARGO_TARGET_DIR` — Cargo takes an exclusive lock and two agents would
serialise behind it, turning a parallel night into a sequential one.

**Every agent takes its own wake lock for the length of its run**, or the
machine suspends underneath an unattended package:

```
wake-lock acquire night-<letter> "package <letter>, overnight"
…work…
wake-lock release night-<letter>
```

One lock per package, named after the package, released by the package that took
it. A closed terminal must not be able to end them, so do not rely on the
Ghostty lock. Never touch the GNOME sleep settings directly — a global value
that every session restores on exit means the last one to finish strips the
protection from everyone still running.

**No package may touch the phone.** Nothing tonight needs it, and the device
lock is a lease held across a whole measurement, not a per-command slot. An
agent that thinks it needs `adb` has misread its package.

## The packages

| # | Package | Owns | Language | Size |
| --- | --- | --- | --- | --- |
| A | [Library reads leave the main thread](night-a-android-reads-leave-the-main-thread.md) | 5 Kotlin screen/seam files | Kotlin | large |
| B | [The scanner stops being one function](night-b-the-scanner-stops-being-one-function.md) | `library/scanner*.rs` | Rust | large, highest risk |
| C | [The source clients share their boundary](night-c-the-source-clients-share-their-boundary.md) | 3 `http.rs` + one new module | Rust | small |
| D | [The loaders become coroutines](night-d-the-loaders-become-coroutines.md) | 5 Kotlin loader files | Kotlin | medium |
| E | [The portrait file earns its size](night-e-the-portrait-file-earns-its-size.md) | `artist_portrait.rs` | Rust | small |

**Why the file ownership is written into every package.** A and D are both
Android; B, C and E are all `crates/`. They do not overlap by file, which is the
only thing that matters for a clean merge. `ActivityPlaybackControls.kt` holds a
seventh executor that package D would naturally take — it is excluded from D on
purpose, because package A may need it. One leftover executor is cheaper than a
conflict nobody is awake to resolve.

If a package finds it needs a file it does not own, that is a stop condition. It
should record the finding and stop, not reach across.

## Autonomy, and where it ends

Every package authorises the agent to run plan → code → check → refactor → land
by itself, including the squash merge into `dev`. This overrides the standing
house rule that `/plan`, `/code` and `/ship` are the owner's call; the owner
asked for exactly this for tonight.

What no package authorises:

- Touching a file outside its `owns:` list.
- Landing with a red gate, or with fewer tests than the base.
- Editing a test or a fixture to make a change pass.
- Touching the real library database at `~/.local/share/reprise/reprise.db`, or
  the music under `~/Music`. Every app run must be fully isolated:
  `dbus-run-session -- xvfb-run -a env XDG_DATA_HOME=$(mktemp -d)
  XDG_CACHE_HOME=$(mktemp -d) XDG_STATE_HOME=$(mktemp -d) …`.
- Building under `/tmp`. It is a 16 GB tmpfs and a `target/` there lives in RAM.

## Traps every package will meet

**The Android suite runs only through `scripts/check-android-suite.sh`.** It
builds the FFI for the *host* and exports `LD_LIBRARY_PATH`.
`scripts/android-build.sh` looks like the same setup but builds for the device;
a raw `gradlew` after it fails 28 Robolectric tests with
`NoClassDefFoundError` at `NativeLibrary.java:325`, none of them real. A fresh
worktree also needs `android/local.properties` copied in — it is gitignored, and
Gradle aborts before compiling without it.

**Several gates are ratchets that fail in both directions.**
`check-frontend-thinness.sh` holds a `view_floor` and a dead-code allowlist;
`check-architecture.sh` holds `http_boundary_budget=16`. Removing a use lowers
the number, and the number must come down in the same commit, with the reason in
the commit message. The scripts print the value to write.

**A red base is not yours to fix.** Before debugging a failure, run the same
gate on unmodified `origin/dev` in a second worktree. `dev` gates have been red
before for reasons unrelated to the branch under test.

**Never read a verdict through a pipe.** `script | tail` reports `tail`'s exit
status, which is always 0. Redirect to a file and check `$?`.

**Prove a new guard falls.** If a package adds a test or a gate, break the thing
it guards and observe it go red before trusting it. A guard that has only ever
been green is decoration.

## Landing

Squash-merge into `dev`. The squash takes the **PR title verbatim**, so it must
read as English prose describing what changed for a user — not a refactoring
label. The repository's commits are written this way; match them.

End every commit message and PR body with the session trailer the owner's setup
provides.

## What is deliberately not in tonight's run

- **A size budget for `reprise-android-ffi`.** That is a new gate, not cleanup,
  and it should follow package E rather than race it.
- **`CoreError`** — roughly 705 public signatures still return
  `rusqlite::Error`. That is a project with its own wave in
  `docs/plans/consolidation-plan.md`, not a night's work.
- **The large `sources_http` consolidation.** Measured: about 90 of 1,297 lines
  would collapse. Package C takes exactly those and explicitly refuses the rest.
- **`reprise-mcp` in `bump-version.sh`.** It has a missing case arm like
  `reprise-cli` did, but it ships in neither the Meson install nor the Flatpak
  manifest, so whether it should move a version is a policy question for the
  owner.

Context for all of the above: `refactoring-survey-2026-09-07.findings.md`.
