//! Mapping from [`DataError`] to MCP results.
//!
//! Two failure channels, per the SDK's own guidance and spec D19:
//! - caller-fixable failures (denied capability, invalid input) become a
//!   caller-visible [`CallToolResult`] error so the agent reads the reason;
//! - infrastructure failures become an opaque protocol [`ErrorData`] whose
//!   detail is logged to stderr and never sent to the client (no leaks).

use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::ErrorData;
use serde::Serialize;
use tokio::task::JoinError;

use crate::data::DataError;

/// A `spawn_blocking` join failure (panic/cancellation) is a server-internal
/// fault: log it, return an opaque protocol error.
pub fn join_error(error: &JoinError) -> ErrorData {
    tracing::error!(error = %error, "MCP worker task failed to join");
    ErrorData::internal_error("internal server error", None)
}

/// Builds a successful tool result carrying both a structured JSON body (for
/// capable clients) and a short text summary (spec §9: structured output plus
/// a text fallback).
pub fn structured_ok<T: Serialize>(
    value: &T,
    summary: String,
) -> Result<CallToolResult, ErrorData> {
    let json = serde_json::to_value(value).map_err(|error| {
        tracing::error!(error = %error, "failed to serialize MCP tool result");
        ErrorData::internal_error("internal serialization error", None)
    })?;
    let mut result = CallToolResult::success(vec![ContentBlock::text(summary)]);
    result.structured_content = Some(json);
    Ok(result)
}

/// Serializes a resource body to a JSON string, mapping failure to an opaque
/// protocol error.
pub fn serialize_resource<T: Serialize>(value: &T) -> Result<String, ErrorData> {
    serde_json::to_string(value).map_err(|error| {
        tracing::error!(error = %error, "failed to serialize MCP resource");
        ErrorData::internal_error("internal serialization error", None)
    })
}

/// Maps a [`DataError`] to a tool-call outcome (`music_*` tools).
pub fn into_tool_outcome(error: DataError) -> Result<CallToolResult, ErrorData> {
    match error {
        // `playback:control` is read live (see `playback_denied`'s doc comment)
        // so it gets its own restart-free message rather than the generic
        // write-cap one below.
        DataError::CapabilityDenied("playback:control") => Ok(playback_denied()),
        DataError::CapabilityDenied(cap) => Ok(tool_error(format!(
            "Permission denied: the '{cap}' capability is not granted. \
             Enable it in Reprise and restart the MCP server."
        ))),
        DataError::InvalidInput(message) => Ok(tool_error(message)),
        internal @ (DataError::Db(_) | DataError::Open(_)) => {
            tracing::error!(error = %internal, "internal error handling MCP tool call");
            Err(ErrorData::internal_error("internal server error", None))
        }
    }
}

/// Maps a [`DataError`] to a resource-read protocol error. Resources have no
/// tool-style error channel, so caller-fixable errors keep a clear (path-free)
/// message while infrastructure detail is logged, not leaked.
pub fn resource_error(error: DataError) -> ErrorData {
    match error {
        DataError::CapabilityDenied(cap) => {
            ErrorData::invalid_request(format!("the '{cap}' capability is not granted"), None)
        }
        DataError::InvalidInput(message) => ErrorData::invalid_params(message, None),
        internal @ (DataError::Db(_) | DataError::Open(_)) => {
            tracing::error!(error = %internal, "internal error handling MCP resource read");
            ErrorData::internal_error("internal server error", None)
        }
    }
}

pub fn tool_error(message: String) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message)])
}

/// The shared `playback:control` denial message, used verbatim by both
/// `music_playback_control` (denies inline) and `music_play` (denies via
/// [`into_tool_outcome`]). Deliberately does *not* tell the caller to restart
/// the MCP server: unlike the write-class caps (`playlist:create`,
/// `ai:create`), which are captured in an immutable startup snapshot,
/// `playback:control` is read live on every call — a re-grant takes effect on
/// the very next tool call. Not `#[cfg(feature = "mpris")]`: it is reached
/// through [`into_tool_outcome`], which compiles regardless of that feature.
pub fn playback_denied() -> CallToolResult {
    tool_error(
        "Permission denied: the 'playback:control' capability is not granted. \
         Enable it in Reprise."
            .to_owned(),
    )
}

/// Builds a successful tool result carrying only a short text summary — the
/// plain-text counterpart to [`structured_ok`] for tools with no structured
/// body worth reporting (the playback tools: there is nothing to serialize
/// beyond a human-readable confirmation).
#[cfg(feature = "mpris")]
fn tool_text(summary: String) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(summary)])
}

/// Maps a [`crate::playback::PlaybackError`] to a tool-call outcome
/// (`music_playback_control`/`music_play`). A missing player is caller-fixable
/// (start the app) so it becomes a clear tool error; a genuine bus fault is
/// logged and returned as an opaque protocol error, matching
/// [`into_tool_outcome`]'s split between caller-fixable and internal failures.
#[cfg(feature = "mpris")]
pub fn playback_outcome(
    result: Result<(), crate::playback::PlaybackError>,
    ok_summary: String,
) -> Result<CallToolResult, ErrorData> {
    use crate::playback::PlaybackError;
    match result {
        Ok(()) => Ok(tool_text(ok_summary)),
        Err(PlaybackError::NoPlayer) => Ok(tool_error(
            "no running Reprise app on the session bus — start the app first".to_owned(),
        )),
        Err(PlaybackError::Bus(message)) => {
            tracing::error!(message, "playback bus error");
            Err(ErrorData::internal_error("internal server error", None))
        }
    }
}
