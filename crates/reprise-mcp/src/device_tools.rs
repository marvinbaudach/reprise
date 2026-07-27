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
            state from the running Reprise app: manual and smart playlist rows, \
            MP3 quality, deduplicated target totals, change summary, current and \
            projected storage composition, blockers, warnings, controls, progress, \
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
            running Reprise app. Actions: configure (device_name, sources with \
            kind playlist/smart plus id, quality_kbps), start, cancel. MP3 \
            quality_kbps accepts 128, 192, 256 or 320 and defaults to 256. Configuration \
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
