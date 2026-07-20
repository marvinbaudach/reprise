//! reprise-core: the cross-platform, GUI-free engine behind Reprise
//! (Transmission model — one core, multiple native frontends). Everything
//! here compiles from cross-platform crates only: no gtk4/libadwaita, no
//! gstreamer, no zbus, not even glib (`cargo tree -p reprise-core` is the
//! enforced proof). A frontend consumes: `db` (open/migrate/default_path);
//! `library` (scanner, watcher, playlists, m3u, settings, stats); the
//! `queries` and `view_source` windowed query layer; `queue` (playback
//! order engine); `format`; and the platform contracts — `playback`
//! (`PlaybackBackend` trait plus `PlayerEvent`) and `media_integration`
//! (`MediaIntegrationHandles` plus state/command types), plus `waveform`
//! (`WaveformBackend`) and `fingerprint` (`FingerprintBackend`) — whose concrete
//! implementations live in per-OS platform crates (Linux: GStreamer and
//! MPRIS in `reprise-platform-linux`).

pub mod artist_news;
pub mod artist_portrait;
pub mod audio_analysis;
pub mod browser;
pub mod cover;
pub mod cover_download;
pub mod db;
mod db_library_doctor;
mod db_library_doctor_remote;
mod db_listen_history;
mod db_tag_write_jobs;
pub mod device_sync;
pub mod fingerprint;
pub mod format;
pub mod library;
pub use library::library_doctor;
pub mod lyrics;
pub mod media_integration;
pub mod models;
pub mod modules;
pub mod musicbrainz;
pub mod playback;
pub mod queries;
pub mod queue;
pub mod scrobbling;
pub mod sound_profile;
pub mod up_next;
pub mod view_source;
pub mod waveform;

#[cfg(test)]
mod artist_news_tests;
#[cfg(test)]
mod fingerprint_tests;
#[cfg(test)]
mod lyrics_tests;
