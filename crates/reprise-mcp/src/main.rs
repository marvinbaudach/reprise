//! `reprise-mcp` — a local, stdio-only Model Context Protocol server exposing
//! the Reprise library to agents over the official Rust SDK (`rmcp`, pinned).
//!
//! Transport is stdio only (spec D16): stdout carries protocol frames and
//! nothing else; all logging goes to stderr (spec §9). Data is reached
//! exclusively through `reprise-core` facades, and every response follows the
//! D19 leak matrix (never a path, XDG/cache/db path, lyric, serial, credential
//! or raw listen event).

mod capability;
mod catalog_resources;
mod config;
mod data;
mod data_concerts;
#[cfg(feature = "mpris")]
mod device_dto;
#[cfg(feature = "mpris")]
mod device_sync;
#[cfg(feature = "mpris")]
mod device_tools;
mod dto;
mod error;
#[cfg(feature = "mpris")]
mod playback;
mod playlist_update;
mod server;
mod source_actions;
mod source_data;
mod source_tools;
mod startup;
mod stdin_cap;

use std::path::PathBuf;
use std::process::ExitCode;

use rmcp::ServiceExt;

// Exit codes the test harness / scripts can tell apart.
const EXIT_BAD_ARGS: u8 = 2;
const EXIT_SCHEMA_TOO_NEW: u8 = 3;

fn main() -> ExitCode {
    init_tracing();

    let config = match config::Config::from_args(std::env::args().skip(1)) {
        Ok(config) => config,
        Err(message) => {
            eprintln!("reprise-mcp: {message}");
            return ExitCode::from(EXIT_BAD_ARGS);
        }
    };

    let db_path = config.database_path();
    let staging_path = config.staging_path();
    let caps = match startup::prepare(&db_path) {
        Ok(caps) => caps,
        Err(startup::StartupError::SchemaTooNew { found, supported }) => {
            eprintln!(
                "reprise-mcp: database schema {found} is newer than this server \
                 supports ({supported}); please update reprise-mcp"
            );
            return ExitCode::from(EXIT_SCHEMA_TOO_NEW);
        }
        Err(startup::StartupError::Open(error)) => {
            eprintln!("reprise-mcp: failed to open database: {error}");
            return ExitCode::FAILURE;
        }
        Err(startup::StartupError::Query(error)) => {
            eprintln!("reprise-mcp: failed to read capabilities: {error}");
            return ExitCode::FAILURE;
        }
    };

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("reprise-mcp: failed to start async runtime: {error}");
            return ExitCode::FAILURE;
        }
    };

    runtime.block_on(serve(db_path, staging_path, caps))
}

async fn serve(db_path: PathBuf, staging_path: PathBuf, caps: startup::StartupCaps) -> ExitCode {
    let handler = server::RepriseServer::new(
        db_path,
        staging_path,
        caps.playlist_create,
        caps.playlist_manage,
        caps.ai_create,
        caps.sources_manage,
        #[cfg(feature = "mpris")]
        caps.device_sync,
    );
    // Cap stdin per line so a hostile or newline-less client cannot OOM the
    // process through rmcp's unbounded `read_until` (see `stdin_cap`). `serve`
    // accepts an `(AsyncRead, AsyncWrite)` pair as its transport, so we swap the
    // default `rmcp::transport::stdio()` reader for the capped one.
    let transport = (
        stdin_cap::LineCappedReader::new(tokio::io::stdin(), stdin_cap::MAX_LINE_BYTES),
        tokio::io::stdout(),
    );
    let service = match handler.serve(transport).await {
        Ok(service) => service,
        Err(error) => {
            tracing::error!(error = ?error, "failed to start MCP server");
            return ExitCode::FAILURE;
        }
    };
    tracing::info!("reprise-mcp stdio server ready");
    match service.waiting().await {
        Ok(_reason) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(error = ?error, "MCP service ended with error");
            ExitCode::FAILURE
        }
    }
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_env("REPRISE_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    // Protocol cleanliness (spec §9 / D16): every log line goes to stderr so
    // stdout stays byte-for-byte MCP JSON-RPC.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .try_init();
}
