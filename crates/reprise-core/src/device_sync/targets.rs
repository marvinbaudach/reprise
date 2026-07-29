//! Named, per-device MTP sync targets (`MTP-38`).
//!
//! `E-5` settled that Reprise supports exactly one connected MTP device, so
//! the turn 7 (7a/7e) plan to split phone-sync configuration into global
//! Preferences rules plus per-device folders (`E-6`'s superseded addendum)
//! no longer applies — that split existed only to answer "which device do
//! these rules apply to". The cap is editable per device (`MTP-37`); this
//! module still models only the per-device *placement* half — where each
//! of the three content categories lands on *this* device, whether that
//! folder is in play at all, and its cap — while *what content* is wanted
//! (which shows, which channels, the transfer profile) stays a separate
//! concern owned elsewhere (`selection`, `podcasts::phone_sync`,
//! `DeviceSettings::profile`): [`SyncTarget`] must never grow a "what
//! content" field, or that separation stops being expressed in the type
//! system and becomes something only a comment promises.
//!
//! Replaces the single implicit managed folder from `78e379fd`
//! (`super::ManagedRoot`, always `Music/Reprise`) with three named
//! targets, one per content category. No migration: see
//! `docs/plans/podcasts-youtube-radio-turn6.md` §1b — there are no shipped
//! installations, so the old shape is simply replaced, not carried
//! forward. `ManagedRoot` and its MTP-transport consumers are untouched by
//! this commit; wiring the new model into the actual transfer path is
//! E2/E4, not this one.
//!
//! ## MTP reality this type is shaped around
//!
//! MTP has no paths. A folder is an object handle under a `StorageID`, and
//! handles are **not** stable across reconnects — the same folder can get
//! a different handle the next time the device is plugged in. So
//! [`SyncTarget`] persists only [`StorageId`] plus a path string; the
//! handle itself is never stored anywhere and must be re-resolved fresh on
//! every reconnect (the device browser, E6 — not built here). A folder
//! also cannot move across MTP storage boundaries: if the user repoints a
//! target at a different storage, the sync layer has to copy into the new
//! location and clean up everything under the old one.
//! [`target_storage_transition`] is the pure decision of *whether* that
//! happened, so the transfer code (E4) does not have to re-derive it from
//! two raw `SyncTarget` values.

use rusqlite::{params, Connection, OptionalExtension};

const GIB: u64 = 1024 * 1024 * 1024;

/// Design default for YouTube audio: "8 GiB, oldest files leave first".
pub const YOUTUBE_AUDIO_DEFAULT_CAP_BYTES: u64 = 8 * GIB;
/// Design default for podcast episodes: "cap 4 GiB".
pub const PODCAST_EPISODES_DEFAULT_CAP_BYTES: u64 = 4 * GIB;

/// One of the three content categories that get their own named device
/// folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SyncTargetKind {
    Playlists,
    YoutubeAudio,
    PodcastEpisodes,
}

impl SyncTargetKind {
    pub const ALL: [Self; 3] = [Self::Playlists, Self::YoutubeAudio, Self::PodcastEpisodes];

