use super::super::*;

const LEGACY_BANDS_KEY: &str = "playback.equalizer_bands";
const CURVE_KEY: &str = "playback.equalizer_curve";

#[test]
fn migration_projects_the_legacy_ten_levels_back_exactly() {
    let conn = open(None).unwrap();
    migrate_connection(&conn).unwrap();
    let legacy_levels = [-12.0, -8.5, -4.0, -1.25, 0.0, 1.5, 3.0, 6.25, 9.0, 12.0];
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)",
        rusqlite::params![
            LEGACY_BANDS_KEY,
            legacy_levels.map(|level| level.to_string()).join(","),
        ],
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 52).unwrap();

    migrate_connection(&conn).unwrap();

    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 54);
    let db = Db::from_connection(conn);
    assert_eq!(
        crate::library::settings::get_setting(&db, LEGACY_BANDS_KEY).unwrap(),
        None,
        "the anonymous GStreamer-shaped value must not remain a second source of truth",
    );
    assert!(
        crate::library::settings::get_setting(&db, CURVE_KEY)
            .unwrap()
            .is_some(),
        "the migration must store the authored frequency/gain curve",
    );
    assert_eq!(
        crate::library::settings::get_equalizer_bands(&db),
        legacy_levels,
        "the unchanged GNOME-facing projection must preserve every legacy level",
    );
}

/// A value the migration could not parse used to become a flat curve with no
/// log line at all, while the reader it replaced warned on exactly these two
/// cases. Deleting either `tracing::warn!` in `parse_legacy_levels` turns this
/// red.
#[test]
fn a_legacy_value_the_migration_cannot_parse_is_replaced_out_loud() {
    for (stored, expected_warning) in [
        ("0,0,three,0,0,0,0,0,0,0", "invalid equalizer bands"),
        ("1,2,3", "wrong equalizer band count"),
    ] {
        let conn = open(None).unwrap();
        migrate_connection(&conn).unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)",
            rusqlite::params![LEGACY_BANDS_KEY, stored],
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 52).unwrap();

        let logs = crate::log_capture::CapturedLogs::default();
        tracing::subscriber::with_default(crate::log_capture::LogCapture(logs.clone()), || {
            crate::db_equalizer::migrate_v53(&conn).unwrap();
        });

        assert!(
            logs.joined().contains(expected_warning),
            "{stored:?} must be reported as {expected_warning:?}, logged: {}",
            logs.joined(),
        );
        let db = Db::from_connection(conn);
        assert_eq!(
            crate::library::settings::get_equalizer_bands(&db),
            [0.0; 10],
            "an unreadable legacy value falls back to the flat preset",
        );
    }
}

/// The `DELETE` runs unconditionally, so a `SELECT` failure that is *not*
/// "absent" must abort the whole transaction rather than let the migration
/// destroy a value it never managed to look at. Putting `.ok()` back on the
/// `query_row` turns this red: the row is deleted and the migration reports
/// success.
#[test]
fn a_legacy_value_that_cannot_be_read_aborts_instead_of_being_deleted() {
    let conn = open(None).unwrap();
    migrate_connection(&conn).unwrap();
    // A blob is not text: the reader fails, and this is not "no rows".
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, x'00ff')",
        [LEGACY_BANDS_KEY],
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 52).unwrap();

    let error = crate::db_equalizer::migrate_v53(&conn).unwrap_err();

    assert!(
        matches!(error, rusqlite::Error::InvalidColumnType(..)),
        "the read failure must reach the caller, got: {error:?}",
    );
    let survivors: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM settings WHERE key = ?1",
            [LEGACY_BANDS_KEY],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(survivors, 1, "the unread value must still be there");
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 52, "a failed migration must not claim the version");
}
