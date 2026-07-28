//! MCP tool-router surface for podcast/YouTube and radio source management.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router, ErrorData};

use crate::discovery_actions::SearchSourcesParams;
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
}
