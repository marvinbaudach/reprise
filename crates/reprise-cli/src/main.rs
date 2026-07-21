//! `reprise-cli`: a headless, MIT-licensed command-line frontend over the
//! `reprise-core` engine. It opens the same SQLite library the GTK app uses
//! (WAL, so both can run at once) and drives it through core's `&Connection`
//! facades — every mutation lands in `change_log`, so a running app refreshes
//! live. See `cli` for the surface and `commands` for the implementations.

mod cli;
mod commands;
mod db_open;
mod error;
mod json_models;
mod output;
mod retry;

use clap::Parser;

use cli::{Cli, Command, EventsAction, LibraryAction, PlaylistAction};
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
