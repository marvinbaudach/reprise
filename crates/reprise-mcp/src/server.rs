//! The stdio MCP server over `reprise-core`, plus feature-gated live playback
//! and queue controls over the running app's local D-Bus interface.
//!
//! Resources: `reprise://library/summary`, `reprise://playlists`, and
//! `reprise://concerts`. Tools: `music_search_tracks` (read),
//! `music_create_playlist` (write, gated on the `playlist:create` capability).
//! All blocking database and bus work runs on `spawn_blocking`; the handler
//! itself holds only paths and startup capability snapshots, so it stays
//! `Send + Sync`.

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
use crate::dto::{
    BrowseLibraryParams, CreateInstrumentalParams, CreatePlaylistParams, GetPlaylistParams,
    JobStatusParams, SearchTracksParams, UpdatePlaylistParams,
};
#[cfg(feature = "mpris")]
use crate::dto::{
    PlayParams, PlaybackControlParams, PlaybackStateParams, QueueParams, SetPlaybackParams,
};
use crate::error;

/// URI of the library-summary resource.
pub const RESOURCE_LIBRARY_SUMMARY: &str = "reprise://library/summary";
/// URI of the playlist-listing resource.
pub const RESOURCE_PLAYLISTS: &str = "reprise://playlists";
/// URI of the filtered upcoming-concert listing.
pub const RESOURCE_CONCERTS: &str = "reprise://concerts";

const RESOURCE_MIME_JSON: &str = "application/json";

const SERVER_INSTRUCTIONS: &str = "Reprise local music library and player. \
    Read-only tools and resources expose path-free track, artist, album, and \
    playlist metadata. Playlist creation and safe rename/append operations use \
    separate opt-in capabilities. Playback tools expose transport, live state, \
    volume, seek, shuffle, repeat, targeted play, and a bounded Play Next queue; \
    they require the running Reprise app. \
    `music_create_instrumental` queues experimental vocal-removal renders of \
    explicit tracks (requires the 'ai:create' capability, off by default) and \
    returns immediately with job ids; `music_get_job_status` reports their \
    state and progress. Both AI tools return ids only, never file paths.";

/// The MCP server handler.
#[derive(Clone)]
pub struct RepriseServer {
    db_path: Arc<PathBuf>,
    staging_path: Arc<PathBuf>,
    write_granted_at_startup: bool,
    playlist_manage_granted_at_startup: bool,
    ai_create_granted_at_startup: bool,
    tool_router: ToolRouter<Self>,
}

impl RepriseServer {
    #[cfg(feature = "mpris")]
    async fn playback_allowed(&self) -> Result<Option<CallToolResult>, ErrorData> {
        let path = self.db_path.clone();
        let allowed = tokio::task::spawn_blocking(move || data::playback_allowed(path.as_path()))
            .await
            .map_err(|error| error::join_error(&error))?;
        match allowed {
            Ok(true) => Ok(None),
            Ok(false) => Ok(Some(error::playback_denied())),
            Err(err) => error::into_tool_outcome(err).map(Some),
        }
    }

    /// Builds a handler bound to `db_path` (and `staging_path` for the AI job
    /// queue). The three booleans are startup snapshots for the write-class
    /// capabilities (`playlist:create`, `playlist:manage`, `ai:create`) — the
    /// restart half of the D18 / Beschluss 7 gate.
    pub fn new(
        db_path: PathBuf,
        staging_path: PathBuf,
        write_granted_at_startup: bool,
        playlist_manage_granted_at_startup: bool,
        ai_create_granted_at_startup: bool,
    ) -> Self {
        Self {
            db_path: Arc::new(db_path),
            staging_path: Arc::new(staging_path),
            write_granted_at_startup,
            playlist_manage_granted_at_startup,
            ai_create_granted_at_startup,
            tool_router: Self::build_tool_router(),
        }
    }

    /// Combines the always-on tool router with the `mpris`-gated playback
    /// router when the feature is compiled in. `#[tool_router]` (see
    /// `tool_router.rs` in the rmcp-macros source) scans an impl block's
    /// methods for the `#[tool]` attribute purely syntactically and cannot
    /// see per-method `#[cfg(...)]` gates, so the two playback tools live in
    /// their own `#[cfg(feature = "mpris")]` impl block with a separate named
    /// router (`playback_tool_router`) merged in only for that build.
    #[cfg(feature = "mpris")]
    fn build_tool_router() -> ToolRouter<Self> {
        Self::tool_router() + Self::playback_tool_router()
    }

