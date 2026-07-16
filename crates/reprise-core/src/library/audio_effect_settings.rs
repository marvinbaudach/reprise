//! Atomic persistence facade for the complete playback-effects state.

use rusqlite::Connection;

use crate::playback::AudioEffects;

use super::settings;

pub fn load(conn: &Connection) -> AudioEffects {
    AudioEffects {
        equalizer_enabled: settings::get_equalizer_enabled(conn),
        equalizer_bands: settings::get_equalizer_bands(conn),
        replay_gain: settings::get_replay_gain_mode(conn),
    }
}

pub fn store(conn: &Connection, effects: &AudioEffects) -> Result<(), rusqlite::Error> {
    let transaction = conn.unchecked_transaction()?;
    settings::set_equalizer_enabled(&transaction, effects.equalizer_enabled)?;
    settings::set_equalizer_bands(&transaction, effects.equalizer_bands)?;
    settings::set_replay_gain_mode(&transaction, effects.replay_gain)?;
    transaction.commit()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::settings::ReplayGainMode;

    #[test]
    fn complete_effect_state_round_trips_atomically() {
        let conn = crate::db::open_migrated(None).unwrap();
        let expected = AudioEffects {
            equalizer_enabled: true,
            equalizer_bands: [6.0; 10],
            replay_gain: ReplayGainMode::Track,
        };

        store(&conn, &expected).unwrap();

        assert_eq!(load(&conn), expected);
    }
}
