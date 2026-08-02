pub mod artist_detail;
pub mod audio_effect_settings;
pub mod exclusions;
pub mod group_key;
pub(crate) mod import_errors;
pub mod lastfm_stats;
pub mod library_doctor;
pub mod listenbrainz;
pub mod m3u;
pub(crate) mod mounts;
mod playlist_delete;
pub mod playlist_membership;
pub mod playlists;
pub mod relink;
pub mod remote_stats;
pub mod rhythmbox_import;
pub mod scanner;
pub mod session;
pub mod settings;
pub mod source;
pub mod stats;
pub mod stats_period;
pub mod stats_screen;
pub mod stats_snapshot;
pub mod tag_edit;
// The single seam that opens library content for a lofty parser — see its
// module doc for why all four tag readers go through one place.
mod tag_edit_seed;
pub mod tag_edit_session;
mod tag_edit_write;
#[cfg(test)]
mod tag_edit_write_adversarial_tests;
mod tag_mutation;
mod tag_mutation_guarded;
#[cfg(test)]
mod tag_mutation_guarded_tests;
pub(crate) mod tag_probe;
pub mod tag_write_job;
pub mod trash_tracks;
pub mod watcher;
