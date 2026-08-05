//! MCP tool-router surface for the local Sound Similarity module.
//!
//! Its own router (like `source_tools.rs`) rather than two more methods in
//! `server.rs`: that file is already close to the 800-line ceiling
//! `scripts/check-architecture.sh` enforces.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router, ErrorData};

use crate::server::RepriseServer;
use crate::sound_similarity::{SimilarTracksParams, SoundProfileParams};

#[tool_router(router = sound_tool_router, vis = "pub(crate)")]
impl RepriseServer {
    #[tool(
        name = "music_similar_tracks",
        description = "Rank the local library by how similar it sounds to one \
            track, using the derived sound profiles (timbre, dynamics, rhythm) \
            — never online data. Returns matches nearest-first with opaque \
            track ids, title, artist, album, a weighted `distance` (smaller is \
            nearer) and a `percentile` giving the share of the compared \
            population that lies farther away, plus `compared_tracks`, the \
            size of that population. `limit` (1..=50, default 7), `weighting` \
            (default|timbre|dynamics), `exclude_same_album` (default true) and \
            `exclude_same_artist` (default false) override the module's \
            shipped defaults; at most two matches carry the same artist \
            whatever those are set to. Profiles are derived in the background \
            by the Reprise app, so a library that has none yet reports \
            `status` `no_profiles_yet` and an unprofiled track reports \
            `track_not_analysed`, each with the `profiles_ready` of \
            `library_tracks` inventory — never a silently empty list. \
            Read-only, no capability beyond the base read access."
    )]
    async fn music_similar_tracks(
        &self,
        Parameters(params): Parameters<SimilarTracksParams>,
    ) -> Result<rmcp::model::CallToolResult, ErrorData> {
        let path = self.source_db_path();
        let outcome = tokio::task::spawn_blocking(move || {
            crate::sound_similarity::similar_tracks(path.as_path(), &params)
        })
        .await
        .map_err(|error| crate::error::join_error(&error))?;

        match outcome {
            Ok(result) => {
                let summary = format!(
                    "{} match(es) for track {} among {} compared track(s)",
                    result.matches.len(),
                    result.track_id,
                    result.compared_tracks
                );
                crate::error::structured_ok(&result, summary)
            }
            Err(error) => crate::error::into_tool_outcome(error),
        }
    }

    #[tool(
        name = "music_sound_profile",
        description = "Read one track's stored sound profile: its three axis \
            positions as library-wide percentiles (timbre dark-to-bright, \
            dynamics dense-to-open, tempo slow-to-fast — the same derivation \
            the app's Sound tab draws), plus the file line (format, bit depth, \
            sample rate, bitrate, size in bytes, and the highest frequency the \
            recording still occupies). The tempo axis is null when no stable \
            tempo estimate was found. A track without a derived profile \
            reports `status` `track_not_analysed`, a library without any \
            reports `no_profiles_yet`, both with the `profiles_ready` of \
            `library_tracks` inventory. Read-only and never returns a file \
            path."
    )]
    async fn music_sound_profile(
        &self,
        Parameters(params): Parameters<SoundProfileParams>,
    ) -> Result<rmcp::model::CallToolResult, ErrorData> {
        let path = self.source_db_path();
        let outcome = tokio::task::spawn_blocking(move || {
            crate::sound_similarity::sound_profile(path.as_path(), &params)
        })
        .await
        .map_err(|error| crate::error::join_error(&error))?;

        match outcome {
            Ok(result) => {
                let summary = format!(
                    "Sound profile for track {}: {}",
                    result.track_id, result.status
                );
                crate::error::structured_ok(&result, summary)
            }
            Err(error) => crate::error::into_tool_outcome(error),
        }
    }
}
