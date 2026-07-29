//! Persisted podcast behavior and filter settings.

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

pub const DEFAULT_IMPORT_COUNT: usize = 25;
pub const DEFAULT_YOUTUBE_IMPORT_COUNT: usize = 10;
pub const DEFAULT_REFRESH_HOURS: i64 = 6;
/// `MTP-36`: decided 2026-07-29 — a global default of 5.
pub const DEFAULT_LATEST_PER_CHANNEL: usize = 5;

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
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PodcastFilterConfig {
    pub unplayed_only: bool,
    pub show: Option<String>,
    pub source: Option<PodcastKind>,
}

pub fn load(conn: &Connection) -> Result<PodcastConfig, rusqlite::Error> {
    Ok(PodcastConfig {
        import_count: integer_setting(conn, IMPORT_COUNT_KEY)?
            .unwrap_or(DEFAULT_IMPORT_COUNT as i64)
            .clamp(5, 100) as usize,
        auto_download_default: crate::library::settings::get_bool(
            conn,
            AUTO_DOWNLOAD_DEFAULT_KEY,
            false,
        )?,
        cleanup_policy: crate::library::settings::get_setting(conn, CLEANUP_POLICY_KEY)?
            .as_deref()
            .map(CleanupPolicy::from_setting)
            .unwrap_or_default(),
        youtube_import_count: integer_setting(conn, YOUTUBE_IMPORT_COUNT_KEY)?
            .unwrap_or(DEFAULT_YOUTUBE_IMPORT_COUNT as i64)
            .clamp(3, 50) as usize,
        youtube_hide_shorts_default: crate::library::settings::get_bool(
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
    })
}

pub fn load_filter(conn: &Connection) -> Result<PodcastFilterConfig, rusqlite::Error> {
    Ok(PodcastFilterConfig {
        unplayed_only: crate::library::settings::get_bool(conn, FILTER_UNPLAYED_KEY, false)?,
        show: non_empty_setting(conn, FILTER_SHOW_KEY)?,
        source: match non_empty_setting(conn, FILTER_SOURCE_KEY)?.as_deref() {
            Some("rss") => Some(PodcastKind::Rss),
            Some("youtube") => Some(PodcastKind::Youtube),
            _ => None,
        },
    })
}

/// The one authority for "may a refresh/download for this kind start a
/// network request right now" — ANDs the global online-sources gate
/// (`NET-1a`) with the kind's own module (Podcasts for RSS, YouTube for
/// YouTube). Every podcast/YouTube network entry point routes through this
/// instead of checking a module flag alone.
pub fn source_network_allowed(
    conn: &Connection,
    kind: PodcastKind,
) -> Result<bool, rusqlite::Error> {
    let module = match kind {
        PodcastKind::Rss => &crate::modules::PODCASTS_MODULE,
        PodcastKind::Youtube => &crate::modules::YOUTUBE_MODULE,
    };
    crate::online_sources::network_allowed(conn, module)
}

fn non_empty_setting(conn: &Connection, key: &str) -> Result<Option<String>, rusqlite::Error> {
    Ok(crate::library::settings::get_setting(conn, key)?
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty()))
}

