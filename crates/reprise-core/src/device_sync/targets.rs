//! The single per-device playlists target (`MTP-54`).
//!
//! MTP folders are object handles under a [`StorageId`], and handles are not
//! stable across reconnects. Reprise therefore persists only the storage id
//! and device-relative path, resolving a fresh handle whenever it connects.

use rusqlite::{params, Connection, OptionalExtension};

pub const DEFAULT_TARGET_PATH: &str = "/Music/Reprise";

/// An MTP storage id (PTP/MTP `StorageID`), such as internal storage or an SD
/// card. It is never a path component or an object handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StorageId(pub u32);

/// The playlists folder persisted for one device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncTarget {
    pub storage_id: Option<StorageId>,
    pub path: String,
    pub enabled: bool,
}

impl Default for SyncTarget {
    fn default() -> Self {
        Self {
            storage_id: None,
            path: DEFAULT_TARGET_PATH.to_owned(),
            enabled: true,
        }
    }
}

/// Whether a target's storage changed between two persisted states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageTransition {
    SameOrFirstResolution,
    Changed { previous: StorageId },
}

#[must_use]
pub fn target_storage_transition(previous: &SyncTarget, next: &SyncTarget) -> StorageTransition {
    match (previous.storage_id, next.storage_id) {
        (Some(previous_id), Some(next_id)) if previous_id != next_id => {
            StorageTransition::Changed {
                previous: previous_id,
            }
        }
        _ => StorageTransition::SameOrFirstResolution,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SyncTargetError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
}

/// Loads the target for `serial`, creating its playlists default when absent.
pub fn load_or_create_target(
    db: &crate::db::Db,
    serial: &str,
) -> Result<SyncTarget, SyncTargetError> {
    let conn = db.conn();
    match load_target_in(conn, serial)? {
        Some(target) => Ok(target),
        None => {
            let target = SyncTarget::default();
            save_target_in(conn, serial, &target)?;
            Ok(target)
        }
    }
}

pub fn load_target(
    db: &crate::db::Db,
    serial: &str,
) -> Result<Option<SyncTarget>, rusqlite::Error> {
    load_target_in(db.conn(), serial)
}

fn load_target_in(conn: &Connection, serial: &str) -> Result<Option<SyncTarget>, rusqlite::Error> {
    conn.query_row(
        "SELECT storage_id, path, enabled
         FROM device_sync_targets
         WHERE device_serial = ?1",
        [serial],
        |row| {
            let storage_id = row.get::<_, Option<i64>>(0)?;
            Ok(SyncTarget {
                storage_id: storage_id.map(|value| StorageId(u32::try_from(value).unwrap_or(0))),
                path: row.get(1)?,
                enabled: row.get(2)?,
            })
        },
    )
    .optional()
}

pub fn save_target(
    db: &crate::db::Db,
    serial: &str,
    target: &SyncTarget,
) -> Result<(), SyncTargetError> {
    save_target_in(db.conn(), serial, target)
}

fn save_target_in(
    conn: &Connection,
    serial: &str,
    target: &SyncTarget,
) -> Result<(), SyncTargetError> {
    let storage_id = target.storage_id.map(|id| i64::from(id.0));
    conn.execute(
        "INSERT INTO device_sync_targets (device_serial, storage_id, path, enabled)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(device_serial) DO UPDATE SET
           storage_id = excluded.storage_id,
           path = excluded.path,
           enabled = excluded.enabled",
        params![serial, storage_id, target.path, target.enabled],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mtp_54_a_fresh_device_has_exactly_one_playlists_target() {
        let db = crate::db::Db::open_in_memory().unwrap();

        let target = load_or_create_target(&db, "pixel").unwrap();

        assert_eq!(target, SyncTarget::default());
        let count: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM device_sync_targets WHERE device_serial = 'pixel'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(target.path, DEFAULT_TARGET_PATH);
    }

    #[test]
    fn persisted_target_is_not_replaced_by_the_default() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let expected = SyncTarget {
            storage_id: Some(StorageId(7)),
            path: "/SD/Playlists".into(),
            enabled: false,
        };
        save_target(&db, "pixel", &expected).unwrap();

        assert_eq!(load_or_create_target(&db, "pixel").unwrap(), expected);
    }

    #[test]
    fn save_target_round_trips_storage_path_and_activation() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let expected = SyncTarget {
            storage_id: Some(StorageId(u32::MAX)),
            path: "/Music/Custom".into(),
            enabled: false,
        };

        save_target(&db, "pixel", &expected).unwrap();

        assert_eq!(load_target(&db, "pixel").unwrap(), Some(expected));
    }

    #[test]
    fn targets_are_independent_per_device() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let phone = SyncTarget {
            storage_id: Some(StorageId(1)),
            path: "/Music/Phone".into(),
            enabled: true,
        };
        let tablet = SyncTarget {
            storage_id: Some(StorageId(2)),
            path: "/Music/Tablet".into(),
            enabled: false,
        };
        save_target(&db, "phone", &phone).unwrap();
        save_target(&db, "tablet", &tablet).unwrap();

        assert_eq!(load_target(&db, "phone").unwrap(), Some(phone));
        assert_eq!(load_target(&db, "tablet").unwrap(), Some(tablet));
    }

    #[test]
    fn mtp_32_storage_change_is_reported_only_after_resolution() {
        let unresolved = SyncTarget::default();
        let internal = SyncTarget {
            storage_id: Some(StorageId(1)),
            ..unresolved.clone()
        };
        let sd_card = SyncTarget {
            storage_id: Some(StorageId(2)),
            ..unresolved.clone()
        };

        assert_eq!(
            target_storage_transition(&unresolved, &internal),
            StorageTransition::SameOrFirstResolution
        );
        assert_eq!(
            target_storage_transition(&internal, &internal),
            StorageTransition::SameOrFirstResolution
        );
        assert_eq!(
            target_storage_transition(&internal, &sd_card),
            StorageTransition::Changed {
                previous: StorageId(1)
            }
        );
    }
}