    /// Suggested path — 7d's device browser can override it per device,
    /// these are only the starting point. Deliberately **not** a single
    /// shared root under `/Music`: Android's media scanner has no other
    /// sorting hint than the folder name outside its own well-known
    /// directories.
    ///
    /// - Playlists keep the existing `/Music/Reprise` — `MTP-17`'s
    ///   authoritative area, unchanged by this commit.
    /// - YouTube audio gets its own `/Music/Reprise-YouTube` sibling,
    ///   deliberately *outside* the playlist tree: it is not organized
    ///   into playlists, and folding it into `/Music/Reprise` would make
    ///   that folder no longer describe "tracks in a Reprise playlist" —
    ///   the only signal Android's scanner gives the user for that
    ///   folder's contents.
    /// - Podcast episodes go under `/Podcasts/Reprise`, not under
    ///   `/Music` at all: Android's media scanner recognizes a top-level
    ///   `/Podcasts` folder and keeps its contents out of the music
    ///   library entirely. Do **not** "tidy" this into
    ///   `/Music/Reprise-Podcasts` for symmetry with YouTube audio — that
    ///   would put spoken-word episodes back into the music library,
    ///   exactly what this path is chosen to avoid.
    #[must_use]
    pub const fn default_path(self) -> &'static str {
        match self {
            Self::Playlists => "/Music/Reprise",
            Self::YoutubeAudio => "/Music/Reprise-YouTube",
            Self::PodcastEpisodes => "/Podcasts/Reprise",
        }
    }

    /// Suggested cap. `None` for playlists — existing behavior, unbounded,
    /// because the user curates playlist contents directly. YouTube audio
    /// and podcast episodes accumulate automatically from subscriptions,
    /// so both get a cap with oldest-first eviction (`super::cap`); the
    /// values are the design doc's, not derived from device capacity.
    #[must_use]
    pub const fn default_cap_bytes(self) -> Option<u64> {
        match self {
            Self::Playlists => None,
            Self::YoutubeAudio => Some(YOUTUBE_AUDIO_DEFAULT_CAP_BYTES),
            Self::PodcastEpisodes => Some(PODCAST_EPISODES_DEFAULT_CAP_BYTES),
        }
    }

    const fn storage_value(self) -> &'static str {
        match self {
            Self::Playlists => "playlists",
            Self::YoutubeAudio => "youtube_audio",
            Self::PodcastEpisodes => "podcast_episodes",
        }
    }
}

/// An MTP storage id (PTP/MTP `StorageID`), e.g. internal flash vs. an SD
/// card. Not a path component, and never derived from one — see the
/// module docs on why MTP object handles are never persisted here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StorageId(pub u32);

/// One content category's device folder, persisted per device.
///
/// Deliberately excludes anything that decides *what* content is wanted —
/// that stays in existing per-item selection (`podcasts::phone_sync`,
/// playlist selection, `MTP-37`) and the transfer profile
/// (`DeviceSettings::profile`), not here. Adding a "which
/// shows/channels/playlists" field would blur the placement/content
/// separation this type exists to express.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncTarget {
    pub kind: SyncTargetKind,
    /// `None` until the device browser (E6) has resolved a folder on some
    /// storage. MTP object handles themselves are never persisted — see
    /// the module docs.
    pub storage_id: Option<StorageId>,
    pub path: String,
    /// Whether this device's folder participates in sync at all.
    /// Independent of the global "sync this content type" rule and of
    /// per-item selection — this only says whether *this device* has an
    /// active slot for the category.
    pub enabled: bool,
    pub cap_bytes: Option<u64>,
}

impl SyncTarget {
    /// The suggested target for a freshly seen device: the design
    /// defaults, active immediately so a new device is usable without a
    /// device-browser detour for the common case.
    #[must_use]
    pub fn default_for(kind: SyncTargetKind) -> Self {
        Self {
            kind,
            storage_id: None,
            path: kind.default_path().to_string(),
            enabled: true,
            cap_bytes: kind.default_cap_bytes(),
        }
    }
}

/// `MTP-38`: whether a target's storage changed between two persisted
/// states. [`Self::SameOrFirstResolution`] covers both "storage id
/// unchanged" and "the folder had never been resolved to a storage yet" —
/// neither needs cleanup, because nothing was ever written under a
/// *different* storage id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageTransition {
    SameOrFirstResolution,
    /// A folder cannot move across MTP storage boundaries: the previous
    /// storage's copy of this target must be treated as orphaned and
    /// cleaned up once the sync layer (E4) has copied into the new one.
    Changed {
        previous: StorageId,
    },
}

