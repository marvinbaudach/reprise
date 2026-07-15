//! Keeps persisted playback-effect settings and the platform player in sync.

use std::cell::RefCell;
use std::rc::Rc;

use reprise_core::library::settings;
use reprise_core::playback::{AudioEffects, PlaybackBackend, PlaybackError};
use rusqlite::Connection;

use super::player_controller::PlayerController;

pub(super) fn stored(conn: &Connection) -> AudioEffects {
    AudioEffects {
        equalizer_enabled: settings::get_equalizer_enabled(conn),
        equalizer_bands: settings::get_equalizer_bands(conn),
        replay_gain: settings::get_replay_gain_mode(conn),
    }
}

pub(super) fn persist(conn: &Connection, effects: &AudioEffects) -> Result<(), rusqlite::Error> {
    let transaction = conn.unchecked_transaction()?;
    settings::set_equalizer_enabled(&transaction, effects.equalizer_enabled)?;
    settings::set_equalizer_bands(&transaction, effects.equalizer_bands)?;
    settings::set_replay_gain_mode(&transaction, effects.replay_gain)?;
    transaction.commit()
}

pub(super) fn apply_initial(
    player: &dyn PlaybackBackend,
    conn: &Rc<RefCell<Connection>>,
) -> AudioEffects {
    let requested = {
        let conn = conn.borrow();
        stored(&conn)
    };
    if player.set_audio_effects(requested.clone()).is_ok() {
        return requested;
    }

    tracing::warn!("stored audio effects are unavailable; falling back to disabled effects");
    let fallback = AudioEffects::default();
    if let Err(error) = player.set_audio_effects(fallback.clone()) {
        tracing::warn!(%error, "could not explicitly restore disabled audio effects");
    }
    let conn = conn.borrow();
    if let Err(error) = settings::set_equalizer_enabled(&conn, false) {
        tracing::warn!(%error, "could not persist equalizer fallback");
    }
    if let Err(error) =
        settings::set_replay_gain_mode(&conn, reprise_core::library::settings::ReplayGainMode::Off)
    {
        tracing::warn!(%error, "could not persist ReplayGain fallback");
    }
    fallback
}

impl PlayerController {
    pub(super) fn set_audio_effects(&self, effects: AudioEffects) -> Result<(), PlaybackError> {
        self.player.set_audio_effects(effects.clone())?;
        *self.active_audio_effects.borrow_mut() = effects;
        Ok(())
    }

    pub(super) fn active_audio_effects(&self) -> AudioEffects {
        self.active_audio_effects.borrow().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::library::settings::ReplayGainMode;
    use reprise_core::playback::{PlaybackBackend, PlaybackState};

    struct RejectingBackend {
        attempts: RefCell<Vec<AudioEffects>>,
    }

    impl PlaybackBackend for RejectingBackend {
        fn play(&self, _: &str) -> Result<(), PlaybackError> {
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
    }

    #[test]
    fn unavailable_stored_effects_fall_back_without_disabling_playback() {
        let conn = Rc::new(RefCell::new(reprise_core::db::open(None).unwrap()));
        reprise_core::db::migrate(&conn.borrow()).unwrap();
        settings::set_equalizer_enabled(&conn.borrow(), true).unwrap();
        settings::set_equalizer_bands(&conn.borrow(), [3.0; 10]).unwrap();
        settings::set_replay_gain_mode(&conn.borrow(), ReplayGainMode::Album).unwrap();
        let backend = RejectingBackend {
            attempts: RefCell::new(Vec::new()),
        };

        assert_eq!(apply_initial(&backend, &conn), AudioEffects::default());
        assert_eq!(backend.attempts.borrow().len(), 2);
        assert!(!settings::get_equalizer_enabled(&conn.borrow()));
        assert_eq!(
            settings::get_replay_gain_mode(&conn.borrow()),
            ReplayGainMode::Off
        );
    }

    #[test]
    fn persist_round_trips_the_complete_active_effect_state() {
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        let effects = AudioEffects {
            equalizer_enabled: true,
            equalizer_bands: [6.0; 10],
            replay_gain: ReplayGainMode::Track,
        };

        persist(&conn, &effects).unwrap();

        assert_eq!(stored(&conn), effects);
    }
}
