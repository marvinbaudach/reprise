//! Persisted podcast behavior and filter settings.

use crate::db::Db;
use rusqlite::Connection;

use super::PodcastKind;

pub const IMPORT_COUNT_KEY: &str = "podcasts.import_count";
pub const AUTO_DOWNLOAD_DEFAULT_KEY: &str = "podcasts.auto_download_default";
pub const CLEANUP_POLICY_KEY: &str = "podcasts.cleanup_policy";
pub const YOUTUBE_IMPORT_COUNT_KEY: &str = "podcasts.youtube_import_count";
pub const YOUTUBE_HIDE_SHORTS_DEFAULT_KEY: &str = "podcasts.youtube_hide_shorts_default";
pub const YTDLP_PATH_KEY: &str = "podcasts.ytdlp_path";
pub const REFRESH_HOURS_KEY: &str = "sources.refresh_hours";
pub const FILTER_UNPLAYED_KEY: &str = "podcasts.filter.unplayed";
pub const FILTER_SHOW_KEY: &str = "podcasts.filter.show";
pub const FILTER_SOURCE_KEY: &str = "podcasts.filter.source";
/// `MTP-36`: the global "latest N per channel" default for the phone-sync
/// YouTube target, overridable per channel
/// (`podcasts::store::latest_per_channel_overrides`). Device-independent —
/// `E-5` means there is exactly one MTP device, so this lives here rather
/// than on `DeviceSettings` or a per-device sync target.
pub const LATEST_PER_CHANNEL_DEFAULT_KEY: &str = "podcasts.latest_per_channel_default";
/// `SRC-10` addendum (Block B2): the "Downloaded" filter chip.
pub const FILTER_DOWNLOADED_KEY: &str = "podcasts.filter.downloaded";
/// `POD-5` / `O-5`: the global "keep N downloaded" default backing
/// `CleanupPolicy::KeepLast5`, overridable per channel
/// (`podcasts::store::set_keep_downloaded`). Same shape as `MTP-36`'s
/// `LATEST_PER_CHANNEL_DEFAULT_KEY` — one mental model for two quantity
/// limits, deliberately (`O-5`).
pub const KEEP_DOWNLOADED_DEFAULT_KEY: &str = "podcasts.keep_downloaded_default";

pub const DEFAULT_IMPORT_COUNT: usize = 25;
pub const DEFAULT_YOUTUBE_IMPORT_COUNT: usize = 10;

/// The range `import_count` is read back through, and therefore the range it
/// is written through. Named rather than inlined at both ends because a clamp
/// that only one of the two sides applies is not a clamp — it is a silent
/// rewrite of whatever the other side stored.
pub const IMPORT_COUNT_MIN: i64 = 5;
pub const IMPORT_COUNT_MAX: i64 = 100;
/// Same, for the YouTube per-channel count. Deliberately a different range
/// from the RSS one: a channel's back catalogue is not a show's.
pub const YOUTUBE_IMPORT_COUNT_MIN: i64 = 3;
pub const YOUTUBE_IMPORT_COUNT_MAX: i64 = 50;
pub const DEFAULT_REFRESH_HOURS: i64 = 6;
/// `MTP-36`: decided 2026-07-29 — a global default of 5.
pub const DEFAULT_LATEST_PER_CHANNEL: usize = 5;
/// `POD-5` / `O-5`: decided 2026-07-29 — `CleanupPolicy::KeepLast5` kept a
/// hardcoded 5 per show; that hardcoded 5 becomes this global default, and
/// "keep N" is just its generalization.
pub const DEFAULT_KEEP_DOWNLOADED: usize = 5;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CleanupPolicy {
    #[default]
    KeepAll,
    DeletePlayedAfter7Days,
    KeepLast5,
}

