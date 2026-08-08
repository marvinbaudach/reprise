# Two gates that lie, in different directions

Two small, unrelated repairs. Both were found while diagnosing something else,
and both share a shape: a check that reports something other than the truth.

## 1. The architecture gate is red on `dev` itself

```
crates/reprise-core/src/library/tag_edit_write.rs has 824 lines;
Rust source files must stay below 800
```

Counter-checked on a clean `origin/dev` worktree on 8 August: this is the base's
own state, not any branch's doing.

Why it matters beyond the number: the size block runs **before** the dependency
and purity checks in `scripts/check-architecture.sh` and exits on failure. While
it is red, none of those later checks run at all — so the gate is not merely
complaining, it is blind.

Bring the file under 800 lines by extraction, not by deletion. Its own module
docs say it was split out of `tag_edit.rs` "purely to stay under this crate's
800-line-per-file rule; the public surface is re-exported at
`library::tag_edit` so callers never see the split" — do the same again, along
whatever seam is actually cohesive rather than at an arbitrary line. Keep the
public surface and the re-export exactly as they are; no caller may have to
change.

Two things in that file carry hard-won reasoning that must survive the move
intact, comments included: the effective-no-op skip (`TAG-5` — a track whose
tags already match must not be written, so its mtime stays untouched), and the
per-file watcher-ignore timing (the ignore window is opened immediately before
the one write it protects, never upfront for a whole batch).

Afterwards `scripts/check-architecture.sh` must exit 0 — including the
dependency and purity checks that have not been running. If they turn out to be
red too, stop and report what they say rather than fixing them here.

## 2. The app can report a version it is not

`crates/reprise-gnome/build.rs::git_short_sha()` embeds the current commit, but
the build script declares no rerun condition tied to git. Cargo therefore reuses
its cached output whenever `reprise-gnome`'s own sources are unchanged — so a
change confined to another crate produces a binary carrying the **previous**
build's commit.

Measured on 8 August: after moving from `53a9011f76` to `de59449029` (a change
in `reprise-platform-linux` only), the freshly built binary still reported
`53a9011f76`. The code was current; the label was not.

Make the build script re-run when `HEAD` moves. It must work in a **linked
worktree**, because that is how every package here is built: `.git` is a file
there, the per-worktree `HEAD` lives in the directory `git rev-parse --git-dir`
reports, and shared refs live under `git rev-parse --git-common-dir`. Emit
`cargo::rerun-if-changed` for the paths that actually decide the answer —
`HEAD` itself, and the ref file it points at when it is symbolic (a detached
`HEAD` has no ref file, and that case must not break the build).

If the git directory cannot be resolved at all — a source tarball, no git —
the build must still succeed, exactly as it does today.

## Proof

For part 1, the gate itself is the proof: `scripts/check-architecture.sh` exits
0, and `cargo test -p reprise-core` still passes with the same test count. No
test may be edited; a pure extraction that needs a test changed is not a pure
extraction.

For part 2, a test cannot easily observe cargo's rerun behaviour, so prove it by
demonstration and say so in the summary: build, note the reported commit, move
`HEAD` to another commit **without touching any `reprise-gnome` file**, rebuild,
and show the reported commit followed. Include both outputs.

## Gates

    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test -p reprise-core
    cargo test -p reprise-gnome
    scripts/check-architecture.sh

Report each exit code separately.
