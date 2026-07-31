//! Runtime discovery for the yt-dlp subprocess boundary.

use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
};

use super::{YtDlp, YtDlpTimeouts};
use crate::podcasts::config::YoutubeBrowser;

impl YtDlp {
    /// Discovers the executable without probing it.
    ///
    /// The environment browser override is reserved for explicit packaging
    /// and diagnostic use. Product paths use [`Self::discover_with_browser`]
    /// so a persisted privacy opt-out cannot be overridden invisibly.
    pub fn discover(setting_path: Option<&str>) -> Self {
        let browser_session = std::env::var_os("REPRISE_YTDLP_COOKIES_FROM_BROWSER");
        Self::from_discovery(setting_path, browser_session.as_deref())
    }

    /// Discovers yt-dlp with an explicitly selected signed-in browser.
    ///
    /// `None` is an authoritative privacy opt-out and therefore never falls
    /// back to the diagnostic environment override.
    pub fn discover_with_browser(
        setting_path: Option<&str>,
        browser: Option<YoutubeBrowser>,
    ) -> Self {
        let browser_session = browser.map(|browser| OsString::from(browser.as_setting()));
        Self::from_discovery(setting_path, browser_session.as_deref())
    }

    fn from_discovery(setting_path: Option<&str>, browser_session: Option<&OsStr>) -> Self {
        Self {
            binary: resolve_binary(
                std::env::var_os("REPRISE_YTDLP_BIN").as_deref(),
                setting_path,
            ),
            browser_session: resolve_browser_session(browser_session),
            timeouts: YtDlpTimeouts::default(),
        }
    }
}

pub fn resolve_binary(environment_override: Option<&OsStr>, setting_path: Option<&str>) -> PathBuf {
    environment_override
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            setting_path
                .map(str::trim)
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
        })
        .unwrap_or_else(|| PathBuf::from("yt-dlp"))
}

pub(super) fn resolve_browser_session(environment_override: Option<&OsStr>) -> Option<OsString> {
    environment_override
        .and_then(OsStr::to_str)
        .map(str::trim)
        .filter(|browser| !browser.is_empty())
        .map(OsString::from)
}
