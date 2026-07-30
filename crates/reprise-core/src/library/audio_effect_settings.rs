//! Atomic persistence facade for the complete playback-effects state.

use crate::db::Db;
use crate::playback::AudioEffects;

use super::settings;

pub fn load(db: &Db) -> AudioEffects {
    let conn = db.conn();
    AudioEffects {
        equalizer_enabled: settings::get_equalizer_enabled_in(conn),
        equalizer_bands: settings::get_equalizer_bands_in(conn),
        replay_gain: settings::get_replay_gain_mode_in(conn),
    }
}

pub fn store(db: &Db, effects: &AudioEffects) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    let transaction = conn.unchecked_transaction()?;
    settings::set_equalizer_enabled_in(&transaction, effects.equalizer_enabled)?;
    settings::set_equalizer_bands_in(&transaction, effects.equalizer_bands)?;
    settings::set_replay_gain_mode_in(&transaction, effects.replay_gain)?;
    transaction.commit()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::settings::ReplayGainMode;

    #[test]
    fn complete_effect_state_round_trips_atomically() {
        let db = Db::open_in_memory().unwrap();
        let expected = AudioEffects {
            equalizer_enabled: true,
            equalizer_bands: [6.0; 10],
            replay_gain: ReplayGainMode::Track,
        };

        store(&db, &expected).unwrap();

        assert_eq!(load(&db), expected);
    }
}
