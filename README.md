# Reprise

Reprise is a native GTK4/libadwaita music player for GNOME and a focused,
modern successor to Rhythmbox. Version 0.1.0 is feature-complete and locally
release-ready; it has not yet been published to a public source host or
Flathub.

## Features

- Fast, windowed column view for large local libraries with search, chip-based
  Genre/Artist/Album filtering, editable persistent column layout, ratings,
  play counts, and missing-file/import-error views.
- Incremental scanning, live folder watching, and move detection that keeps
  ratings, play counts, playlists, and added dates across renames.
- GStreamer playback with queue, shuffle, repeat, a live ten-band equalizer
  with presets, and track/album ReplayGain.
- Full GNOME MPRIS integration: media keys, quick settings, notifications,
  lock-screen controls, and cover art.
- Manual and smart playlists, drag and drop, queue reordering, and M3U/M3U8
  import/export.
- Android USB/MTP synchronization with device browsing, drag-to-copy,
  progress, cancellation, and a strict per-device FIFO queue.
- Embedded, folder, and cached online album covers; automatically retrieved
  played-track lyrics with synchronized highlighting.
- Optional ListenBrainz and Last.fm scrobbling (independent, default-off,
  keyring-stored credentials, durable offline queues).
- Multi-track tag editing that writes only explicitly changed fields;
  database-only removal and confirmed move to Trash.
- First-run setup, validated no-autoplay session restore, and an optional
  one-time Rhythmbox import (ratings, play counts, playlists, column layout).
- Compact minimal-player window (Bar, Cover, Pill, Card layouts) plus native
  preferences for appearance, layout, density, modules, and playback effects.
- Complete English source UI and German gettext translation.

Reprise scans `mp3`, `flac`, `ogg`, `opus`, `m4a`, `aac`, and `wav`; actual
decoding depends on the installed GStreamer codec plugins.

## Architecture

Reprise keeps its reusable music engine separate from native UI and
Linux-specific integration. The frontend consumes narrow contracts, the
platform crate implements them, and an automated architecture gate keeps
`reprise-core` free of GTK, libadwaita, GStreamer, and zbus dependencies.

![Reprise architecture: the native GNOME frontend and future frontends reuse a portable core, while a separate Linux adapter provides GStreamer, MPRIS, MTP, and host integration.](docs/assets/reprise-architecture.svg)

## Privacy and file safety

The library database and settings stay on the local machine; Reprise contains
no telemetry. Music files are only written by an explicit tag-edit action.
Removing a track never deletes its file; moving to Trash always requires
confirmation and has no permanent-delete fallback.

Online access is limited to:

- **Covers** — album/artist metadata queries to MusicBrainz and conservative
  downloads from Cover Art Archive for missing covers; the library-wide
  background check is a visible, cancellable opt-in.
- **Lyrics** — after playback starts, title/artist/album/rounded duration go
  to LRCLIB; never file paths, library contents, ratings, or history. Results
  are cached locally; only played tracks are looked up.
- **ListenBrainz / Last.fm** — both default-off. They transmit artist, title,
  optional release, duration, and listen start time; credentials live only in
  the system keyring, offline listens wait in separate local FIFO queues, and
  Disconnect removes the service's keyring item and queue. Last.fm requires
  bring-your-own API credentials — no project API key is embedded.

Android sync starts only after tracks are dragged onto a phone playlist,
writes only below `Music/Reprise` on the device, and never deletes unrelated
device files.

<!-- TODO(marvin): section on the upcoming Reprise MCP server — what it
     exposes and what agents can do with it (playback control, library
     queries, playlists, …). -->

## Requirements

- Rust stable (edition 2021), Meson 1.3+ and Ninja
- GTK 4.22+, libadwaita 1.9+, GStreamer 1.x with codec plugins
- GVfs with its MTP volume monitor for Android USB synchronization
- SQLite, gettext, and standard GNOME build tools

For MTP, unlock the phone, select USB **File transfer / MTP** mode, and make
sure the device is visible in GNOME Files first.

## Build and run from source

```sh
cargo build --workspace
cargo run
cargo test --workspace
```

Logs go to stderr; set `REPRISE_LOG=debug` for diagnostics.

## Install with Meson

```sh
meson setup _build --prefix="$HOME/.local" -Dprofile=release
meson compile -C _build
meson install -C _build
```

Installs the binary, desktop entry, AppStream metadata, icons, and gettext
catalogs. Packagers can use `DESTDIR` with a `/usr` prefix.

## Build the Flatpak

The manifest uses GNOME 50, builds Cargo dependencies offline from pinned
checksums, and grants only display/audio, cover and lyrics network access,
its own MPRIS name, and the two narrow GVfs permissions for MTP devices.

```sh
flatpak-builder --user --install-deps-from=flathub --force-clean \
  --install /tmp/reprise-flatpak-build org.reprise.Reprise.yml
flatpak run org.reprise.Reprise
```

See [flatpak/README.md](flatpak/README.md) for dependency regeneration and the
source substitution required before a Flathub submission.

## Verification and releasing

`scripts/check-release.sh` runs the complete non-destructive distribution
check. The release checklist, manual GNOME tests, and known publication
blockers live in [RELEASING.md](RELEASING.md). Development-only
`REPRISE_SMOKE_*` hooks exist for isolated headless regression tests and must
never run against a real library.

Before merging a feature branch, run `scripts/check-merge-readiness.sh`. It
fetches the latest `origin/main` and rejects dirty or stale branches, then runs
the architecture, formatting, Clippy, Rustdoc, workspace-test, and dependency-
audit gates. `scripts/check-architecture.sh` is the faster structural linter
for file-size, crate-purity, orphan-module, gettext-source, and frontend rules.
The frontend rules prevent new per-widget CSS providers, deprecated styling,
unsafe blocks, blocking HTTP, and direct GStreamer/process coupling outside a
small documented legacy allowlist. To enforce the full readiness check
automatically before every push, enable the tracked hook once with
`scripts/install-git-hooks.sh`. Use `--no-fetch` only when an offline or CI
environment has already refreshed `origin/main`.

## Relation to Rhythmbox

Reprise follows Rhythmbox's proven local-library model with strong GNOME
integration, but deliberately narrower scope: no podcasts, internet radio,
CD ripping, DAAP sharing, or plugin ecosystem.

## License

Open-core split — engine (`reprise-core`, `reprise-platform-linux`) is
**MIT**, the native GTK4 Linux app (`reprise-gnome`) is **GPL-3.0-or-later**;
future macOS/Windows/mobile frontends are separate and proprietary.
See [LICENSING.md](LICENSING.md).