    #[cfg(not(feature = "mpris"))]
    fn build_tool_router() -> ToolRouter<Self> {
        Self::tool_router()
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

    /// Paginated, path-free artist discovery.
    #[tool(
        name = "music_search_artists",
        description = "Search artists in the present library by case-insensitive \
            substring. Returns artist name, track count, album count and total \
            plays — never file paths. Paginate with limit and offset."
    )]
    async fn music_search_artists(
        &self,
        Parameters(params): Parameters<BrowseLibraryParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let path = self.db_path.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            data::search_artists(path.as_path(), &params.query, params.limit, params.offset)
        })
        .await
        .map_err(|error| error::join_error(&error))?;

        match outcome {
            Ok(result) => error::structured_ok(
                &result,
                format!("{} of {} matching artist(s)", result.returned, result.total),
            ),
            Err(err) => error::into_tool_outcome(err),
        }
    }

    /// Paginated, path-free album discovery.
    #[tool(
        name = "music_search_albums",
        description = "Search albums and album artists in the present library \
            by case-insensitive substring. Returns display metadata and counts \
            — never file paths. Paginate with limit and offset."
    )]
    async fn music_search_albums(
        &self,
        Parameters(params): Parameters<BrowseLibraryParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let path = self.db_path.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            data::search_albums(path.as_path(), &params.query, params.limit, params.offset)
        })
        .await
        .map_err(|error| error::join_error(&error))?;

        match outcome {
            Ok(result) => error::structured_ok(
                &result,
                format!("{} of {} matching album(s)", result.returned, result.total),
            ),
            Err(err) => error::into_tool_outcome(err),
        }
    }

    /// Reads one manual playlist's membership in durable order.
    #[tool(
        name = "music_get_playlist",
        description = "Read one manual playlist by id, including a paginated \
            page of track display metadata in playlist order. Read-only and \
            never returns file paths."
    )]
    async fn music_get_playlist(
        &self,
        Parameters(params): Parameters<GetPlaylistParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let path = self.db_path.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            data::playlist_contents(
                path.as_path(),
                params.playlist_id,
                params.limit,
                params.offset,
            )
        })
        .await
        .map_err(|error| error::join_error(&error))?;

        match outcome {
            Ok(result) => error::structured_ok(
                &result,
                format!(
                    "{} of {} track(s) in playlist '{}'",
                    result.returned, result.total, result.playlist.name
                ),
            ),
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

    /// Applies one non-destructive update to an existing manual playlist.
    #[tool(
        name = "music_update_playlist",
        description = "Update an existing manual playlist without deleting \
            anything. Supported actions: rename, or add_tracks (ordered append; \
            duplicates allowed; at most 500 ids). Requires the separate \
            'playlist:manage' capability, which is off by default. Removing \
            tracks and deleting playlists are intentionally not supported."
    )]
    async fn music_update_playlist(
        &self,
        Parameters(params): Parameters<UpdatePlaylistParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let path = self.db_path.clone();
        let granted = self.playlist_manage_granted_at_startup;
        let outcome = tokio::task::spawn_blocking(move || {
            crate::playlist_update::update(path.as_path(), granted, &params)
        })
        .await
        .map_err(|error| error::join_error(&error))?;

        match outcome {
            Ok(result) => error::structured_ok(
                &result,
                format!(
                    "Updated playlist '{}' (id {}): {} affected",
                    result.name, result.playlist_id, result.affected
                ),
            ),
            Err(err) => error::into_tool_outcome(err),
        }
    }

    /// Queues one experimental vocal-removal render per explicit track and
    /// returns immediately with job ids (plan 3.2). Never renders inline and
    /// never returns a file path.
    #[tool(
        name = "music_create_instrumental",
        description = "Queue experimental vocals-removed instrumental renders of \
            explicit library tracks (at most 500). Registers one background job \
            per track and returns immediately with job ids plus a batch id — \
            rendering happens later in a worker (the running Reprise app or \
            `reprise-cli jobs work`), so jobs stay queued until then. Tracks that \
            already have an instrumental or a pending job are referenced, not \
            re-rendered. `save` (default true) marks renders for the library; \
            `save=false` leaves them in the Conversion staging view to save or \
            discard. Requires the 'ai:create' capability, off by default. Returns \
            ids only, never file paths."
    )]
    async fn music_create_instrumental(
        &self,
        Parameters(params): Parameters<CreateInstrumentalParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let db_path = self.db_path.clone();
        let staging_path = self.staging_path.clone();
        let granted = self.ai_create_granted_at_startup;
        let outcome = tokio::task::spawn_blocking(move || {
            data::create_instrumental(
                db_path.as_path(),
                staging_path.as_path(),
                granted,
                &params.track_ids,
                params.save,
            )
        })
        .await
        .map_err(|error| error::join_error(&error))?;

        match outcome {
            Ok(result) => {
                let summary = format!(
                    "Queued {} instrumental job(s), {} referenced existing (batch {})",
                    result.created, result.deduplicated, result.batch_id
                );
                error::structured_ok(&result, summary)
            }
            Err(err) => error::into_tool_outcome(err),
        }
    }

    /// Reports the state and progress of instrumental jobs (read-only job
    /// metadata; plan 3.2). Returns ids, states, progress and timestamps only —
    /// never a file path or staging location.
    #[tool(
        name = "music_get_job_status",
        description = "Report the state and progress of instrumental jobs by \
            their ids (`job_ids`) and/or a `batch_id` (at least one required). \
            Each job reports its state (queued/running/done/failed/cancelled), \
            progress in permille, and the saved result track id once promoted; a \
            queried batch also returns aggregate progress. Read-only job \
            metadata, available under the 'library:read' capability. Never \
            returns file paths or staging locations."
    )]
    async fn music_get_job_status(
        &self,
        Parameters(params): Parameters<JobStatusParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let db_path = self.db_path.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            data::job_status(
                db_path.as_path(),
                &params.job_ids,
                params.batch_id.as_deref(),
            )
        })
        .await
        .map_err(|error| error::join_error(&error))?;

        match outcome {
            Ok(result) => {
                let summary = match &result.batch {
                    Some(batch) => format!(
                        "{} job(s); batch {} at {}permille ({}/{} done)",
                        result.jobs.len(),
                        batch.batch_id,
                        batch.permille,
                        batch.done,
                        batch.total
                    ),
                    None => format!("{} job(s)", result.jobs.len()),
                };
                error::structured_ok(&result, summary)
            }
            Err(err) => error::into_tool_outcome(err),
        }
    }
}

