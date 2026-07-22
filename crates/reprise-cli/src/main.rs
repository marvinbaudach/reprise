//! `reprise-cli`: a headless, MIT-licensed command-line frontend over the
//! `reprise-core` engine. It opens the same SQLite library the GTK app uses
//! (WAL, so both can run at once) and drives it through core's `&Connection`
//! facades — every mutation lands in `change_log`, so a running app refreshes
//! live. See `cli` for the surface and `commands` for the implementations.

mod cli;
mod clock;
mod commands;
mod db_open;
mod error;
mod json_models;
mod output;
mod retry;
mod staging;

use clap::Parser;

use cli::{
    Cli, Command, EventsAction, InstrumentalAction, JobsAction, LibraryAction, PlaylistAction,
};
use commands::instrumental::SaveMode;
use error::{CliError, ExitCode};

fn main() -> std::process::ExitCode {
    let code = match Cli::try_parse() {
        Ok(cli) => match run(cli) {
            Ok(()) => ExitCode::Success,
            Err(error) => {
                eprintln!("{error}");
                error.exit_code()
            }
        },
        // clap already renders help/version/usage text; adopt its exit
        // convention through our own typed codes (help/version to stdout with
        // Success, real argument errors to stderr with Usage).
        Err(clap_error) => {
            let _ = clap_error.print();
            if clap_error.use_stderr() {
                ExitCode::Usage
            } else {
                ExitCode::Success
            }
        }
    };
    std::process::ExitCode::from(code.as_u8())
}

fn run(cli: Cli) -> Result<(), CliError> {
    let json = cli.json;
    let staging_dir = cli.staging_dir;
    let mut conn = db_open::open(cli.db.as_ref())?;

    match cli.command {
        Command::Playlist { action } => run_playlist(&mut conn, action, json),
        Command::Search {
            query,
            limit,
            offset,
        } => commands::search::run(&mut conn, &query, limit, offset, json),
        Command::Library { action } => match action {
            LibraryAction::Summary => commands::library::summary(&conn, json),
        },
        Command::Scan { path } => commands::scan::run(&mut conn, path, json),
        Command::Events { action } => match action {
            EventsAction::Tail { since } => commands::events::tail(&conn, since, json),
        },
        Command::Instrumental { action } => {
            run_instrumental(&mut conn, staging_dir.as_ref(), action, json)
        }
        Command::Jobs { action } => run_jobs(&mut conn, staging_dir.as_ref(), action, json),
    }
}

fn run_instrumental(
    conn: &mut rusqlite::Connection,
    staging_dir: Option<&std::path::PathBuf>,
    action: InstrumentalAction,
    json: bool,
) -> Result<(), CliError> {
    match action {
        InstrumentalAction::Create {
            track_ids,
            save,
            stage,
            wait,
            wait_timeout,
        } => {
            let mode = SaveMode::from_flags(save, stage);
            let waiting = commands::instrumental::WaitOptions::new(wait, wait_timeout);
            commands::instrumental::create(conn, staging_dir, &track_ids, mode, waiting, json)
        }
        InstrumentalAction::Save { job_ids } => {
            commands::instrumental::save(conn, staging_dir, &job_ids, json)
        }
        InstrumentalAction::Discard { job_ids } => {
            commands::instrumental::discard(conn, staging_dir, &job_ids, json)
        }
    }
}

fn run_jobs(
    conn: &mut rusqlite::Connection,
    staging_dir: Option<&std::path::PathBuf>,
    action: JobsAction,
    json: bool,
) -> Result<(), CliError> {
    match action {
        JobsAction::Status { batch } => {
            commands::jobs::status(conn, staging_dir, batch.as_deref(), json)
        }
        #[cfg(feature = "worker")]
        JobsAction::Work(args) => commands::worker::run(conn, staging_dir, &args, json),
    }
}

fn run_playlist(
    conn: &mut rusqlite::Connection,
    action: PlaylistAction,
    json: bool,
) -> Result<(), CliError> {
    match action {
        PlaylistAction::List => commands::playlist::list(conn, json),
        PlaylistAction::Show { id } => commands::playlist::show(conn, id, json),
        PlaylistAction::Create { name, tracks } => {
            commands::playlist::create(conn, &name, &tracks, json)
        }
        PlaylistAction::Rename { id, name } => commands::playlist::rename(conn, id, &name, json),
        PlaylistAction::Delete { id, yes } => commands::playlist::delete(conn, id, yes, json),
    }
}
