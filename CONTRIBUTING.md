# Contributing

Thanks for helping make Reprise a better native music player for GNOME. The
project welcomes focused bug fixes, tests, documentation, accessibility work,
and discussed features.

## Choose an entry point

- `reprise-core` owns portable library, queue, scanner, playlist, and settings
  behavior.
- `reprise-gnome` owns GTK4/libadwaita presentation and interaction.
- `reprise-platform-linux` owns GStreamer, MPRIS, MTP, Trash, and other Linux
  adapters.
- `reprise-view` owns presentation-neutral state shared by native frontends.

Open an issue before starting anything beyond a small fix so the intended
behavior and module boundary are clear. The [agent runbook](AGENTS.md) records
the repository's automation and safety constraints for contributors using
coding agents.

## Build the desktop app

Install the dependencies listed in [README.md](README.md), then build with
Cargo or Meson:

```sh
cargo build --locked --workspace
meson setup _build
meson compile -C _build
```

Flatpak development is described in [flatpak/README.md](flatpak/README.md).

## Make a change

Start a focused branch from `dev`, add a failing test for the behavior, make
the smallest change that passes it, and keep unrelated edits out of the diff.
The [UX rulebook](docs/ux-rules.md) is the source of truth for interaction and
accessibility behavior; its introduction explains how active and proposed
rules are changed.

Before opening a pull request, run the complete local gate:

```sh
MERGE_READINESS_BASE_REF=origin/dev scripts/check-merge-readiness.sh
```

The script owns the current gate list, so this document does not duplicate a
count that will drift. Pull requests target `dev` and are squash-merged; the
complete workflow is documented in
[docs/agents/branching.md](docs/agents/branching.md).

## Commit and pull-request titles

Use a short narrative title that says what changed in the product or project,
for example `The queue keeps its place after filtering` or
`The Flatpak stops shipping the worker`. Keep the body for the reason,
verification, and relevant limitations. Do not add tool-attribution footers.

## Code of Conduct

Participation is governed by [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
