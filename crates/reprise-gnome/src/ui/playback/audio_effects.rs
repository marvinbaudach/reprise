//! Keeps persisted playback-effect settings and the platform player in sync.

use std::rc::Rc;

use reprise_core::db::Db;
use reprise_core::library::{audio_effect_settings, settings};
use reprise_core::playback::{AudioEffects, PlaybackBackend, PlaybackError};

use super::player_controller::PlayerController;

pub(in crate::ui) fn stored(db: &Db) -> AudioEffects {
    audio_effect_settings::load(db)
}

pub(in crate::ui) fn persist(db: &Db, effects: &AudioEffects) -> Result<(), rusqlite::Error> {
    audio_effect_settings::store(db, effects)
}

pub(in crate::ui) fn apply_initial(player: &dyn PlaybackBackend, conn: &Rc<Db>) -> AudioEffects {
    let requested = stored(conn);
    if player.set_audio_effects(requested.clone()).is_ok() {
        return requested;
    }

    tracing::warn!("stored audio effects are unavailable; falling back to disabled effects");
    let fallback = AudioEffects::default();
    if let Err(error) = player.set_audio_effects(fallback.clone()) {
        tracing::warn!(%error, "could not explicitly restore disabled audio effects");
    }
    let conn = &conn;
    if let Err(error) = settings::set_equalizer_enabled(conn, false) {
        tracing::warn!(%error, "could not persist equalizer fallback");
    }
    if let Err(error) =
        settings::set_replay_gain_mode(conn, reprise_core::library::settings::ReplayGainMode::Off)
    {
        tracing::warn!(%error, "could not persist ReplayGain fallback");
    }
    fallback
}

impl PlayerController {
    pub(in crate::ui) fn set_audio_effects(
        &self,
        effects: AudioEffects,
    ) -> Result<(), PlaybackError> {
        self.player.set_audio_effects(effects.clone())?;
        *self.active_audio_effects.borrow_mut() = effects;
        Ok(())
    }

    pub(in crate::ui) fn active_audio_effects(&self) -> AudioEffects {
        self.active_audio_effects.borrow().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::library::settings::ReplayGainMode;
    use reprise_core::playback::{PlaybackBackend, PlaybackState};
    use std::cell::RefCell;

    struct RejectingBackend {
        attempts: RefCell<Vec<AudioEffects>>,
    }

    impl PlaybackBackend for RejectingBackend {
        fn play(&self, _: &str) -> Result<(), PlaybackError> {
            Ok(())
        }

        fn play_uri(&self, _: &str) -> Result<(), PlaybackError> {
            Ok(())
        }

        fn toggle_pause(&self) -> Result<PlaybackState, PlaybackError> {
            Ok(PlaybackState::Paused)
        }

        fn seek_to(&self, _: i64) -> Result<(), PlaybackError> {
            Ok(())
        }

        fn set_volume(&self, _: f64) {}

        fn set_audio_effects(&self, effects: AudioEffects) -> Result<(), PlaybackError> {
            self.attempts.borrow_mut().push(effects.clone());
            if effects == AudioEffects::default() {
                Ok(())
            } else {
                Err(PlaybackError::Backend("effects unavailable".into()))
            }
        }

        fn stop(&self) -> Result<(), PlaybackError> {
            Ok(())
        }

        fn set_next(&self, _path: Option<&str>) {}

        fn set_transition(
            &self,
            _mode: reprise_core::library::settings::TrackTransition,
            _crossfade_seconds: u8,
        ) {
        }
    }

    #[test]
    fn unavailable_stored_effects_fall_back_without_disabling_playback() {
        let conn = Rc::new(crate::test_db::open().unwrap());
        settings::set_equalizer_enabled(&conn, true).unwrap();
        settings::set_equalizer_bands(&conn, [3.0; 10]).unwrap();
        settings::set_replay_gain_mode(&conn, ReplayGainMode::Album).unwrap();
        let backend = RejectingBackend {
            attempts: RefCell::new(Vec::new()),
        };

        assert_eq!(apply_initial(&backend, &conn), AudioEffects::default());
        assert_eq!(backend.attempts.borrow().len(), 2);
        assert!(!settings::get_equalizer_enabled(&conn));
        assert_eq!(settings::get_replay_gain_mode(&conn), ReplayGainMode::Off);
    }

    #[test]
    fn persist_round_trips_the_complete_active_effect_state() {
        let conn = crate::test_db::open().unwrap();
        let effects = AudioEffects {
            equalizer_enabled: true,
            equalizer_bands: [6.0; 10],
            replay_gain: ReplayGainMode::Track,
        };

        persist(&conn, &effects).unwrap();

        assert_eq!(stored(&conn), effects);
    }
}
