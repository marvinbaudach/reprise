//! Block H-G: the global `online-sources-enabled` gate (`NET-1a`) and the
//! three per-module master switches (YouTube, Podcasts, Radio — `SET-8`)
//! over MCP. Reads and writes the exact same core authorities the GTK
//! "Online sources" preferences page calls:
//! `reprise_core::online_sources::{is_enabled, set_enabled}` for the global
//! gate, `reprise_core::modules::{is_enabled, set_enabled}` for each module.

use std::path::Path;

use reprise_core::modules::{ModuleDescriptor, PODCASTS_MODULE, RADIO_MODULE, YOUTUBE_MODULE};
use rmcp::schemars;
use serde::{Deserialize, Serialize};

use crate::data::{self, DataError};

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct ManageOnlineSourcesParams {
    /// One of: `get`, `set`.
    pub action: String,
    /// Required for `set`. One of: `global`, `youtube`, `podcasts`,
    /// `radio`. `global` is the `NET-1a` gate above all three modules;
    /// turning it off stops every request app-wide without deleting
    /// subscriptions or favorites.
    #[serde(default)]
    pub target: Option<String>,
    /// Required for `set`.
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct OnlineSourcesState {
    /// `NET-1a`'s global gate: off means no requests, no downloads, from
    /// any of the three modules below, regardless of their own switch.
    pub global_enabled: bool,
    pub youtube_enabled: bool,
    pub podcasts_enabled: bool,
    pub radio_enabled: bool,
}

impl OnlineSourcesState {
    pub fn summary(&self) -> String {
        if !self.global_enabled {
            return "Online sources are off: local player only".to_owned();
        }
        let on = [
            self.youtube_enabled.then_some("YouTube"),
            self.podcasts_enabled.then_some("Podcasts"),
            self.radio_enabled.then_some("Radio"),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        if on.is_empty() {
            "Online sources are on, but no module is enabled".to_owned()
        } else {
            format!("Online sources on: {}", on.join(", "))
        }
    }
}

pub fn manage_online_sources(
    path: &Path,
    granted_at_startup: bool,
    params: &ManageOnlineSourcesParams,
) -> Result<OnlineSourcesState, DataError> {
    let conn = data::open(path)?;
    match params.action.as_str() {
        "get" => {
            data::require_read(&conn)?;
        }
        "set" => {
            // Gated like the other podcast/YouTube/radio mutations: turning
            // a source on or off is source management, not a different
            // capability domain.
            let allowed = crate::capability::sources_manage_effective(&conn, granted_at_startup)
                .map_err(DataError::Db)?;
            if !allowed {
                return Err(DataError::CapabilityDenied("sources:manage"));
            }
            let target = params
                .target
                .as_deref()
                .ok_or_else(|| DataError::InvalidInput("target is required for set".to_owned()))?;
            let enabled = params
                .enabled
                .ok_or_else(|| DataError::InvalidInput("enabled is required for set".to_owned()))?;
            match target {
                "global" => reprise_core::online_sources::set_enabled(&conn, enabled)
                    .map_err(DataError::Db)?,
                "youtube" => set_module(&conn, &YOUTUBE_MODULE, enabled)?,
                "podcasts" => set_module(&conn, &PODCASTS_MODULE, enabled)?,
                "radio" => set_module(&conn, &RADIO_MODULE, enabled)?,
                other => {
                    return Err(DataError::InvalidInput(format!(
                        "unknown target '{other}'; expected global, youtube, podcasts, or radio"
                    )))
                }
            }
        }
        other => {
            return Err(DataError::InvalidInput(format!(
                "unknown online-sources action '{other}'"
            )))
        }
    }
    read_state(&conn)
}

fn set_module(
    conn: &rusqlite::Connection,
    module: &ModuleDescriptor,
    enabled: bool,
) -> Result<(), DataError> {
    reprise_core::modules::set_enabled(conn, module, enabled).map_err(DataError::Db)
}

fn read_state(conn: &rusqlite::Connection) -> Result<OnlineSourcesState, DataError> {
    Ok(OnlineSourcesState {
        global_enabled: reprise_core::online_sources::is_enabled(conn).map_err(DataError::Db)?,
        youtube_enabled: reprise_core::modules::is_enabled(conn, &YOUTUBE_MODULE)
            .map_err(DataError::Db)?,
        podcasts_enabled: reprise_core::modules::is_enabled(conn, &PODCASTS_MODULE)
            .map_err(DataError::Db)?,
        radio_enabled: reprise_core::modules::is_enabled(conn, &RADIO_MODULE)
            .map_err(DataError::Db)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded_db() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reprise.db");
        let conn = reprise_core::db::open_migrated(Some(&path)).unwrap();
        reprise_core::library::settings::set_bool(
            &conn,
            crate::capability::CAP_SOURCES_MANAGE,
            true,
        )
        .unwrap();
        (dir, path)
    }

    #[test]
    fn get_reads_the_real_defaults() {
        let (_dir, path) = seeded_db();

        let state = manage_online_sources(
            &path,
            true,
            &ManageOnlineSourcesParams {
                action: "get".into(),
                target: None,
                enabled: None,
            },
        )
        .unwrap();

        assert!(state.global_enabled, "NET-1a defaults to on");
        assert!(!state.youtube_enabled, "podcasts/youtube default to off");
        assert!(!state.podcasts_enabled);
        assert!(
            state.radio_enabled,
            "SRC-1: radio defaults to on, it only pings on user action"
        );
    }

    /// The whole point of a settings tool: the two positions must produce
    /// different reads, not just accept a write and echo the same state.
    #[test]
    fn set_global_off_actually_changes_what_get_reports_afterward() {
        let (_dir, path) = seeded_db();

        let before = manage_online_sources(
            &path,
            true,
            &ManageOnlineSourcesParams {
                action: "get".into(),
                target: None,
                enabled: None,
            },
        )
        .unwrap();
        assert!(before.global_enabled);

        let after = manage_online_sources(
            &path,
            true,
            &ManageOnlineSourcesParams {
                action: "set".into(),
                target: Some("global".into()),
                enabled: Some(false),
            },
        )
        .unwrap();

        assert!(!after.global_enabled);
        assert_ne!(before.global_enabled, after.global_enabled);

        // And the effect actually reaches the one real authority every
        // network entry point ANDs itself against.
        let conn = reprise_core::db::open_migrated(Some(&path)).unwrap();
        assert!(!reprise_core::online_sources::network_allowed(
            &conn,
            &reprise_core::modules::YOUTUBE_MODULE
        )
        .unwrap());
    }

    #[test]
    fn set_youtube_module_is_independent_of_podcasts_and_radio() {
        let (_dir, path) = seeded_db();

        let state = manage_online_sources(
            &path,
            true,
            &ManageOnlineSourcesParams {
                action: "set".into(),
                target: Some("youtube".into()),
                enabled: Some(true),
            },
        )
        .unwrap();

        assert!(state.youtube_enabled);
        assert!(!state.podcasts_enabled, "podcasts must not follow youtube");
        assert!(
            state.radio_enabled,
            "radio's own default (on) must be untouched by the youtube toggle"
        );
    }

    #[test]
    fn set_requires_target_and_enabled() {
        let (_dir, path) = seeded_db();

        let missing_target = manage_online_sources(
            &path,
            true,
            &ManageOnlineSourcesParams {
                action: "set".into(),
                target: None,
                enabled: Some(true),
            },
        )
        .unwrap_err();
        assert!(matches!(missing_target, DataError::InvalidInput(_)));

        let missing_enabled = manage_online_sources(
            &path,
            true,
            &ManageOnlineSourcesParams {
                action: "set".into(),
                target: Some("global".into()),
                enabled: None,
            },
        )
        .unwrap_err();
        assert!(matches!(missing_enabled, DataError::InvalidInput(_)));
    }

    #[test]
    fn set_rejects_an_unknown_target() {
        let (_dir, path) = seeded_db();

        let error = manage_online_sources(
            &path,
            true,
            &ManageOnlineSourcesParams {
                action: "set".into(),
                target: Some("spotify".into()),
                enabled: Some(true),
            },
        )
        .unwrap_err();

        assert!(matches!(error, DataError::InvalidInput(_)));
    }

    #[test]
    fn set_is_denied_when_sources_manage_is_not_granted() {
        let (_dir, path) = seeded_db();

        let error = manage_online_sources(
            &path,
            false,
            &ManageOnlineSourcesParams {
                action: "set".into(),
                target: Some("global".into()),
                enabled: Some(false),
            },
        )
        .unwrap_err();

        assert!(matches!(
            error,
            DataError::CapabilityDenied("sources:manage")
        ));
    }

    #[test]
    fn get_never_requires_the_sources_manage_capability() {
        let (_dir, path) = seeded_db();

        let state = manage_online_sources(
            &path,
            false,
            &ManageOnlineSourcesParams {
                action: "get".into(),
                target: None,
                enabled: None,
            },
        )
        .unwrap();

        assert!(state.global_enabled);
    }
}
