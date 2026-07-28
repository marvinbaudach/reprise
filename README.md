# Reprise

[English](README.md) · [Deutsch](README.de.md)

Reprise is a music player for people who still keep their own music files —
and for developers who want to see native desktop UX, a portable core, and
measurable systems work come together in one Rust codebase. A GTK4/libadwaita
GNOME app sits on top of a GUI-free core, with everything Linux-specific kept
behind explicit contracts.

> **Status:** active alpha. Reprise is not a public release yet.

## Why Reprise

- **Everything works locally.** Scanning large libraries, metadata, search,
  playlists, listening history, Android sync, and file safety all work without
  turning your library into a cloud account.
- **Native, not a web view.** GTK4/libadwaita shapes the GNOME experience;
  GStreamer, MPRIS, MTP, keyring, and Trash integration live in a separate
  Linux layer.
- **Checked, not promised.** Architecture, UX, accessibility, performance, and
  delivery rules are enforced by scripts and tests, not just described in the
  README.

## Architecture

![Reprise architecture: a portable Rust core, a Linux platform adapter, and a native GTK4/libadwaita frontend with enforced dependency direction.](docs/assets/reprise-architecture.svg)

| Crate | Owns | Must not own |
|---|---|---|
| `reprise-core` | Library, SQLite queries, queue semantics, scanning, playlists, settings, and platform contracts | GTK, libadwaita, GStreamer, zbus, or GLib dependencies |
| `reprise-platform-linux` | GStreamer playback and analysis, MPRIS/D-Bus, MTP, Trash, and other Linux adapters | Product UI or duplicated domain rules |
| `reprise-gnome` | GTK4/libadwaita presentation, interaction state, accessibility, and desktop composition | Productive SQL, blocking HTTP, or direct GStreamer orchestration |
| `reprise-cli` | Headless CLI over core facades: playlists, search, library summary, scan, and instrumental jobs | Any workspace crate beyond reprise-core (bar the feature-gated mpris/worker exceptions) or productive SQL |
| `reprise-mcp` | Local stdio MCP server exposing read-only library resources and capability-gated create tools to agents | Any workspace crate beyond reprise-core, productive SQL, or playback/queue/tag/delete tools |
| `reprise-stems` | Portable stem-separation backend (ML inference) for the experimental instrumental jobs | Any workspace crate beyond reprise-core, or GUI/engine coupling |

All application logic and data live in the shared engine; the platform crates
only implement the narrow contracts the core defines, and each frontend stays
native. The `reprise-cli` and `reprise-mcp` frontends run as separate processes
on the same database, and a change-log notifier shows their edits live in a
running GTK app, without a restart. `scripts/check-architecture.sh` enforces
the dependency direction, core purity, source-size limits, and known
presentation-layer coupling traps.

## Engineering contracts

- **Every UX rule has a test.** The binding [UX rulebook](docs/ux-rules.md)
  maps each active rule to a test named after it, in Rust or CUA — covering
  keyboard, focus, accessibility, feedback, and reduced motion.
- **Large libraries stay fast and light.** The track model combines GTK widget
  virtualization with lazily loaded 200-row SQLite windows and a fixed cache
  budget. Accepted comparisons use generated 10,000- and 100,000-track
  profiles.
- **Stale async results never hit the wrong row.** Recycled rows and
  long-running workers carry generation tokens, so late covers, metadata,
  lyrics, or progress updates cannot repaint a different visible item.
- **Anything risky is opt-in.** Network modules are off by default,
  credentials go into the system keyring, files only change after a user
  action, and automated checks run on isolated profiles instead of a real
  music library.

Benchmark methods, their limits, and the accepted results are documented in
[TESTING.md](TESTING.md) and the [engineering showcase](docs/showcase.md) —
this README deliberately avoids numbers that would go stale.

## Contributing

**Pick your entry point:** pure library, scanner, queue, or playlist logic in
`reprise-core`; native interaction and accessibility in `reprise-gnome`; or
audio, desktop, and device adapters in `reprise-platform-linux`.

Start with [AGENTS.md](AGENTS.md) and the [UX rulebook](docs/ux-rules.md).
Every change starts with a failing test, respects the core boundary, and lands
via pull request through `feature → dev → main`. The goal is not more code —
it is a better music player, with evidence that each change is correct.

## Build and run

Requirements: Rust 1.92+, Meson 1.3+, Ninja, GTK 4.22+, libadwaita 1.9+,
SQLite, gettext, GStreamer 1.x with the Good Plug-ins, and GVfs with its MTP
volume monitor. Android synchronization specifically requires `lamemp3enc`
and `id3v2mux`; `scripts/check-device-sync-gstreamer.sh` verifies the complete
runtime factory set.

Install the GStreamer codec plugins needed by the files you want to play.

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

Meson builds compile the experimental stem-separation backend by default
(`-Dstem_backend=true`); pass `-Dstem_backend=false` for a core-only binary,
while the plain `cargo build` above always stays core-only.

`reprise-mcp` (the agent-facing MCP server) is not part of the Meson desktop
build above; build it directly with Cargo when you need it. Its
playback-control tools (`music_playback_control`, `music_play`) sit behind the
opt-in `mpris` feature, the same pattern as the CLI's `mpris`/`worker`
exceptions:

```sh
cargo build --locked -p reprise-mcp --release --features mpris
```

The default `cargo build -p reprise-mcp` (no extra features) needs no D-Bus
and simply leaves the playback tools out.

The Flatpak manifest targets GNOME 50 and resolves Cargo dependencies from
pinned checksums. See [flatpak/README.md](flatpak/README.md).

## Verification

The quick local check is:

```sh
cargo fmt --check
cargo clippy --locked --all-targets --workspace -- -D warnings
cargo test --locked --workspace
scripts/check-architecture.sh
scripts/check-ux-traceability.sh
```

Before a merge, a candidate runs the complete pull-request gate — warning-free
Rustdoc, a dependency audit, isolated GTK display suites, and the
accessibility and input contracts:

```sh
MERGE_READINESS_BASE_REF=origin/dev scripts/check-merge-readiness.sh --no-fetch
```

Release candidates additionally validate desktop metadata, Flatpak sources,
translations, and an optimized Meson install through `scripts/check-release.sh`.

Display tests simply fail when their private D-Bus/Xvfb/AT-SPI services are
missing — they never fall back to the live desktop or your user profile.

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
