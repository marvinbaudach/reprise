//! reprise-core: the cross-platform, GUI-free engine behind Reprise
//! (Transmission model — one core, multiple native frontends). Everything
//! here compiles from cross-platform crates only: no gtk4/libadwaita, no
//! gstreamer, no zbus, not even glib (`cargo tree -p reprise-core` is the
//! enforced proof). A frontend consumes: `db` (open/migrate/default_path);
//! `library` (scanner, watcher, playlists, m3u, settings, stats); the
//! `queries` and `view_source` windowed query layer; `queue` (playback
//! order engine); `format`; and the two platform contracts — `playback`
//! (`PlaybackBackend` trait plus `PlayerEvent`) and `media_integration`
//! (`MediaIntegrationHandles` plus state/command types) — whose concrete
//! implementations live in per-OS platform crates (Linux: GStreamer and
//! MPRIS in `reprise-platform-linux`).

pub mod db;
pub mod format;
pub mod library;
pub mod media_integration;
pub mod models;
pub mod modules;
pub mod playback;
pub mod queries;
pub mod queue;
pub mod view_source;
