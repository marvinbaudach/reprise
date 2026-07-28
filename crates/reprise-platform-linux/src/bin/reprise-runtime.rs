//! The activatable Reprise runtime.
//!
//! Started by the session bus, not by a client (§9.4/1). There is exactly
//! one start path, because a client that spawns its own runtime would make
//! the single-owner lease an argument about nothing.
//!
//! The order below is the whole point of the program:
//!
//! 1. claim the lease — *before* GStreamer, the device layer or the database
//!    writer are opened, so a process that loses has never touched an effect
//!    (§9.3);
//! 2. build the runtime;
//! 3. publish `org.reprise.Reprise1` and serve until idle (§9.6).
//!
//! Losing at step 1 is `Refused`: exit with a structured cause, wait for
//! nothing, restart nothing.

use std::process::ExitCode;

use reprise_platform_linux::runtime_service::{
    compose, LeaseError, RuntimeLease, RuntimeService, ServeOptions, ServiceError, ServiceInbox,
};

/// The exit code a refused start uses, so systemd's journal distinguishes
/// "someone else owns this" from "this crashed".
const EXIT_REFUSED: u8 = 3;
/// Anything that stopped the runtime from starting for a different reason.
const EXIT_UNAVAILABLE: u8 = 4;

fn main() -> ExitCode {
    tracing_subscriber_init();

    let lease = match RuntimeLease::claim() {
        Ok(lease) => lease,
        Err(LeaseError::Held) => {
            // Not an error the user has to act on: something already serves
            // the runtime, which is exactly the invariant being upheld.
            tracing::info!("another Reprise runtime holds the lease; exiting");
            return ExitCode::from(EXIT_REFUSED);
        }
        Err(error) => {
            tracing::error!(%error, "cannot claim the runtime lease");
            return ExitCode::from(EXIT_UNAVAILABLE);
        }
    };
    tracing::info!(lease = %lease.path().display(), "runtime lease held");

    let database = reprise_core::db::default_path();

    let inbox = ServiceInbox::new();
    let composition = match compose(&database, inbox.sender()) {
        Ok(composition) => composition,
        Err(error) => {
            tracing::error!(%error, "cannot build the runtime");
            return ExitCode::from(EXIT_UNAVAILABLE);
        }
    };

    match RuntimeService::serve(
        composition.runtime,
        lease,
        &ServeOptions::default(),
        inbox,
        Some(composition.player_events),
    ) {
        Ok(()) => {
            tracing::info!("runtime stopped");
            ExitCode::SUCCESS
        }
        Err(ServiceError::Bus(error)) => {
            tracing::error!(%error, "cannot serve on the session bus");
            ExitCode::from(EXIT_UNAVAILABLE)
        }
        Err(error) => {
            tracing::error!(%error, "runtime service failed to start");
            ExitCode::from(EXIT_UNAVAILABLE)
        }
    }
}

/// Logs to stderr, which systemd routes into the journal. Deliberately not
/// stdout: a runtime started by an MCP server's activation must never write
/// there, because that stream is the protocol (§9.7).
fn tracing_subscriber_init() {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "info");
    }
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
}
