//! The stdio MCP server: two resources and two tools over `reprise-core`.
//!
//! Resources: `reprise://library/summary`, `reprise://playlists`.
//! Tools: `music_search_tracks` (read), `music_create_playlist` (write, gated
//! on the `playlist:create` capability). All blocking database work runs on
//! `spawn_blocking`; the handler itself holds only the database path and the
//! startup capability snapshot, so it stays `Send + Sync`.

use std::path::PathBuf;
use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, Implementation, ListResourcesResult, PaginatedRequestParams,
    ReadResourceRequestParams, ReadResourceResult, Resource, ResourceContents, ServerCapabilities,
    ServerInfo,
};
use rmcp::service::RequestContext;
use rmcp::{tool, tool_handler, tool_router, ErrorData, RoleServer, ServerHandler};

use crate::data;
use crate::dto::{CreatePlaylistParams, SearchTracksParams};
use crate::error;

/// URI of the library-summary resource.
pub const RESOURCE_LIBRARY_SUMMARY: &str = "reprise://library/summary";
/// URI of the playlist-listing resource.
pub const RESOURCE_PLAYLISTS: &str = "reprise://playlists";

const RESOURCE_MIME_JSON: &str = "application/json";

const SERVER_INSTRUCTIONS: &str = "Reprise local music library. Read-only tools \
    and resources expose track metadata (never file paths); \
    `music_create_playlist` creates a new manual playlist and requires the \
    'playlist:create' capability, which is off by default.";

/// The MCP server handler.
#[derive(Clone)]
pub struct RepriseServer {
    db_path: Arc<PathBuf>,
    write_granted_at_startup: bool,
    tool_router: ToolRouter<Self>,
}

impl RepriseServer {
    /// Builds a handler bound to `db_path`. `write_granted_at_startup` is the
    /// `playlist:create` snapshot taken during startup (the restart half of
    /// the D18 write gate).
    pub fn new(db_path: PathBuf, write_granted_at_startup: bool) -> Self {
        Self {
            db_path: Arc::new(db_path),
            write_granted_at_startup,
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl RepriseServer {
    /// Paginated, read-only metadata search over the present library.
    #[tool(
        name = "music_search_tracks",
        description = "Search the present music library by title, artist, album \
            or genre. Returns opaque track ids plus display metadata (title, \
            artist, album, year, genre, rating, duration) — never file paths. \
            Paginate with `limit` (1..=200, default 50) and `offset`."
    )]
    async fn music_search_tracks(
        &self,
        Parameters(params): Parameters<SearchTracksParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let path = self.db_path.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            data::search_tracks(path.as_path(), &params.query, params.limit, params.offset)
        })
        .await
        .map_err(|error| error::join_error(&error))?;

        match outcome {
            Ok(result) => {
                let summary = format!("{} of {} matching track(s)", result.returned, result.total);
                error::structured_ok(&result, summary)
            }
            Err(err) => error::into_tool_outcome(err),
        }
    }

    /// Creates a new manual playlist from an explicit, ordered list of track
    /// ids. Never overwrites or deletes an existing playlist (Beschluss 2).
    #[tool(
        name = "music_create_playlist",
        description = "Create a new manual playlist from an explicit, ordered \
            list of track ids (at most 500; duplicates allowed). Requires the \
            'playlist:create' capability, which is off by default. Never \
            overwrites or deletes an existing playlist."
    )]
    async fn music_create_playlist(
        &self,
        Parameters(params): Parameters<CreatePlaylistParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let path = self.db_path.clone();
        let granted = self.write_granted_at_startup;
        let outcome = tokio::task::spawn_blocking(move || {
            data::create_playlist(path.as_path(), granted, &params.name, &params.track_ids)
        })
        .await
        .map_err(|error| error::join_error(&error))?;

        match outcome {
            Ok(result) => {
                let summary = format!(
                    "Created playlist '{}' (id {}) with {} track(s)",
                    result.name, result.playlist_id, result.track_count
                );
                error::structured_ok(&result, summary)
            }
            Err(err) => error::into_tool_outcome(err),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for RepriseServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(Implementation::new(
            "reprise-mcp",
            env!("CARGO_PKG_VERSION"),
        ))
        .with_instructions(SERVER_INSTRUCTIONS)
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Ok(ListResourcesResult::with_all_items(vec![
            Resource::new(RESOURCE_LIBRARY_SUMMARY, "library-summary")
                .with_description(
                    "Track, artist and album counts, total duration and analysis coverage.",
                )
                .with_mime_type(RESOURCE_MIME_JSON),
            Resource::new(RESOURCE_PLAYLISTS, "playlists")
                .with_description("Manual playlists: id, name and track count (no paths).")
                .with_mime_type(RESOURCE_MIME_JSON),
        ]))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, ErrorData> {
        let uri = request.uri;
        let path = self.db_path.clone();

        let json = match uri.as_str() {
            RESOURCE_LIBRARY_SUMMARY => {
                let outcome =
                    tokio::task::spawn_blocking(move || data::library_summary(path.as_path()))
                        .await
                        .map_err(|error| error::join_error(&error))?;
                error::serialize_resource(&outcome.map_err(error::resource_error)?)?
            }
            RESOURCE_PLAYLISTS => {
                let outcome =
                    tokio::task::spawn_blocking(move || data::list_playlists(path.as_path()))
                        .await
                        .map_err(|error| error::join_error(&error))?;
                error::serialize_resource(&outcome.map_err(error::resource_error)?)?
            }
            other => {
                return Err(ErrorData::resource_not_found(
                    format!("unknown resource: {other}"),
                    None,
                ));
            }
        };

        Ok(ReadResourceResult::new(vec![ResourceContents::text(
            json, uri,
        )
        .with_mime_type(RESOURCE_MIME_JSON)]))
    }
}
