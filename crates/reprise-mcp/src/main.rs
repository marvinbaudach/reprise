//! `reprise-mcp` — a local, stdio-only Model Context Protocol server exposing
//! the Reprise library to agents over the official Rust SDK (`rmcp`, pinned).
//!
//! Transport is stdio only (spec D16): stdout carries protocol frames and
//! nothing else; all logging goes to stderr (spec §9). Data is reached
//! exclusively through `reprise-core` facades, and every response follows the
//! D19 leak matrix (never a path, XDG/cache/db path, lyric, serial, credential
//! or raw listen event).

mod capability;
mod config;
mod data;
mod dto;
mod error;
mod server;
mod startup;

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
    let write_granted_at_startup = match startup::prepare(&db_path) {
        Ok(granted) => granted,
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

    runtime.block_on(serve(db_path, write_granted_at_startup))
}

async fn serve(db_path: PathBuf, write_granted_at_startup: bool) -> ExitCode {
    let handler = server::RepriseServer::new(db_path, write_granted_at_startup);
    let service = match handler.serve(rmcp::transport::stdio()).await {
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
