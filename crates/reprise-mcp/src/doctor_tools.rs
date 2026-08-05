//! MCP tool-router surface for Library Doctor scans, review, apply, and undo.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router, ErrorData};

use crate::doctor_dto::{ApplyTagsParams, ReviewTagsParams, ScanTagsParams};
use crate::server::RepriseServer;

#[tool_router(router = doctor_tool_router, vis = "pub(crate)")]
impl RepriseServer {
    #[tool(
        name = "music_scan_tags",
        description = "Scan embedded music tags for internally inconsistent metadata. By default this writes no music files. Set apply_safe=true to apply unambiguous changes through Reprise's journaled tag-write queue; that requires the 'tags:write' capability, which is off by default."
    )]
    async fn music_scan_tags(
        &self,
        Parameters(params): Parameters<ScanTagsParams>,
    ) -> Result<rmcp::model::CallToolResult, ErrorData> {
        let path = self.doctor_db_path();
        let granted = self.tags_write_granted_at_startup();
        let outcome = tokio::task::spawn_blocking(move || {
            crate::doctor_actions::scan_tags(path.as_path(), granted, &params)
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
        name = "music_review_tags",
        description = "Read the latest Library Doctor review set, grouped by album and containing no file paths. Optionally filter by casing, year, or genre and paginate albums with limit and offset."
    )]
    async fn music_review_tags(
        &self,
        Parameters(params): Parameters<ReviewTagsParams>,
    ) -> Result<rmcp::model::CallToolResult, ErrorData> {
        let path = self.doctor_db_path();
        let outcome = tokio::task::spawn_blocking(move || {
            crate::doctor_actions::review_tags(path.as_path(), &params)
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
        name = "music_apply_tags",
        description = "Apply selected Library Doctor rows or albums, resolve one spelling group, or revert the latest complete scan bracket. Every action writes through Reprise's journaled tag queue and requires the 'tags:write' capability, which is off by default."
    )]
    async fn music_apply_tags(
        &self,
        Parameters(params): Parameters<ApplyTagsParams>,
    ) -> Result<rmcp::model::CallToolResult, ErrorData> {
        let path = self.doctor_db_path();
        let granted = self.tags_write_granted_at_startup();
        let outcome = tokio::task::spawn_blocking(move || {
            crate::doctor_actions::apply_tags(path.as_path(), granted, &params)
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
