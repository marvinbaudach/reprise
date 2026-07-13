# Reprise

Reprise is a native GTK4/libadwaita music player for GNOME and a focused,
modern successor to Rhythmbox. Version 0.1.0 is feature-complete and locally
release-ready; it has not yet been published to a public source host or
Flathub.

## Features

- Fast, windowed column view for large local libraries, with search,
  Genre/Artist/Album browsing, an editable persistent column layout, ratings,
  play counts, missing-file and import-error views.
- Incremental background scanning, live folder watching, and move detection so
  renamed files keep ratings, play counts, playlist membership, and added dates.
- GStreamer playback with seek, volume, queue, shuffle, repeat, previous/next,
  automatic advance, safe skipping of missing or unplayable tracks, a live
  ten-band equalizer with presets, and track/album ReplayGain.
- GNOME MPRIS integration for media keys, quick settings, notifications, and
  lock-screen controls.
- Manual and smart playlists, multi-select actions, drag and drop, queue
  reordering, and M3U/M3U8 import and export.
- Embedded, folder, and cached online album covers plus a full Now Playing view.
- Optional ListenBrainz scrobbling with playing-now updates, durable offline
  delivery, and credentials stored in the system keyring.
- Optional Last.fm scrobbling with bring-your-own API credentials, browser
  authorization, and an independent durable offline queue.
- Multi-track tag editing that writes only fields explicitly changed by the
  user, confirmed database-only removal, and confirmed move to Trash.
- First-run setup and validated session restore for window, view, filters,
  sorting, and exact queue state, including a default-off import offer when
  Rhythmbox settings are detected. Restoring a session never starts playback.
- A compact minimal-player window plus native preferences for appearance,
  player-bar placement, sidebar/status visibility, list density, column layout,
  library actions, optional modules, and playback effects. Changes persist and
  apply immediately except MPRIS, whose restart requirement is shown explicitly.
- Complete English source UI and German gettext translation.

Reprise scans `mp3`, `flac`, `ogg`, `opus`, `m4a`, `aac`, and `wav` files.
Actual decoding is provided by the installed GStreamer runtime and its codec
plugins, so format support can vary by distribution.

## Privacy and file safety

The library database and settings stay on the local machine. Reprise does not
include telemetry. It reads music files during scanning and playback and changes
them only after an explicit tag-edit action. Removing a track from the library
does not remove its file; moving a file to Trash always requires confirmation
and has no permanent-delete fallback.

Online cover lookup is disabled by default. When enabled explicitly, Reprise
sends album/artist metadata queries to MusicBrainz and downloads matching images
from Cover Art Archive.

ListenBrainz scrobbling is also disabled by default. After you connect an account,
Reprise sends artist, title, optional release, duration, and the listen start time
to ListenBrainz; playing-now updates omit the start time. The user token is stored
only in the system keyring (or the encrypted Secret-Portal-backed store in Flatpak),
never in the library database or logs. Completed listens wait in a local FIFO queue
while offline and are removed only after ListenBrainz accepts them. Disabling the
module stops transmission but keeps pending listens locally; Disconnect removes
both the keyring token and that ListenBrainz queue. Reprise sends no other telemetry.

Last.fm scrobbling is independently disabled by default. Enabling it requires API
credentials for a Last.fm desktop application; no project-wide API key is embedded
in Reprise or committed to this repository. Reprise opens Last.fm authorization only
after an explicit user action, then keeps the API key, shared secret, account name,
and session key together in the system keyring. It transmits artist, title, optional
release, duration, and listen start time for completed tracks; playing-now updates
omit the start time. Its offline FIFO is separate from ListenBrainz, and Disconnect
removes only the Last.fm keyring item and Last.fm queue.

## Requirements

- Rust stable (edition 2021)
- Meson 1.3+ and Ninja
- GTK 4.22+ and libadwaita 1.9+
- GStreamer 1.x and codec plugins for the formats you use
- SQLite, gettext, and standard GNOME build tools

The Flatpak manifest supplies these through GNOME Platform/SDK 50 and the stable
Rust SDK extension.

## Build and run from source

```sh
cargo build --workspace
cargo run
cargo test --workspace
```

Logs go to stderr. Set `REPRISE_LOG=debug` for additional diagnostics.

## Install with Meson

For a per-user optimized installation:

```sh
meson setup _build --prefix="$HOME/.local" -Dprofile=release
meson compile -C _build
meson install -C _build
```

Meson installs the `reprise` binary, desktop entry, AppStream metadata, full and
symbolic icons, and gettext catalogs. Packagers can use `DESTDIR` with a `/usr`
prefix.

## Build the Flatpak

The local manifest uses GNOME 50, builds Cargo dependencies offline from pinned
checksums, and grants only display, graphics, audio, opt-in cover-network, and
the application's own MPRIS permissions.

```sh
flatpak-builder --user --install-deps-from=flathub --force-clean \
  --install /tmp/reprise-flatpak-build org.reprise.Reprise.yml
flatpak run org.reprise.Reprise
```

See [flatpak/README.md](flatpak/README.md) for dependency regeneration and the
single source substitution required before a real Flathub submission.

## Verification and releasing

Run the complete non-destructive distribution check from the repository root:

```sh
scripts/check-release.sh
```

The release checklist, manual GNOME tests, known external publication blockers,
and maintainer handoff are documented in [RELEASING.md](RELEASING.md).

Development-only `REPRISE_SMOKE_*` environment hooks support fully isolated
headless regression tests. They are not user-facing features and must never be
run against a real library database or music collection.

## Relation to Rhythmbox

Reprise follows Rhythmbox's proven local-library model: a column-based collection,
smart and manual playlists, a play queue, ratings, and strong GNOME integration.
Its scope is deliberately narrower: it does not currently provide podcasts,
internet radio, CD ripping, device sync, DAAP sharing, or a plugin ecosystem.

## License

Open-core split — engine (`reprise-core`, `reprise-platform-linux`) is **MIT**, the native GTK4 Linux app (`reprise-gnome`) is **GPL-3.0-or-later**; future macOS/Windows/mobile frontends are separate and proprietary. See [LICENSING.md](LICENSING.md).