fn integer_setting(conn: &Connection, key: &str) -> Result<Option<i64>, rusqlite::Error> {
    Ok(non_empty_setting(conn, key)?.and_then(|value| value.parse().ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        crate::db::open_migrated(None).unwrap()
    }

    #[test]
    fn defaults_match_the_source_contract() {
        let config = load(&conn()).unwrap();
        assert_eq!(config.import_count, 25);
        assert!(!config.auto_download_default);
        assert_eq!(config.cleanup_policy, CleanupPolicy::KeepAll);
        assert_eq!(config.youtube_import_count, 10);
        assert!(config.youtube_hide_shorts_default);
        assert_eq!(config.ytdlp_path, None);
        assert_eq!(config.refresh_hours, 6);
        assert_eq!(config.latest_per_channel_default, 5);
    }

    #[test]
    fn values_are_bundled_and_bounded() {
        let conn = conn();
        crate::library::settings::set_setting(&conn, IMPORT_COUNT_KEY, "900").unwrap();
        crate::library::settings::set_bool(&conn, AUTO_DOWNLOAD_DEFAULT_KEY, true).unwrap();
        crate::library::settings::set_setting(
            &conn,
            CLEANUP_POLICY_KEY,
            CleanupPolicy::KeepLast5.as_setting(),
        )
        .unwrap();
        crate::library::settings::set_setting(&conn, YOUTUBE_IMPORT_COUNT_KEY, "900").unwrap();
        crate::library::settings::set_bool(&conn, YOUTUBE_HIDE_SHORTS_DEFAULT_KEY, false).unwrap();
        crate::library::settings::set_setting(&conn, YTDLP_PATH_KEY, " /opt/yt-dlp ").unwrap();
        crate::library::settings::set_setting(&conn, REFRESH_HOURS_KEY, "0").unwrap();
        crate::library::settings::set_setting(&conn, LATEST_PER_CHANNEL_DEFAULT_KEY, "900")
            .unwrap();

        let config = load(&conn).unwrap();
        assert_eq!(config.import_count, 100);
        assert!(config.auto_download_default);
        assert_eq!(config.cleanup_policy, CleanupPolicy::KeepLast5);
        assert_eq!(config.youtube_import_count, 50);
        assert!(!config.youtube_hide_shorts_default);
        assert_eq!(config.ytdlp_path.as_deref(), Some("/opt/yt-dlp"));
        assert_eq!(config.refresh_hours, 1);
        assert_eq!(config.latest_per_channel_default, 100);
    }

    #[test]
    fn mtp_36_latest_per_channel_default_clamp_floor_is_zero_not_the_documented_minimum() {
        // Unlike every other count on this page, 0 is a valid, meaningful
        // value here (unlimited) — the clamp floor must not reject it back
        // up to some positive minimum the way `import_count`'s floor of 5
        // would.
        let conn = conn();
        crate::library::settings::set_setting(&conn, LATEST_PER_CHANNEL_DEFAULT_KEY, "0").unwrap();

        assert_eq!(load(&conn).unwrap().latest_per_channel_default, 0);
    }

    #[test]
    fn net_1a_source_network_allowed_ands_the_global_gate_with_the_kind_module() {
        let conn = conn();

        // Neither Podcasts nor YouTube is on by default.
        assert!(!source_network_allowed(&conn, PodcastKind::Rss).unwrap());
        assert!(!source_network_allowed(&conn, PodcastKind::Youtube).unwrap());

        crate::modules::set_enabled(&conn, &crate::modules::PODCASTS_MODULE, true).unwrap();
        assert!(source_network_allowed(&conn, PodcastKind::Rss).unwrap());
        assert!(
            !source_network_allowed(&conn, PodcastKind::Youtube).unwrap(),
            "Podcasts on must not implicitly allow YouTube (issue #96)"
        );

        crate::modules::set_enabled(&conn, &crate::modules::YOUTUBE_MODULE, true).unwrap();
        assert!(source_network_allowed(&conn, PodcastKind::Youtube).unwrap());

        crate::online_sources::set_enabled(&conn, false).unwrap();
        assert!(!source_network_allowed(&conn, PodcastKind::Rss).unwrap());
        assert!(!source_network_allowed(&conn, PodcastKind::Youtube).unwrap());
    }

    #[test]
    fn sticky_filter_values_are_bundled_and_invalid_sources_clear() {
        let conn = conn();
        crate::library::settings::set_bool(&conn, FILTER_UNPLAYED_KEY, true).unwrap();
        crate::library::settings::set_setting(&conn, FILTER_SHOW_KEY, " Show ").unwrap();
        crate::library::settings::set_setting(&conn, FILTER_SOURCE_KEY, "youtube").unwrap();

        assert_eq!(
            load_filter(&conn).unwrap(),
            PodcastFilterConfig {
                unplayed_only: true,
                show: Some("Show".to_owned()),
                source: Some(PodcastKind::Youtube),
            }
        );

        crate::library::settings::set_setting(&conn, FILTER_SOURCE_KEY, "unknown").unwrap();
        assert_eq!(load_filter(&conn).unwrap().source, None);
    }
}