impl CleanupPolicy {
    #[must_use]
    pub fn as_setting(self) -> &'static str {
        match self {
            Self::KeepAll => "keep_all",
            Self::DeletePlayedAfter7Days => "delete_played_7d",
            Self::KeepLast5 => "keep_last_5",
        }
    }

    #[must_use]
    pub fn from_setting(value: &str) -> Self {
        match value {
            "delete_played_7d" => Self::DeletePlayedAfter7Days,
            "keep_last_5" => Self::KeepLast5,
            _ => Self::KeepAll,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PodcastConfig {
    pub import_count: usize,
    pub auto_download_default: bool,
    pub cleanup_policy: CleanupPolicy,
    /// YouTube's per-channel episode window ("Episodes per channel" on the
    /// Online sources page) — independent of `import_count`, which is the
    /// RSS "Episodes per show" setting.
    pub youtube_import_count: usize,
    /// Seeds new/untouched YouTube channels' Shorts visibility; a channel's
    /// own explicit override (see `youtube_channel_detail`) always wins.
    pub youtube_hide_shorts_default: bool,
    pub ytdlp_path: Option<String>,
    pub refresh_hours: i64,
    /// `MTP-36`: the global "latest N per channel" default for YouTube
    /// phone sync — `0` means unlimited, like every other numeric sync
    /// setting since `MTP-38`. A subscription's own override (`podcasts::
    /// store::latest_per_channel_overrides`) always wins over this.
    pub latest_per_channel_default: usize,
    /// `POD-5` / `O-5`: the global "keep N downloaded" default backing
    /// `CleanupPolicy::KeepLast5` — `0` means unlimited, like every other
    /// numeric sync/cleanup setting since `E-9`. A subscription's own
    /// override (`SubscriptionRow::keep_downloaded`) always wins over this
    /// (`podcasts::downloads::resolve_keep_downloaded`).
    pub keep_downloaded_default: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PodcastFilterConfig {
    pub unplayed_only: bool,
    pub show: Option<String>,
    pub source: Option<PodcastKind>,
    pub downloaded_only: bool,
}

pub fn load(db: &Db) -> Result<PodcastConfig, rusqlite::Error> {
    let conn = db.conn();
    load_in(conn)
}

pub(crate) fn load_in(conn: &Connection) -> Result<PodcastConfig, rusqlite::Error> {
    Ok(PodcastConfig {
        import_count: integer_setting(conn, IMPORT_COUNT_KEY)?
            .unwrap_or(DEFAULT_IMPORT_COUNT as i64)
            .clamp(IMPORT_COUNT_MIN, IMPORT_COUNT_MAX) as usize,
        auto_download_default: crate::library::settings::get_bool_in(
            conn,
            AUTO_DOWNLOAD_DEFAULT_KEY,
            false,
        )?,
        cleanup_policy: crate::library::settings::get_setting_in(conn, CLEANUP_POLICY_KEY)?
            .as_deref()
            .map(CleanupPolicy::from_setting)
            .unwrap_or_default(),
        youtube_import_count: integer_setting(conn, YOUTUBE_IMPORT_COUNT_KEY)?
            .unwrap_or(DEFAULT_YOUTUBE_IMPORT_COUNT as i64)
            .clamp(YOUTUBE_IMPORT_COUNT_MIN, YOUTUBE_IMPORT_COUNT_MAX)
            as usize,
        youtube_hide_shorts_default: crate::library::settings::get_bool_in(
            conn,
            YOUTUBE_HIDE_SHORTS_DEFAULT_KEY,
            true,
        )?,
        ytdlp_path: non_empty_setting(conn, YTDLP_PATH_KEY)?,
        refresh_hours: integer_setting(conn, REFRESH_HOURS_KEY)?
            .unwrap_or(DEFAULT_REFRESH_HOURS)
            .clamp(1, 24),
        // `MTP-36`: 0 is a valid, meaningful value (unlimited) — the clamp
        // floor stays 0, unlike the other counts on this page which have no
        // "unlimited" reading.
        latest_per_channel_default: integer_setting(conn, LATEST_PER_CHANNEL_DEFAULT_KEY)?
            .unwrap_or(DEFAULT_LATEST_PER_CHANNEL as i64)
            .clamp(0, 100) as usize,
        // `POD-5` / `E-9`: 0 is a valid, meaningful value (unlimited) — same
        // zero-floor reasoning as `latest_per_channel_default` above.
        keep_downloaded_default: integer_setting(conn, KEEP_DOWNLOADED_DEFAULT_KEY)?
            .unwrap_or(DEFAULT_KEEP_DOWNLOADED as i64)
            .clamp(0, 100) as usize,
    })
}

pub fn load_filter(db: &Db) -> Result<PodcastFilterConfig, rusqlite::Error> {
    let conn = db.conn();
    Ok(PodcastFilterConfig {
        unplayed_only: crate::library::settings::get_bool_in(conn, FILTER_UNPLAYED_KEY, false)?,
        show: non_empty_setting(conn, FILTER_SHOW_KEY)?,
        source: match non_empty_setting(conn, FILTER_SOURCE_KEY)?.as_deref() {
            Some("rss") => Some(PodcastKind::Rss),
            Some("youtube") => Some(PodcastKind::Youtube),
            _ => None,
        },
        downloaded_only: crate::library::settings::get_bool_in(conn, FILTER_DOWNLOADED_KEY, false)?,
    })
}

/// Persists the podcast episodes-per-show count, through the same clamp
/// [`load`] reads it back through.
///
/// These setters live next to their readers on purpose. Written in the
/// frontend — where they were until this commit — each one duplicated a key
/// name and skipped the clamp, so a value the UI happened to allow could be
/// stored and then silently read back as something else.
pub fn set_import_count(db: &Db, value: usize) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    let clamped = (value as i64).clamp(IMPORT_COUNT_MIN, IMPORT_COUNT_MAX);
    crate::library::settings::set_setting_in(conn, IMPORT_COUNT_KEY, &clamped.to_string())
}

/// Persists whether newly discovered episodes download automatically.
pub fn set_auto_download_default(db: &Db, value: bool) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    crate::library::settings::set_bool_in(conn, AUTO_DOWNLOAD_DEFAULT_KEY, value)
}

/// Persists the cleanup policy, through the policy's own setting spelling
/// rather than a string the caller has to get right.
pub fn set_cleanup_policy(db: &Db, value: CleanupPolicy) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    crate::library::settings::set_setting_in(conn, CLEANUP_POLICY_KEY, value.as_setting())
}

