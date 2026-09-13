---
slug: the-flatpak-sources-follow-the-lock
worktree: /home/marvin/Projects/reprise-the-flatpak-sources-follow-the-lock
branch: feature/the-flatpak-sources-follow-the-lock
phase: reviewed
codex_session:
created: 2026-09-13
---
# The Flatpak sources follow the lock

Closes #883.

## Why

Dependabot PRs #864 (trash 5.2.6 → 5.2.7), #861 (rmcp 3.1.4 → 3.2.0) and
#766 (zvariant 5.14.0 → 5.15.0) compiled and passed their tests, but every one
of them failed the *Base and contract checks* job because
`flatpak/cargo-sources.json` was not regenerated for the new `Cargo.lock`.
`scripts/check-flatpak-cargo-sources.sh` compares the checksummed packages in
the lock against that file and fails on any drift. The three PRs were closed
unmerged; the versions on `dev` are still the old ones.

This branch replaces those three PRs with focused commits that each carry
their own regenerated Flatpak sources, so the contract check is green on every
commit.

## Facts to build on

- `trash = "5"` in `crates/reprise-platform-linux/Cargo.toml` — a caret
  requirement, so only `Cargo.lock` moves.
- `rmcp = { version = "=3.1.4", … }` in `crates/reprise-mcp/Cargo.toml` — an
  exact pin, so the manifest line changes to `=3.2.0` as well.
- `zvariant = "5"` in `crates/reprise-runtime-protocol/Cargo.toml` — only the
  lock moves. `zbus` stays where it is unless `cargo update -p zvariant` forces
  it; do not bump `zbus` deliberately.
- The generator is on PATH as `flatpak-cargo-generator.py` (a `uv` script that
  needs network access for git sources). Invocation, from the repo root:
  `flatpak-cargo-generator.py Cargo.lock -o flatpak/cargo-sources.json`
- The contract check: `scripts/check-flatpak-cargo-sources.sh` (no arguments,
  from the repo root). It prints the missing and orphaned packages on failure.

## Tasks

Each task is one commit. Each commit leaves the tree green on its own — that
is what "focused replacements" in #883 means.

### T1 — trash 5.2.7

`cargo update -p trash --precise 5.2.7`. Regenerate the Flatpak sources. Run
the contract check. Commit as `trash 5.2.7 arrives with its Flatpak sources`.

### T2 — rmcp 3.2.0

Change the exact pin in `crates/reprise-mcp/Cargo.toml` to `=3.2.0`, then
`cargo update -p rmcp --precise 3.2.0`. If rmcp 3.2.0 changed an API the MCP
crate uses, make the smallest adaptation that keeps behaviour identical — the
existing tests in `crates/reprise-mcp` are the contract and must pass
unchanged. Regenerate the Flatpak sources, run the contract check, commit as
`rmcp 3.2.0 arrives with its Flatpak sources`.

### T3 — zvariant 5.15.0

`cargo update -p zvariant --precise 5.15.0`. Regenerate, check, commit as
`zvariant 5.15.0 arrives with its Flatpak sources`.

### T4 — verification

After T3, run the workspace gates once (see *Verification scope*), and also
`scripts/check-flatpak-manifest.sh`. Nothing else in the tree changes. If a
gate is red for a reason unrelated to these three crates, say so in the
summary instead of fixing unrelated code.

## Out of scope

- Any other dependency. #884 (gtk-rs family) and #885 (quick-xml 0.42) are
  separate branches.
- Formatting or reordering `flatpak/cargo-sources.json` beyond what the
  generator emits.

## Parallelität

Not cut. All three tasks rewrite `Cargo.lock` and `flatpak/cargo-sources.json`,
so there is no disjoint file group; the commits are sequential by design.