// The two playback tools live in their own `#[tool_router]`-decorated impl
// block, gated on the `mpris` feature, rather than as individually
// `#[cfg(...)]`-gated methods inside the block above: `#[tool_router]`
// collects its routes by scanning for the `#[tool]` attribute token
// syntactically (see rmcp-macros' `tool_router.rs`) and has no visibility into
// per-method `#[cfg]` gates, so a mixed block would still reference these
// methods' generated `_tool_attr` helpers even when `mpris` is off. The two
// routers are combined in `RepriseServer::build_tool_router`.
#[cfg(feature = "mpris")]
#[tool_router(router = playback_tool_router)]
impl RepriseServer {
    /// Sends a transport action to the running app's MPRIS player.
    #[tool(
        name = "music_playback_control",
        description = "Control the running Reprise app's playback: action is one of \
            'play', 'pause', 'stop', 'next', 'previous'. Requires the app to be \
            running and the 'playback:control' capability (on by default)."
    )]
    async fn music_playback_control(
        &self,
        Parameters(params): Parameters<PlaybackControlParams>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(denial) = self.playback_allowed().await? {
            return Ok(denial);
        }
        let Some(action) = crate::playback::TransportAction::from_str(&params.action) else {
            return Ok(error::tool_error(format!(
                "unknown action '{}'",
                params.action
            )));
        };
        let result = tokio::task::spawn_blocking(move || crate::playback::transport(action))
            .await
            .map_err(|error| error::join_error(&error))?;
        error::playback_outcome(result, format!("Playback: {}", params.action))
    }

    /// Reads path-free live state from the running app's MPRIS player.
    #[tool(
        name = "music_get_playback_state",
        description = "Read the running Reprise app's current playback status, \
            track metadata, position, volume, shuffle and repeat state. Never \
            returns file or cover paths. Requires the app running and the \
            'playback:control' capability (on by default)."
    )]
    async fn music_get_playback_state(
        &self,
        Parameters(_params): Parameters<PlaybackStateParams>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(denial) = self.playback_allowed().await? {
            return Ok(denial);
        }
        let result = tokio::task::spawn_blocking(crate::playback::state)
            .await
            .map_err(|error| error::join_error(&error))?;
        error::playback_structured_outcome(result, |state| format!("Playback is {}", state.status))
    }

    /// Changes a live player setting through MPRIS.
    #[tool(
        name = "music_set_playback",
        description = "Change a running Reprise playback setting. Actions: \
            set_volume with volume 0..1; seek with a relative offset_seconds; \
            set_shuffle with enabled; set_repeat with repeat off, all or one. \
            Requires the app running and the 'playback:control' capability."
    )]
    async fn music_set_playback(
        &self,
        Parameters(params): Parameters<SetPlaybackParams>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(denial) = self.playback_allowed().await? {
            return Ok(denial);
        }
        let setting = match crate::playback::PlaybackSetting::from_params(&params) {
            Ok(setting) => setting,
            Err(message) => return Ok(error::tool_error(message)),
        };
        let result = tokio::task::spawn_blocking(move || crate::playback::set(setting))
            .await
            .map_err(|error| error::join_error(&error))?;
        match result {
            Ok(summary) => error::playback_outcome(Ok(()), summary),
            Err(error) => error::playback_outcome(Err(error), String::new()),
        }
    }

    /// Reads or safely mutates the running app's manual Play Next queue.
    #[tool(
        name = "music_queue",
        description = "Read or update the running Reprise Play Next queue. \
            Actions: status; add_next or add_last with track_ids; clear. Clear \
            removes only manual Play Next entries and preserves the playback \
            context. Status returns at most 200 ids per section plus complete \
            totals. Requires the 'playback:control' capability."
    )]
    async fn music_queue(
        &self,
        Parameters(params): Parameters<QueueParams>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(denial) = self.playback_allowed().await? {
            return Ok(denial);
        }
        let action = match crate::playback::QueueAction::from_params(&params) {
            Ok(action) => action,
            Err(message) => return Ok(error::tool_error(message)),
        };
        if action == crate::playback::QueueAction::Status {
            let result = tokio::task::spawn_blocking(crate::playback::queue_state)
                .await
                .map_err(|error| error::join_error(&error))?;
            return error::playback_structured_outcome(result, |state| {
                format!(
                    "{} Play Next and {} context track(s)",
                    state.play_next_total, state.context_total
                )
            });
        }
        let track_ids = match &action {
            crate::playback::QueueAction::AddNext(ids)
            | crate::playback::QueueAction::AddLast(ids) => Some(ids.clone()),
            crate::playback::QueueAction::Status | crate::playback::QueueAction::Clear => None,
        };
        if let Some(track_ids) = track_ids {
            let path = self.db_path.clone();
            let outcome = tokio::task::spawn_blocking(move || {
                data::validate_present_track_ids(path.as_path(), &track_ids)
            })
            .await
            .map_err(|error| error::join_error(&error))?;
            if let Err(err) = outcome {
                return error::into_tool_outcome(err);
            }
        }
        let result = tokio::task::spawn_blocking(move || crate::playback::queue_mutate(action))
            .await
            .map_err(|error| error::join_error(&error))?;
        match result {
            Ok(summary) => error::playback_outcome(Ok(()), summary),
            Err(error) => error::playback_outcome(Err(error), String::new()),
        }
    }

    /// Starts playing an explicit list of tracks or a whole playlist in the
    /// running app.
    #[tool(
        name = "music_play",
        description = "Start playing an explicit list of tracks or a whole playlist \
            in the running Reprise app. Provide exactly one of track_ids or \
            playlist_id. Requires the app running and the 'playback:control' \
            capability (on by default)."
    )]
    async fn music_play(
        &self,
        Parameters(params): Parameters<PlayParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let path = self.db_path.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            if !data::playback_allowed(path.as_path())? {
                return Err(data::DataError::CapabilityDenied("playback:control"));
            }
            data::resolve_play_ids(path.as_path(), &params)
        })
        .await
        .map_err(|error| error::join_error(&error))?;
        let ids = match outcome {
            Ok(ids) => ids,
            Err(err) => return error::into_tool_outcome(err),
        };
        let count = ids.len();
        let result = tokio::task::spawn_blocking(move || crate::playback::play_track_ids(ids))
            .await
            .map_err(|error| error::join_error(&error))?;
        error::playback_outcome(result, format!("Playing {count} track(s)"))
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
            Resource::new(RESOURCE_CONCERTS, "concerts")
                .with_description(
                    "Upcoming concerts for library artists after saved filters: dates, \
                     venues, cities, ticket links. No file paths.",
                )
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
            RESOURCE_CONCERTS => {
                let outcome =
                    tokio::task::spawn_blocking(move || data::list_concerts(path.as_path()))
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