/// Persists the YouTube per-channel import count, through the same clamp
/// [`load`] reads it back through.
pub fn set_youtube_import_count(db: &Db, value: usize) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    let clamped = (value as i64).clamp(YOUTUBE_IMPORT_COUNT_MIN, YOUTUBE_IMPORT_COUNT_MAX);
    crate::library::settings::set_setting_in(conn, YOUTUBE_IMPORT_COUNT_KEY, &clamped.to_string())
}

/// Persists whether YouTube Shorts are hidden by default on new channels.
pub fn set_youtube_hide_shorts_default(db: &Db, value: bool) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    crate::library::settings::set_bool_in(conn, YOUTUBE_HIDE_SHORTS_DEFAULT_KEY, value)
}

/// Persists the whole podcast filter — the exact inverse of [`load_filter`],
/// and kept adjacent to it so the two cannot drift. `None` is stored as the
/// empty string, which is what [`load_filter`] reads back as `None`.
pub fn save_filter(db: &Db, filter: &PodcastFilterConfig) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    crate::library::settings::set_bool_in(conn, FILTER_UNPLAYED_KEY, filter.unplayed_only)?;
    crate::library::settings::set_setting_in(
        conn,
        FILTER_SHOW_KEY,
        filter.show.as_deref().unwrap_or_default(),
    )?;
    crate::library::settings::set_setting_in(
        conn,
        FILTER_SOURCE_KEY,
        match filter.source {
            Some(PodcastKind::Rss) => "rss",
            Some(PodcastKind::Youtube) => "youtube",
            None => "",
        },
    )?;
    crate::library::settings::set_bool_in(conn, FILTER_DOWNLOADED_KEY, filter.downloaded_only)
}

/// The one authority for "may a refresh/download for this kind start a
/// network request right now" — ANDs the global online-sources gate
/// (`NET-1a`) with the kind's own module (Podcasts for RSS, YouTube for
/// YouTube). Every podcast/YouTube network entry point routes through this
/// instead of checking a module flag alone.
pub fn source_network_allowed(db: &Db, kind: PodcastKind) -> Result<bool, rusqlite::Error> {
    let conn = db.conn();
    source_network_allowed_in(conn, kind)
}

pub(crate) fn source_network_allowed_in(
    conn: &Connection,
    kind: PodcastKind,
) -> Result<bool, rusqlite::Error> {
    let module = match kind {
        PodcastKind::Rss => &crate::modules::PODCASTS_MODULE,
        PodcastKind::Youtube => &crate::modules::YOUTUBE_MODULE,
    };
    crate::online_sources::network_allowed_in(conn, module)
}

fn non_empty_setting(conn: &Connection, key: &str) -> Result<Option<String>, rusqlite::Error> {
    Ok(crate::library::settings::get_setting_in(conn, key)?
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty()))
}

