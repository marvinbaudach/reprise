# Reprise

A native GTK4/libadwaita music player for GNOME — a spiritual successor to
Rhythmbox.

Reprise is in early development (stage 1: the audible core — playback,
library scanning, and the SQLite-backed track database). There is no UI yet;
`cargo run` currently just opens and migrates the database.

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