/// `MTP-38`: the pure storage-boundary decision described in the module
/// docs. Only compares `storage_id` — a path change on the same storage is
/// an ordinary rename/move within that storage, not a boundary crossing.
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
    #[error("device sync target cap is too large for SQLite: {0}")]
    CapTooLarge(u64),
}

/// Loads all three targets for `serial`, creating and persisting any
/// missing ones with their defaults. Returns them in
/// [`SyncTargetKind::ALL`] order.
pub fn load_or_create_targets(
    conn: &Connection,
    serial: &str,
) -> Result<[SyncTarget; 3], SyncTargetError> {
    let mut targets = SyncTargetKind::ALL.map(SyncTarget::default_for);
    for target in &mut targets {
        match load_target(conn, serial, target.kind)? {
            Some(loaded) => *target = loaded,
            None => save_target(conn, serial, target)?,
        }
    }
    Ok(targets)
}

/// Loads one target, if it has been persisted for this device yet.
pub fn load_target(
    conn: &Connection,
    serial: &str,
    kind: SyncTargetKind,
) -> Result<Option<SyncTarget>, rusqlite::Error> {
    conn.query_row(
        "SELECT storage_id, path, enabled, cap_bytes
         FROM device_sync_targets
         WHERE device_serial = ?1 AND kind = ?2",
        params![serial, kind.storage_value()],
        |row| {
            let storage_id = row.get::<_, Option<i64>>(0)?;
            let cap_bytes = row.get::<_, Option<i64>>(3)?;
            Ok(SyncTarget {
                kind,
                storage_id: storage_id.map(|value| StorageId(u32::try_from(value).unwrap_or(0))),
                path: row.get(1)?,
                enabled: row.get(2)?,
                cap_bytes: cap_bytes.map(|value| u64::try_from(value).unwrap_or(0)),
            })
        },
    )
    .optional()
}

