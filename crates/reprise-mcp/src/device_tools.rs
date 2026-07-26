//! MCP tools for live, path-free Android synchronization state and control.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router, ErrorData};

use crate::device_dto::{DeviceSyncParams, GetDeviceSyncStateParams};
use crate::server::RepriseServer;

#[tool_router(router = device_tool_router, vis = "pub(crate)")]
impl RepriseServer {
    #[tool(
        name = "music_get_device_sync_state",
        description = "Read connected Android devices and live synchronization \
            state from the running Reprise app: free and total storage, managed \
            and selected track counts, planned copy/removal delta, progress, \
            current title and effective bytes per second. Never returns serials \
            or filesystem/device paths."
    )]
    async fn music_get_device_sync_state(
        &self,
        Parameters(_params): Parameters<GetDeviceSyncStateParams>,
    ) -> Result<rmcp::model::CallToolResult, ErrorData> {
        let outcome = tokio::task::spawn_blocking(crate::device_sync::state)
            .await
            .map_err(|error| crate::error::join_error(&error))?;
        crate::error::playback_structured_outcome(outcome, |state| {
            format!("{} connected device(s)", state.devices.len())
        })
    }

    #[tool(
        name = "music_device_sync",
        description = "Configure or control Android synchronization in the \
            running Reprise app. Actions: configure_playlist (device_name, \
            playlist_name, remove_unselected, bitrate_kbps), start, cancel. Configuration \
            and start are separate so the destructive delta can be inspected \
            with music_get_device_sync_state before transfer. Only files \
            managed by Reprise under Music/Reprise are eligible for removal. \
            Requires the opt-in 'device:sync' capability, off by default."
    )]
    async fn music_device_sync(
        &self,
        Parameters(params): Parameters<DeviceSyncParams>,
    ) -> Result<rmcp::model::CallToolResult, ErrorData> {
        let action = match crate::device_sync::DeviceSyncAction::from_params(&params) {
            Ok(action) => action,
            Err(message) => return Ok(crate::error::tool_error(message)),
        };
        let path = self.source_db_path();
        let granted = self.device_sync_granted_at_startup();
        let allowed = tokio::task::spawn_blocking(move || {
            crate::data::device_sync_allowed(path.as_path(), granted)
        })
        .await
        .map_err(|error| crate::error::join_error(&error))?;
        match allowed {
            Ok(true) => {}
            Ok(false) => {
                return Ok(crate::error::tool_error(
                    "Permission denied: the 'device:sync' capability is not \
                     granted. Enable it in Reprise and restart the MCP server."
                        .to_owned(),
                ));
            }
            Err(error) => return crate::error::into_tool_outcome(error),
        }

        let outcome = tokio::task::spawn_blocking(move || crate::device_sync::mutate(action))
            .await
            .map_err(|error| crate::error::join_error(&error))?;
        match outcome {
            Ok(summary) => crate::error::playback_outcome(Ok(()), summary),
            Err(error) => crate::error::playback_outcome(Err(error), String::new()),
        }
    }
}
