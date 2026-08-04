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
    // `.ok()` here would have treated a genuine read failure exactly like "the
    // key is absent" while the unconditional DELETE below ran either way — a
    // stored value could be destroyed without ever having been looked at, with
    // no error and no trace. Only "no rows" means absent.
    let legacy = match transaction.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        [LEGACY_EQUALIZER_BANDS_KEY],
        |row| row.get::<_, String>(0),
    ) {
        Ok(value) => Some(value),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(error) => return Err(error),
    };
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

/// Falls back to the flat preset for a value this migration cannot read — and
/// says so, the way the reader it replaced did. A silent fallback turns a
/// corrupt setting into a legitimate-looking flat curve with nothing left to
/// explain where the old one went.
fn parse_legacy_levels(value: &str) -> [f64; 10] {
    let levels = match value
        .split(',')
        .map(str::parse::<f64>)
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(levels) => levels,
        Err(error) => {
            tracing::warn!(%error, "invalid equalizer bands; using flat preset");
            return [0.0; 10];
        }
    };
    let count = levels.len();
    <Vec<f64> as TryInto<[f64; 10]>>::try_into(levels).unwrap_or_else(|_| {
        tracing::warn!(count, "wrong equalizer band count; using flat preset");
        [0.0; 10]
    })
}