/// Persists one target, replacing whatever was previously stored for its
/// `(device_serial, kind)` pair.
pub fn save_target(
    conn: &Connection,
    serial: &str,
    target: &SyncTarget,
) -> Result<(), SyncTargetError> {
    let storage_id = target.storage_id.map(|id| i64::from(id.0));
    let cap_bytes = target.cap_bytes.map(sqlite_i64).transpose()?;
    conn.execute(
        "INSERT INTO device_sync_targets
         (device_serial, kind, storage_id, path, enabled, cap_bytes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(device_serial, kind) DO UPDATE SET
           storage_id = excluded.storage_id,
           path = excluded.path,
           enabled = excluded.enabled,
           cap_bytes = excluded.cap_bytes",
        params![
            serial,
            target.kind.storage_value(),
            storage_id,
            target.path,
            target.enabled,
            cap_bytes,
        ],
    )?;
    Ok(())
}

fn sqlite_i64(value: u64) -> Result<i64, SyncTargetError> {
    i64::try_from(value).map_err(|_| SyncTargetError::CapTooLarge(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn migrated() -> Connection {
        crate::db::open_migrated(None).unwrap()
    }

    #[test]
    fn mtp_38_defaults_match_the_design_docs_folders_and_caps() {
        let playlists = SyncTarget::default_for(SyncTargetKind::Playlists);
        let youtube = SyncTarget::default_for(SyncTargetKind::YoutubeAudio);
        let podcasts = SyncTarget::default_for(SyncTargetKind::PodcastEpisodes);

        assert_eq!(playlists.path, "/Music/Reprise");
        assert_eq!(playlists.cap_bytes, None);

        assert_eq!(youtube.path, "/Music/Reprise-YouTube");
        assert_eq!(youtube.cap_bytes, Some(8 * GIB));

        assert_eq!(podcasts.path, "/Podcasts/Reprise");
        assert_eq!(podcasts.cap_bytes, Some(4 * GIB));

        for target in [&playlists, &youtube, &podcasts] {
            assert_eq!(target.storage_id, None);
            assert!(target.enabled);
        }
    }

    #[test]
    fn mtp_38_load_or_create_persists_defaults_for_a_new_device() {
        let conn = migrated();

        let created = load_or_create_targets(&conn, "mtp:pixel").unwrap();
        let reloaded = load_or_create_targets(&conn, "mtp:pixel").unwrap();

        assert_eq!(created, reloaded);
        assert_eq!(
            reloaded.map(|target| target.kind),
            SyncTargetKind::ALL,
            "targets come back in stable ALL order"
        );
    }

    #[test]
    fn mtp_38_save_target_round_trips_storage_id_and_cap() {
        let conn = migrated();
        load_or_create_targets(&conn, "mtp:pixel").unwrap();

        let resolved = SyncTarget {
            kind: SyncTargetKind::YoutubeAudio,
            storage_id: Some(StorageId(0x0001_0002)),
            path: "/Music/Reprise-YouTube".to_string(),
            enabled: true,
            cap_bytes: Some(2 * GIB),
        };
        save_target(&conn, "mtp:pixel", &resolved).unwrap();

        let loaded = load_target(&conn, "mtp:pixel", SyncTargetKind::YoutubeAudio)
            .unwrap()
            .unwrap();
        assert_eq!(loaded, resolved);
    }

    #[test]
    fn mtp_38_targets_are_independent_per_device() {
        let conn = migrated();
        let phone = SyncTarget {
            kind: SyncTargetKind::PodcastEpisodes,
            storage_id: Some(StorageId(1)),
            path: "/Podcasts/Reprise".to_string(),
            enabled: true,
            cap_bytes: Some(GIB),
        };
        let dap = SyncTarget {
            path: "/Music/Podcasts".to_string(),
            storage_id: Some(StorageId(9)),
            ..phone.clone()
        };
        save_target(&conn, "mtp:phone", &phone).unwrap();
        save_target(&conn, "mtp:dap", &dap).unwrap();

        assert_eq!(
            load_target(&conn, "mtp:phone", SyncTargetKind::PodcastEpisodes)
                .unwrap()
                .unwrap()
                .path,
            "/Podcasts/Reprise"
        );
        assert_eq!(
            load_target(&conn, "mtp:dap", SyncTargetKind::PodcastEpisodes)
                .unwrap()
                .unwrap()
                .path,
            "/Music/Podcasts"
        );
    }

    #[test]
    fn mtp_38_no_storage_change_when_id_is_unchanged_or_not_yet_resolved() {
        let unresolved = SyncTarget::default_for(SyncTargetKind::Playlists);
        let still_unresolved = SyncTarget::default_for(SyncTargetKind::Playlists);
        assert_eq!(
            target_storage_transition(&unresolved, &still_unresolved),
            StorageTransition::SameOrFirstResolution
        );

        let first_resolution = SyncTarget {
            storage_id: Some(StorageId(7)),
            ..unresolved.clone()
        };
        assert_eq!(
            target_storage_transition(&unresolved, &first_resolution),
            StorageTransition::SameOrFirstResolution,
            "None -> Some is a first resolution, not a boundary crossing"
        );

        let same_storage_new_path = SyncTarget {
            path: "/Music/Reprise2".to_string(),
            ..first_resolution.clone()
        };
        assert_eq!(
            target_storage_transition(&first_resolution, &same_storage_new_path),
            StorageTransition::SameOrFirstResolution,
            "a path change on the same storage is not a boundary crossing"
        );
    }

    #[test]
    fn mtp_38_storage_change_is_reported_with_the_previous_storage_id() {
        let before = SyncTarget {
            storage_id: Some(StorageId(1)),
            ..SyncTarget::default_for(SyncTargetKind::PodcastEpisodes)
        };
        let after = SyncTarget {
            storage_id: Some(StorageId(2)),
            ..before.clone()
        };

        assert_eq!(
            target_storage_transition(&before, &after),
            StorageTransition::Changed {
                previous: StorageId(1)
            }
        );
    }
}
