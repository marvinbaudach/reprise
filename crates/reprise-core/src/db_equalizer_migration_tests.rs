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
    assert_eq!(version, 53);
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
