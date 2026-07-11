# Reprise

A native GTK4/libadwaita music player for GNOME — a spiritual successor to
Rhythmbox.

Reprise is in early development (stage 1: the audible core — playback,
library scanning, and the SQLite-backed track database). Stage 1 currently
provides:

- A GTK4/libadwaita application window (header bar, search entry, status
  line).
- Folder scanning into the SQLite track database: runs in the background,
  is incremental on repeat scans, and logs per-file import errors instead of
  aborting the scan.
- A sortable track list (title, artist, album, year, length, rating) backed
  by a windowed SQL query, with live search-as-you-type filtering.
- Playback via GStreamer: double-click a track to play it.
- A bottom player bar: play/pause, seek, and volume.
- A status line reporting track count and total library duration.

There is no library-management UI beyond scan-and-browse yet (no playlists,
tagging, or editing).

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

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
