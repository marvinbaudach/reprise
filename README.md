# Reprise

[English](README.md) · [Deutsch](README.de.md)

Reprise is a native GTK4/libadwaita music player for GNOME. Its domain logic
lives in a portable, GUI-free Rust core; Linux playback and desktop integration
sit behind explicit platform contracts.

> **Status:** active alpha. Reprise is not a public release yet.

## Product scope

- Large local libraries with incremental scanning, move reconciliation, search,
  album and artist views, playlists, queueing, ratings, and play history.
- GStreamer playback with gapless transitions, crossfade, ReplayGain, a
  ten-band equalizer, synchronized lyrics, and optional scrobbling.
- Native GNOME integration through MPRIS, media keys, notifications, session
  restore, system keyring credentials, and confirmed Trash workflows.
- Android USB/MTP browsing and synchronization with explicit plans, progress,
  cancellation, and bounded device paths.

## Architecture

![Reprise architecture: a portable Rust core, a Linux platform adapter, and a native GTK4/libadwaita frontend with enforced dependency direction.](docs/assets/reprise-architecture.svg)

| Crate | Owns | Must not own |
|---|---|---|
| `reprise-core` | Library, SQLite queries, queue semantics, scanning, playlists, settings, and platform contracts | GTK, libadwaita, GStreamer, zbus, or GLib dependencies |
| `reprise-platform-linux` | GStreamer playback and analysis, MPRIS/D-Bus, MTP, Trash, and other Linux adapters | Product UI or duplicated domain rules |
| `reprise-gnome` | GTK4/libadwaita presentation, interaction state, accessibility, and desktop composition | Productive SQL, blocking HTTP, or direct GStreamer orchestration |

The shared engine owns behavior and data; platform crates implement narrow
contracts, while each frontend remains native. `scripts/check-architecture.sh`
enforces the dependency direction, core purity, source-size limits, and known
presentation-layer coupling hazards.

## Engineering contracts

- **Behavior is specified.** The binding [UX rulebook](docs/ux-rules.md) maps
  every active rule to a rule-named Rust or CUA test, including keyboard,
  focus, accessibility, feedback, and reduced-motion behavior.
- **Large libraries stay bounded.** The track model combines GTK widget
  virtualization with lazy 200-row SQLite windows and a fixed cache budget.
  Accepted comparisons use generated 10,000- and 100,000-track profiles.
- **Async UI work is identity-safe.** Recycled rows and long-running workers
  use generation tokens so stale covers, metadata, lyrics, and progress cannot
  repaint a different visible item.
- **Risky edges are explicit.** Network modules are opt-in, credentials use the
  system keyring, file mutation requires a user action, and automated checks
  use isolated profiles rather than a real music library.

Benchmark methods, limitations, and accepted evidence live in
[TESTING.md](TESTING.md) and the [engineering showcase](docs/showcase.md), not
as fast-drifting totals in this developer entry point.

## Build and run

Requirements:

- Rust 1.92+ (edition 2021), Meson 1.3+, and Ninja
- GTK 4.22+, libadwaita 1.9+, SQLite, gettext, and standard GNOME build tools
- GStreamer 1.x plus the codec plugins needed by the files being played
- GVfs with its MTP volume monitor for Android device access

```sh
cargo build --locked --workspace
cargo run --locked -p reprise-gnome
cargo test --locked --workspace
```

Install through Meson:

```sh
meson setup _build --prefix="$HOME/.local" -Dprofile=release
meson compile -C _build
meson install -C _build
```

The Flatpak manifest targets GNOME 50 and resolves Cargo dependencies from
pinned checksums. See [flatpak/README.md](flatpak/README.md).

## Verification

The focused local baseline is:

```sh
cargo fmt --check
cargo clippy --locked --all-targets --workspace -- -D warnings
cargo test --locked --workspace
scripts/check-architecture.sh
scripts/check-ux-traceability.sh
```

A clean merge candidate runs the complete pull-request gate, including
warning-free Rustdoc, dependency audit, isolated GTK display suites, and the
accessibility and input contracts:

```sh
MERGE_READINESS_BASE_REF=origin/dev scripts/check-merge-readiness.sh --no-fetch
```

Release candidates additionally validate desktop metadata, Flatpak sources,
translations, and an optimized Meson install through `scripts/check-release.sh`.

Display tests fail closed when private D-Bus/Xvfb/AT-SPI services are
unavailable; they never fall back to the live desktop or user profile.

## Documentation

| Document | Purpose |
|---|---|
| [AGENTS.md](AGENTS.md) | Repository workflow, safety boundaries, and required gates |
| [TESTING.md](TESTING.md) | Test layers, benchmark method, and evidence limits |
| [docs/ux-rules.md](docs/ux-rules.md) | Binding interaction and accessibility contracts |
| [docs/agents/branching.md](docs/agents/branching.md) | `feature → dev → main` pull-request flow |
| [docs/showcase.md](docs/showcase.md) | Portfolio positioning and deeper engineering evidence |
| [RELEASING.md](RELEASING.md) | Packaging and release checklist |

## License

The portable engine (`reprise-core`, `reprise-platform-linux`) is **MIT**. The
native GTK4 frontend (`reprise-gnome`) is **GPL-3.0-or-later**. See
[LICENSING.md](LICENSING.md) for the rationale and component boundaries.
