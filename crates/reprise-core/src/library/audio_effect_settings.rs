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
    // An unchanged state is the common case (re-applying the stored effects).
    // Settle it before any transaction opens, so it never queues for the write
    // lock behind another writer (mirrors `set_setting_in`).
    if conn.is_autocommit() && settings::audio_effects_are_stored_in(conn, effects)? {
        return Ok(());
    }
    // IMMEDIATE: each setter reads (its dedup check) before it writes, so a
    // deferred transaction would fail the write-lock upgrade with
    // `SQLITE_BUSY_SNAPSHOT` when a rival commits in between (see
    // `events::in_txn_immediate`).
    crate::events::in_txn_immediate(conn, |conn| {
        settings::set_equalizer_enabled_in(conn, effects.equalizer_enabled)?;
        settings::set_equalizer_bands_in(conn, effects.equalizer_bands)?;
        settings::set_replay_gain_mode_in(conn, effects.replay_gain)
    })
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