fn integer_setting(conn: &Connection, key: &str) -> Result<Option<i64>, rusqlite::Error> {
    Ok(non_empty_setting(conn, key)?.and_then(|value| value.parse().ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Db {
        Db::open_in_memory().unwrap()
    }

    #[test]
    fn defaults_match_the_source_contract() {
        let config = load(&db()).unwrap();
        assert_eq!(config.import_count, 25);
        assert!(!config.auto_download_default);
        assert_eq!(config.cleanup_policy, CleanupPolicy::KeepAll);
        assert_eq!(config.youtube_import_count, 10);
        assert!(config.youtube_hide_shorts_default);
        assert_eq!(config.ytdlp_path, None);
        assert_eq!(config.refresh_hours, 6);
        assert_eq!(config.latest_per_channel_default, 5);
        assert_eq!(config.keep_downloaded_default, 5);
    }

    #[test]
    fn values_are_bundled_and_bounded() {
        let db = db();
        crate::library::settings::set_setting(&db, IMPORT_COUNT_KEY, "900").unwrap();
        crate::library::settings::set_bool(&db, AUTO_DOWNLOAD_DEFAULT_KEY, true).unwrap();
        crate::library::settings::set_setting(
            &db,
            CLEANUP_POLICY_KEY,
            CleanupPolicy::KeepLast5.as_setting(),
        )
        .unwrap();
        crate::library::settings::set_setting(&db, YOUTUBE_IMPORT_COUNT_KEY, "900").unwrap();
        crate::library::settings::set_bool(&db, YOUTUBE_HIDE_SHORTS_DEFAULT_KEY, false).unwrap();
        crate::library::settings::set_setting(&db, YTDLP_PATH_KEY, " /opt/yt-dlp ").unwrap();
        crate::library::settings::set_setting(&db, REFRESH_HOURS_KEY, "0").unwrap();
        crate::library::settings::set_setting(&db, LATEST_PER_CHANNEL_DEFAULT_KEY, "900").unwrap();
        crate::library::settings::set_setting(&db, KEEP_DOWNLOADED_DEFAULT_KEY, "900").unwrap();

        let config = load(&db).unwrap();
        assert_eq!(config.import_count, 100);
        assert!(config.auto_download_default);
        assert_eq!(config.cleanup_policy, CleanupPolicy::KeepLast5);
        assert_eq!(config.youtube_import_count, 50);
        assert!(!config.youtube_hide_shorts_default);
        assert_eq!(config.ytdlp_path.as_deref(), Some("/opt/yt-dlp"));
        assert_eq!(config.refresh_hours, 1);
        assert_eq!(config.latest_per_channel_default, 100);
        assert_eq!(config.keep_downloaded_default, 100);
    }

    #[test]
    fn mtp_36_latest_per_channel_default_clamp_floor_is_zero_not_the_documented_minimum() {
        // Unlike every other count on this page, 0 is a valid, meaningful
        // value here (unlimited) — the clamp floor must not reject it back
        // up to some positive minimum the way `import_count`'s floor of 5
        // would.
        let db = db();
        crate::library::settings::set_setting(&db, LATEST_PER_CHANNEL_DEFAULT_KEY, "0").unwrap();

        assert_eq!(load(&db).unwrap().latest_per_channel_default, 0);
    }

    #[test]
    fn pod_5_keep_downloaded_default_clamp_floor_is_zero_not_the_documented_minimum() {
        // Same reasoning as `latest_per_channel_default` above (`E-9`): 0
        // means unlimited here, so the clamp floor must not reject it back
        // up to a positive minimum.
        let db = db();
        crate::library::settings::set_setting(&db, KEEP_DOWNLOADED_DEFAULT_KEY, "0").unwrap();

        assert_eq!(load(&db).unwrap().keep_downloaded_default, 0);
    }

    #[test]
    fn net_1a_source_network_allowed_ands_the_global_gate_with_the_kind_module() {
        let db = db();

        // Neither Podcasts nor YouTube is on by default.
        assert!(!source_network_allowed(&db, PodcastKind::Rss).unwrap());
        assert!(!source_network_allowed(&db, PodcastKind::Youtube).unwrap());

        crate::modules::set_enabled(&db, &crate::modules::PODCASTS_MODULE, true).unwrap();
        assert!(source_network_allowed(&db, PodcastKind::Rss).unwrap());
        assert!(
            !source_network_allowed(&db, PodcastKind::Youtube).unwrap(),
            "Podcasts on must not implicitly allow YouTube (issue #96)"
        );

        crate::modules::set_enabled(&db, &crate::modules::YOUTUBE_MODULE, true).unwrap();
        assert!(source_network_allowed(&db, PodcastKind::Youtube).unwrap());

        crate::online_sources::set_enabled(&db, false).unwrap();
        assert!(!source_network_allowed(&db, PodcastKind::Rss).unwrap());
        assert!(!source_network_allowed(&db, PodcastKind::Youtube).unwrap());
    }

    #[test]
    fn sticky_filter_values_are_bundled_and_invalid_sources_clear() {
        let db = db();
        crate::library::settings::set_bool(&db, FILTER_UNPLAYED_KEY, true).unwrap();
        crate::library::settings::set_setting(&db, FILTER_SHOW_KEY, " Show ").unwrap();
        crate::library::settings::set_setting(&db, FILTER_SOURCE_KEY, "youtube").unwrap();

        assert_eq!(
            load_filter(&db).unwrap(),
            PodcastFilterConfig {
                unplayed_only: true,
                show: Some("Show".to_owned()),
                source: Some(PodcastKind::Youtube),
                downloaded_only: false,
            }
        );

        crate::library::settings::set_setting(&db, FILTER_SOURCE_KEY, "unknown").unwrap();
        assert_eq!(load_filter(&db).unwrap().source, None);
    }

    /// `SRC-10` addendum (Block B2): the "Downloaded" filter persists like
    /// every other sticky filter value.
    #[test]
    fn src_10_downloaded_filter_persists_across_a_reload() {
        let db = db();
        crate::library::settings::set_bool(&db, FILTER_DOWNLOADED_KEY, true).unwrap();

        assert!(load_filter(&db).unwrap().downloaded_only);
    }

    #[test]
    fn setting_the_counts_clamps_to_the_range_load_reads_them_back_through() {
        // The bug this closes: written unclamped (as the frontend did), a
        // value outside the range is stored happily and then read back as a
        // different number, so the UI shows something nobody chose.
        let db = db();

        set_import_count(&db, 4).unwrap();
        assert_eq!(load(&db).unwrap().import_count, IMPORT_COUNT_MIN as usize);
        set_import_count(&db, 1_000).unwrap();
        assert_eq!(load(&db).unwrap().import_count, IMPORT_COUNT_MAX as usize);

        set_youtube_import_count(&db, 1).unwrap();
        assert_eq!(
            load(&db).unwrap().youtube_import_count,
            YOUTUBE_IMPORT_COUNT_MIN as usize
        );
        set_youtube_import_count(&db, 1_000).unwrap();
        assert_eq!(
            load(&db).unwrap().youtube_import_count,
            YOUTUBE_IMPORT_COUNT_MAX as usize
        );
    }

    #[test]
    fn every_setter_round_trips_through_its_own_reader() {
        let db = db();

        set_import_count(&db, 42).unwrap();
        set_auto_download_default(&db, true).unwrap();
        set_cleanup_policy(&db, CleanupPolicy::KeepLast5).unwrap();
        set_youtube_import_count(&db, 20).unwrap();
        set_youtube_hide_shorts_default(&db, false).unwrap();

        let config = load(&db).unwrap();
        assert_eq!(config.import_count, 42);
        assert!(config.auto_download_default);
        assert_eq!(config.cleanup_policy, CleanupPolicy::KeepLast5);
        assert_eq!(config.youtube_import_count, 20);
        assert!(!config.youtube_hide_shorts_default);
    }

    #[test]
    fn save_filter_is_the_exact_inverse_of_load_filter() {
        let db = db();
        let filter = PodcastFilterConfig {
            unplayed_only: true,
            show: Some("Some Show".to_owned()),
            source: Some(PodcastKind::Youtube),
            downloaded_only: true,
        };

        save_filter(&db, &filter).unwrap();
        assert_eq!(load_filter(&db).unwrap(), filter);

        // And back to empty: `None` must survive as `None`, not as `Some("")`.
        save_filter(&db, &PodcastFilterConfig::default()).unwrap();
        assert_eq!(
            load_filter(&db).unwrap(),
            PodcastFilterConfig::default(),
            "an empty filter must round trip as empty, not as empty strings"
        );
    }
}
