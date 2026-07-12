# Reprise

A native GTK4/libadwaita music player for GNOME — a spiritual successor to
Rhythmbox.

Reprise has completed its planned application feature stages and is now in
release-readiness work. What works today:

- GTK4/libadwaita window with navigation sidebar (library, playlists,
  smart playlists, problem sources with badges), search, and status line.
- Folder scanning into a SQLite track database: background, incremental,
  per-file import-error log, and **move detection** — relocated or renamed
  files/albums keep their ratings, play counts, and added dates.
- A sortable, windowed track list that scales to large libraries.
- Playback via GStreamer with a full queue: auto-advance, shuffle, repeat
  (off/all/one), previous/next; robust against deleted or broken files
  (mark missing, toast, auto-skip — never a crash).
- Clickable star ratings and play-count tracking (50%-listened threshold).
- MPRIS integration: GNOME quick settings, lock screen, media keys.
- A bottom player bar: play/pause, seek, time display, and volume.
- Manual and smart playlists, multi-select context actions, drag and drop,
  M3U import/export, folder watching, keyboard shortcuts, album covers,
  batch tag editing, safe removal/trash, browse facets, first-run setup, and
  validated no-autoplay session restore.

## Relation to Rhythmbox

Reprise deliberately walks in Rhythmbox's footsteps: the column-based
library view, smart playlists, the play queue, and the planned
`rhythmdb.xml` import (ratings, play counts, playlists) all come from
there. Rhythmbox has served GNOME users for over two decades — Reprise
exists because its GTK3 codebase has made a GTK4 future difficult, not
because it wasn't good.

A snapshot for perspective (counted 2026-07-11, `wc -l` over the upstream
git checkouts): Rhythmbox is ~165,000 lines of C (plus ~9,000 lines of
Python plugins and ~11,000 lines of UI XML); Reprise currently implements
its core listening workflow in ~8,600 lines of Rust (plus ~4,000 lines of
tests). That is not a like-for-like comparison — Rhythmbox does far more
today (podcasts, internet radio, DAAP sharing, CD ripping, device sync, a
plugin ecosystem) — but it illustrates what a fresh start on a modern
stack (Rust, SQL-backed views, GTK4's data widgets) buys: the essentials
fit in a codebase one person can read.

## Development

Logging goes to stderr via `tracing`; the level defaults to `info` and can
be overridden with the `REPRISE_LOG` environment variable (e.g.
`REPRISE_LOG=debug cargo run`).

A few environment variables exist purely to drive the app headlessly for
development and end-to-end verification (`xvfb-run`, CI, etc.) — none are
user-facing features:

- `REPRISE_SCAN_DIR=/path/to/music` — scans the given folder into the
  database synchronously at startup, before the window is shown, since the
  folder-picker dialog can't be driven headlessly.
- `REPRISE_SMOKE_ACTIVATE=1` — activates (double-click-equivalent) the first
  track in the list shortly after the window is shown.
- `REPRISE_AUDIO_SINK=fakesink` — overrides the GStreamer audio sink, for
  running playback without a real audio device.
- `REPRISE_SMOKE_QUIT=1` — quits the application automatically after
  startup, so a headless run exits instead of hanging.

## Requirements

- Rust (stable, edition 2021)
- GTK 4.22+ and libadwaita 1.9+ development packages
- GStreamer, including the plugins needed for your audio formats (e.g.
  `gst-plugins-base`, `gst-plugins-good`)

## Build & run

```sh
cargo build
cargo run
cargo test
```

For an installable release build:

```sh
meson setup _build --prefix=/usr
meson compile -C _build
meson install -C _build
```

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
