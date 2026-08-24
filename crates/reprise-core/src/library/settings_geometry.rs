//! Persisted list geometry for the single, comfortable list density.

use rusqlite::{Connection, Error as SqlError};

use super::{get_setting_in, set_setting_in};

pub const ROW_HEIGHT_KEY: &str = "ui.row_height";
pub const SECTION_HEADER_HEIGHT_KEY: &str = "ui.section_header_height";

fn get_height_in(conn: &Connection, key: &str) -> Result<Option<f64>, SqlError> {
    Ok(get_setting_in(conn, key)?
        .and_then(|value| value.parse().ok())
        .filter(|height: &f64| height.is_finite() && *height > 0.0))
}

fn set_height_in(conn: &Connection, key: &str, height: Option<f64>) -> Result<(), SqlError> {
    set_setting_in(conn, key, &height.unwrap_or(0.0).to_string())
}

pub fn get_row_height(db: &crate::db::Db) -> Result<Option<f64>, SqlError> {
    get_height_in(db.conn(), ROW_HEIGHT_KEY)
}

pub fn set_row_height(db: &crate::db::Db, height: Option<f64>) -> Result<(), SqlError> {
    set_height_in(db.conn(), ROW_HEIGHT_KEY, height)
}

pub fn get_section_header_height(db: &crate::db::Db) -> Result<Option<f64>, SqlError> {
    get_height_in(db.conn(), SECTION_HEADER_HEIGHT_KEY)
}

pub fn set_section_header_height(
    db: &crate::db::Db,
    height: Option<f64>,
) -> Result<(), SqlError> {
    set_height_in(db.conn(), SECTION_HEADER_HEIGHT_KEY, height)
}

pub fn set_row_and_section_header_heights(
    db: &crate::db::Db,
    row_height: f64,
    section_header_height: f64,
) -> Result<(), SqlError> {
    crate::events::in_txn(db.conn(), |conn| {
        set_height_in(conn, ROW_HEIGHT_KEY, Some(row_height))?;
        set_height_in(conn, SECTION_HEADER_HEIGHT_KEY, Some(section_header_height))
    })
}

#[cfg(test)]
mod tests {
    use super::super::set_setting_in;
    use super::*;

    #[test]
    fn row_and_section_header_heights_round_trip_independently() {
        let db = crate::db::Db::open_in_memory().unwrap();

        set_row_height(&db, Some(34.0)).unwrap();
        set_section_header_height(&db, Some(36.0)).unwrap();

        assert_eq!(get_row_height(&db).unwrap(), Some(34.0));
        assert_eq!(get_section_header_height(&db).unwrap(), Some(36.0));
    }

    #[test]
    fn invalid_or_cleared_geometry_is_not_returned() {
        let db = crate::db::Db::open_in_memory().unwrap();

        set_setting_in(db.conn(), SECTION_HEADER_HEIGHT_KEY, "NaN").unwrap();
        assert_eq!(get_section_header_height(&db).unwrap(), None);
        set_setting_in(db.conn(), SECTION_HEADER_HEIGHT_KEY, "-1").unwrap();
        assert_eq!(get_section_header_height(&db).unwrap(), None);
        set_section_header_height(&db, None).unwrap();
        assert_eq!(get_section_header_height(&db).unwrap(), None);
    }

    #[test]
    fn settled_row_and_header_heights_are_written_together() {
        let db = crate::db::Db::open_in_memory().unwrap();

        set_row_and_section_header_heights(&db, 34.0, 38.0).unwrap();

        assert_eq!(get_row_height(&db).unwrap(), Some(34.0));
        assert_eq!(get_section_header_height(&db).unwrap(), Some(38.0));
    }
}
