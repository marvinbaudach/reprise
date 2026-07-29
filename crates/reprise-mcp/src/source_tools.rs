//! MCP tool-router surface for podcast/YouTube and radio source management.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router, ErrorData};

use crate::channel_detail::GetChannelDetailParams;
use crate::discovery_actions::SearchSourcesParams;
use crate::episode_actions::ManageEpisodesParams;
use crate::online_sources_tool::ManageOnlineSourcesParams;
use crate::server::RepriseServer;
use crate::source_actions::{ManagePodcastsParams, ManageRadioParams};

#[tool_router(router = source_tool_router, vis = "pub(crate)")]
impl RepriseServer {
    #[tool(
        name = "music_search_sources",
        description = "Search for a new podcast, YouTube channel, or radio \
            station to subscribe to. `provider` (`rss`, `youtube`, or \
            `radio`) pins the search to exactly one provider — there is no \
            mixed result list. Sources already subscribed are filtered out. \
            Requires the 'sources:manage' capability, which is off by \
            default: search performs network (iTunes, radio-browser) and \
            subprocess (yt-dlp) work, gated exactly like the \
            add/edit/remove/refresh mutations. Add a result with \
            music_manage_podcasts or music_manage_radio."
    )]
    async fn music_search_sources(
        &self,
        Parameters(params): Parameters<SearchSourcesParams>,
    ) -> Result<rmcp::model::CallToolResult, ErrorData> {
        let path = self.source_db_path();
        let granted = self.sources_manage_granted_at_startup();
        let outcome = tokio::task::spawn_blocking(move || {
            crate::discovery_actions::search_sources(path.as_path(), granted, &params)
        })
        .await
        .map_err(|error| crate::error::join_error(&error))?;

        match outcome {
            Ok(result) => {
                let summary = result.summary();
                crate::error::structured_ok(&result, summary)
            }
            Err(error) => crate::error::into_tool_outcome(error),
        }
    }

    #[tool(
        name = "music_manage_podcasts",
        description = "Manage cached RSS and YouTube podcast subscriptions. \
            Actions: add, edit, remove, refresh. Mutations require the \
            'sources:manage' capability, which is off by default. Read cached \
            subscriptions and episodes through the reprise://podcasts resource."
    )]
    async fn music_manage_podcasts(
        &self,
        Parameters(params): Parameters<ManagePodcastsParams>,
    ) -> Result<rmcp::model::CallToolResult, ErrorData> {
        let path = self.source_db_path();
        let granted = self.sources_manage_granted_at_startup();
        let outcome = tokio::task::spawn_blocking(move || {
            crate::source_actions::manage_podcasts(path.as_path(), granted, &params)
        })
        .await
        .map_err(|error| crate::error::join_error(&error))?;

        match outcome {
            Ok(result) => {
                let summary = result.summary();
                crate::error::structured_ok(&result, summary)
            }
            Err(error) => crate::error::into_tool_outcome(error),
        }
    }

    #[tool(
        name = "music_manage_radio",
        description = "Manage cached radio favorites. Actions: add (requires \
            an HTTP(S) stream, PLS, M3U, or HLS URL; playlists are resolved, \
            and name is optional and otherwise read from ICY metadata), edit, \
            remove. Mutations require \
            the 'sources:manage' capability, which is off by default. Read \
            cached favorites through the reprise://radio resource."
    )]
    async fn music_manage_radio(
        &self,
        Parameters(params): Parameters<ManageRadioParams>,
    ) -> Result<rmcp::model::CallToolResult, ErrorData> {
        let path = self.source_db_path();
        let granted = self.sources_manage_granted_at_startup();
        let outcome = tokio::task::spawn_blocking(move || {
            crate::source_actions::manage_radio(path.as_path(), granted, &params)
        })
        .await
        .map_err(|error| crate::error::join_error(&error))?;

        match outcome {
            Ok(result) => crate::error::structured_ok(
                &result,
                format!(
                    "{} radio favorite '{}' (id {})",
                    result.action, result.name, result.id
                ),
            ),
            Err(error) => crate::error::into_tool_outcome(error),
        }
    }

    #[tool(
        name = "music_get_channel_detail",
        description = "Read one podcast/YouTube subscription's channel detail: \
            the episode window, the Shorts filter (YouTube only — RSS episodes \
            are never Shorts), and per-episode/aggregate download state with \
            real file sizes. `show_shorts` defaults to false and `limit` \
            defaults to 10 (pass 40 for the 'Load more' window), matching the \
            GTK YouTube channel detail's defaults (POD-10/POD-11) — both \
            surfaces call the same reprise-core projection. Read-only, no \
            capability required beyond the base read access every tool has."
    )]
    async fn music_get_channel_detail(
        &self,
        Parameters(params): Parameters<GetChannelDetailParams>,
    ) -> Result<rmcp::model::CallToolResult, ErrorData> {
        let path = self.source_db_path();
        let outcome = tokio::task::spawn_blocking(move || {
            crate::channel_detail::channel_detail(path.as_path(), &params)
        })
        .await
        .map_err(|error| crate::error::join_error(&error))?;

        match outcome {
            Ok(result) => {
                let summary = format!(
                    "{} of {} episode(s) in '{}'",
                    result.shown, result.available, result.title
                );
                crate::error::structured_ok(&result, summary)
            }
            Err(error) => crate::error::into_tool_outcome(error),
        }
    }

    #[tool(
        name = "music_manage_episodes",
        description = "Batch-act on up to 100 podcast/YouTube episodes by id. \
            Actions: `download` (downloads each episode now through the same \
            path the refresh pipeline's auto-download uses; gated per episode \
            by whether its source — RSS or YouTube — is enabled), `remove` \
            (tombstones and immediately commits — unlike the GUI's ten-second \
            undo toast, there is no MCP-side undo window), and `want_on_device` \
            (sets or clears the persistent 'sync to phone' intent, MTP-20; \
            requires `wanted`. An episode marked wanted without a local file \
            has its download queued automatically by the sync pipeline rather \
            than being rejected). Reports one outcome per id, so a bad id in a \
            batch never fails the whole call. Requires 'sources:manage', off \
            by default."
    )]
    async fn music_manage_episodes(
        &self,
        Parameters(params): Parameters<ManageEpisodesParams>,
    ) -> Result<rmcp::model::CallToolResult, ErrorData> {
        let path = self.source_db_path();
        let granted = self.sources_manage_granted_at_startup();
        let outcome = tokio::task::spawn_blocking(move || {
            crate::episode_actions::manage_episodes(path.as_path(), granted, &params)
        })
        .await
        .map_err(|error| crate::error::join_error(&error))?;

        match outcome {
            Ok(result) => {
                let summary = result.summary();
                crate::error::structured_ok(&result, summary)
            }
            Err(error) => crate::error::into_tool_outcome(error),
        }
    }

    #[tool(
        name = "music_manage_online_sources",
        description = "Read or change the Online sources settings (design 7b, \
            NET-1a/SET-8): action `get` reads the global gate plus the three \
            independent module switches (YouTube, Podcasts, Radio); action \
            `set` changes one of them (`target` = global|youtube|podcasts|radio, \
            `enabled`). Turning the global gate off stops every request app-wide \
            — cover downloads, portraits, lyrics, New Releases, and all three \
            sources — without deleting subscriptions or favorites. `get` needs \
            no special capability; `set` requires 'sources:manage', off by \
            default."
    )]
    async fn music_manage_online_sources(
        &self,
        Parameters(params): Parameters<ManageOnlineSourcesParams>,
    ) -> Result<rmcp::model::CallToolResult, ErrorData> {
        let path = self.source_db_path();
        let granted = self.sources_manage_granted_at_startup();
        let outcome = tokio::task::spawn_blocking(move || {
            crate::online_sources_tool::manage_online_sources(path.as_path(), granted, &params)
        })
        .await
        .map_err(|error| crate::error::join_error(&error))?;

        match outcome {
            Ok(result) => {
                let summary = result.summary();
                crate::error::structured_ok(&result, summary)
            }
            Err(error) => crate::error::into_tool_outcome(error),
        }
    }
}
