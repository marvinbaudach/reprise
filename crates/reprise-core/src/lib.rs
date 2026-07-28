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

pub mod agent_device_sync;
pub mod ai_conversion;
pub mod ai_jobs;
pub mod ai_promotion;
pub mod ai_staging;
pub mod artist_news;
mod artist_news_candidates;
pub mod artist_news_history;
mod artist_news_ledger;
pub mod artist_news_links;
mod artist_news_parsing;
mod artist_news_pipeline;
mod artist_news_query;
pub mod artist_news_refresh;
mod artist_news_view;
pub mod artist_portrait;
pub mod browser;
pub mod concerts;
pub mod cover;
pub mod cover_download;
pub mod db;
mod db_ai_jobs;
mod db_artist_news_fetch;
mod db_change_log;
mod db_concerts;
mod db_device_sync;
mod db_drop_audio_analysis_mix;
mod db_grandfather;
mod db_library_doctor;
mod db_library_doctor_remote;
mod db_library_exclusions;
mod db_listen_history;
mod db_mix_planner;
mod db_new_releases_history;
mod db_podcasts_radio;
mod db_recently_added;
mod db_release_discography;
mod db_sync_log;
mod db_tag_write_jobs;
pub mod device_sync;
pub mod events;
pub mod external_link;
pub mod fingerprint;
pub mod format;
mod http_body;
pub mod library;
pub use library::library_doctor;
pub mod lyrics;
pub mod media_integration;
pub mod models;
pub mod modules;
pub mod musicbrainz;
pub mod playback;
pub mod podcasts;
pub mod provenance;
pub mod queries;
pub mod queue;
pub mod radio;
pub mod scrobbling;
pub mod stem_separation;
pub mod up_next;
pub mod updates;
pub mod view_source;
pub mod visuals;
pub mod waveform;

#[cfg(test)]
mod artist_news_candidates_tests;
#[cfg(test)]
mod artist_news_parsing_tests;
#[cfg(test)]
mod artist_news_pipeline_tests;
#[cfg(test)]
mod artist_news_query_tests;
#[cfg(test)]
mod artist_news_view_tests;
#[cfg(test)]
mod fingerprint_tests;
#[cfg(test)]
mod lyrics_tests;
