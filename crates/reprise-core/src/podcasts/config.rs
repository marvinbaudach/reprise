//! Persisted podcast behavior and filter settings.

use rusqlite::Connection;

use super::PodcastKind;

pub const IMPORT_COUNT_KEY: &str = "podcasts.import_count";
pub const AUTO_DOWNLOAD_DEFAULT_KEY: &str = "podcasts.auto_download_default";
pub const CLEANUP_POLICY_KEY: &str = "podcasts.cleanup_policy";
pub const YOUTUBE_ENABLED_KEY: &str = "podcasts.youtube_enabled";
pub const YTDLP_PATH_KEY: &str = "podcasts.ytdlp_path";
pub const REFRESH_HOURS_KEY: &str = "sources.refresh_hours";
pub const FILTER_UNPLAYED_KEY: &str = "podcasts.filter.unplayed";
pub const FILTER_SHOW_KEY: &str = "podcasts.filter.show";
pub const FILTER_SOURCE_KEY: &str = "podcasts.filter.source";

pub const DEFAULT_IMPORT_COUNT: usize = 25;
pub const DEFAULT_REFRESH_HOURS: i64 = 6;

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
    pub youtube_enabled: bool,
    pub ytdlp_path: Option<String>,
    pub refresh_hours: i64,
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
        youtube_enabled: crate::library::settings::get_bool(conn, YOUTUBE_ENABLED_KEY, true)?,
        ytdlp_path: non_empty_setting(conn, YTDLP_PATH_KEY)?,
        refresh_hours: integer_setting(conn, REFRESH_HOURS_KEY)?
            .unwrap_or(DEFAULT_REFRESH_HOURS)
            .clamp(1, 24),
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
        assert!(config.youtube_enabled);
        assert_eq!(config.ytdlp_path, None);
        assert_eq!(config.refresh_hours, 6);
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
        crate::library::settings::set_bool(&conn, YOUTUBE_ENABLED_KEY, false).unwrap();
        crate::library::settings::set_setting(&conn, YTDLP_PATH_KEY, " /opt/yt-dlp ").unwrap();
        crate::library::settings::set_setting(&conn, REFRESH_HOURS_KEY, "0").unwrap();

        let config = load(&conn).unwrap();
        assert_eq!(config.import_count, 100);
        assert!(config.auto_download_default);
        assert_eq!(config.cleanup_policy, CleanupPolicy::KeepLast5);
        assert!(!config.youtube_enabled);
        assert_eq!(config.ytdlp_path.as_deref(), Some("/opt/yt-dlp"));
        assert_eq!(config.refresh_hours, 1);
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
