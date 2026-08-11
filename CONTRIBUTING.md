# Contributing

Thanks for looking. Reprise is a GTK4 music player for GNOME, written in Rust.

## Before you start

Open an issue describing what you want to change. For anything beyond a small
fix, agreeing on the approach first saves both of us work.

## Building

```sh
meson setup build
meson compile -C build
```

Running from a Flatpak sandbox is described in `flatpak/README.md`.

## What has to pass

Run this before opening a pull request:

```sh
scripts/check-merge-readiness.sh
```

It runs the project's chain of gates, including `cargo fmt --check`, clippy with
`-D warnings`, the workspace test suite, a dependency audit, an architecture
lint, a frontend thinness budget, accessibility semantics, input parity, and
UX rule traceability.

## UX rules

`docs/ux-rules.md` is binding. For `[active]` rules, deviating from a rule is a
bug. If you hit a case no rule covers, add a rule rather than deciding locally:
append a `[planned]` draft with the next free ID in the affected section and
mark it `<!-- REVIEW: rule proposal -->`.

Every `[active]` rule needs a test carrying its ID in the name, for example
`fn play_1a_resumes_after_seek()`. The traceability gate enforces this.

## Commit messages

`<type>: <description>`, where type is one of feat, fix, refactor, docs, test,
chore, perf, ci.

## Code of Conduct

Participation is governed by [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
