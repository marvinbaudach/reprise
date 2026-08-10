//! Persisted list geometry keyed by the active list density.

use rusqlite::{Connection, Error as SqlError};

use super::{get_setting_in, set_setting_in, ListDensity};

pub const ROW_HEIGHT_COMFORTABLE_KEY: &str = "ui.row_height.comfortable";
pub const ROW_HEIGHT_STANDARD_KEY: &str = "ui.row_height.standard";
pub const ROW_HEIGHT_COMPACT_KEY: &str = "ui.row_height.compact";
pub const SECTION_HEADER_HEIGHT_COMFORTABLE_KEY: &str = "ui.section_header_height.comfortable";
pub const SECTION_HEADER_HEIGHT_STANDARD_KEY: &str = "ui.section_header_height.standard";
pub const SECTION_HEADER_HEIGHT_COMPACT_KEY: &str = "ui.section_header_height.compact";

const fn row_height_key(density: ListDensity) -> &'static str {
    match density {
        ListDensity::Comfortable => ROW_HEIGHT_COMFORTABLE_KEY,
        ListDensity::Standard => ROW_HEIGHT_STANDARD_KEY,
        ListDensity::Compact => ROW_HEIGHT_COMPACT_KEY,
    }
}

const fn section_header_height_key(density: ListDensity) -> &'static str {
    match density {
        ListDensity::Comfortable => SECTION_HEADER_HEIGHT_COMFORTABLE_KEY,
        ListDensity::Standard => SECTION_HEADER_HEIGHT_STANDARD_KEY,
        ListDensity::Compact => SECTION_HEADER_HEIGHT_COMPACT_KEY,
    }
}

fn get_height_in(conn: &Connection, key: &str) -> Result<Option<f64>, SqlError> {
    Ok(get_setting_in(conn, key)?
        .and_then(|value| value.parse().ok())
        .filter(|height: &f64| height.is_finite() && *height > 0.0))
}

fn set_height_in(conn: &Connection, key: &str, height: Option<f64>) -> Result<(), SqlError> {
    set_setting_in(conn, key, &height.unwrap_or(0.0).to_string())
}

pub fn get_row_height(db: &crate::db::Db, density: ListDensity) -> Result<Option<f64>, SqlError> {
    get_height_in(db.conn(), row_height_key(density))
}

pub fn set_row_height(
    db: &crate::db::Db,
    density: ListDensity,
    height: Option<f64>,
) -> Result<(), SqlError> {
    set_height_in(db.conn(), row_height_key(density), height)
}

pub fn get_section_header_height(
    db: &crate::db::Db,
    density: ListDensity,
) -> Result<Option<f64>, SqlError> {
    get_height_in(db.conn(), section_header_height_key(density))
}

pub fn set_section_header_height(
    db: &crate::db::Db,
    density: ListDensity,
    height: Option<f64>,
) -> Result<(), SqlError> {
    set_height_in(db.conn(), section_header_height_key(density), height)
}

#[cfg(test)]
mod tests {
    use super::super::set_setting_in;
    use super::*;

    #[test]
    fn row_and_section_header_heights_round_trip_independently_per_density() {
        let db = crate::db::Db::open_in_memory().unwrap();

        set_row_height(&db, ListDensity::Standard, Some(34.0)).unwrap();
        set_section_header_height(&db, ListDensity::Standard, Some(36.0)).unwrap();
        set_section_header_height(&db, ListDensity::Compact, Some(38.0)).unwrap();

        assert_eq!(
            get_row_height(&db, ListDensity::Standard).unwrap(),
            Some(34.0)
        );
        assert_eq!(
            get_section_header_height(&db, ListDensity::Standard).unwrap(),
            Some(36.0)
        );
        assert_eq!(
            get_section_header_height(&db, ListDensity::Compact).unwrap(),
            Some(38.0)
        );
        assert_eq!(
            get_section_header_height(&db, ListDensity::Comfortable).unwrap(),
            None
        );
    }

    #[test]
    fn invalid_or_cleared_geometry_is_not_returned() {
        let db = crate::db::Db::open_in_memory().unwrap();

        set_setting_in(db.conn(), SECTION_HEADER_HEIGHT_STANDARD_KEY, "NaN").unwrap();
        assert_eq!(
            get_section_header_height(&db, ListDensity::Standard).unwrap(),
            None
        );
        set_setting_in(db.conn(), SECTION_HEADER_HEIGHT_STANDARD_KEY, "-1").unwrap();
        assert_eq!(
            get_section_header_height(&db, ListDensity::Standard).unwrap(),
            None
        );
        set_section_header_height(&db, ListDensity::Standard, None).unwrap();
        assert_eq!(
            get_section_header_height(&db, ListDensity::Standard).unwrap(),
            None
        );
    }
}
