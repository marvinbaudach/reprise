//! Keeps persisted playback-effect settings and the platform player in sync.

use reprise_core::library::settings;
use reprise_core::playback::{AudioEffects, PlaybackError};
use rusqlite::Connection;

use super::player_controller::PlayerController;

pub(super) fn stored(conn: &Connection) -> AudioEffects {
    AudioEffects {
        equalizer_enabled: settings::get_equalizer_enabled(conn),
        equalizer_bands: settings::get_equalizer_bands(conn),
        replay_gain: settings::get_replay_gain_mode(conn),
    }
}

impl PlayerController {
    pub(super) fn set_audio_effects(&self, effects: AudioEffects) -> Result<(), PlaybackError> {
        self.player.set_audio_effects(effects)
    }
}
