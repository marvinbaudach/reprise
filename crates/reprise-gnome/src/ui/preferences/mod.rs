pub(in crate::ui) mod preference_appearance;
pub(in crate::ui) mod preference_choice_cards;
pub(in crate::ui) mod preference_concerts;
pub(in crate::ui) mod preference_dependencies;
pub(in crate::ui) mod preference_effects;
pub(in crate::ui) mod preference_experimental;
pub(in crate::ui) mod preference_lastfm;
pub(in crate::ui) mod preference_layout;
pub(in crate::ui) mod preference_library;
pub(in crate::ui) mod preference_library_doctor;
pub(in crate::ui) mod preference_listenbrainz;
pub(in crate::ui) mod preference_new_releases;
pub(in crate::ui) mod preference_playback;
pub(in crate::ui) mod preference_plugins;
pub(in crate::ui) mod preference_podcasts;
pub(in crate::ui) mod preference_radio;
pub(in crate::ui) mod preference_rhythmbox;
pub(in crate::ui) mod preference_scrobbling;
pub(in crate::ui) mod preference_sync;
pub(in crate::ui) mod preference_visual_strings;
pub(in crate::ui) mod preference_window_decorations;
pub(in crate::ui) mod preferences_window;
#[path = "preferences.rs"]
mod surface;

#[allow(unused_imports)]
use super::*;
pub(in crate::ui) use surface::{action_row, replay_gain_index, PreferencesContext, SMOKE_ENV};
