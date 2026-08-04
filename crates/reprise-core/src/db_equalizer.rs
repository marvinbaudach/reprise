//! Schema migration from anonymous GStreamer levels to an authored curve.

use rusqlite::Connection;

use crate::equalizer::EqualizerCurve;

pub(crate) const LEGACY_EQUALIZER_BANDS_KEY: &str = "playback.equalizer_bands";
pub(crate) const EQUALIZER_CURVE_KEY: &str = "playback.equalizer_curve";

pub(crate) fn migrate_v53(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 53 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    let legacy = transaction
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            [LEGACY_EQUALIZER_BANDS_KEY],
            |row| row.get::<_, String>(0),
        )
        .ok();
    if let Some(value) = legacy {
        let curve = EqualizerCurve::from_gstreamer_levels(parse_legacy_levels(&value));
        transaction.execute(
            "INSERT OR IGNORE INTO settings (key, value) VALUES (?1, ?2)",
            rusqlite::params![EQUALIZER_CURVE_KEY, curve.serialize()],
        )?;
    }
    transaction.execute(
        "DELETE FROM settings WHERE key = ?1",
        [LEGACY_EQUALIZER_BANDS_KEY],
    )?;
    transaction.pragma_update(None, "user_version", 53)?;
    transaction.commit()
}

fn parse_legacy_levels(value: &str) -> [f64; 10] {
    value
        .split(',')
        .map(str::parse::<f64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()
        .and_then(|levels| levels.try_into().ok())
        .unwrap_or([0.0; 10])
}
