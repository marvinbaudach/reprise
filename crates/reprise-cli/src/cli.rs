//! Command-line surface (clap v4 derive).

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Headless command-line surface for the Reprise music library.
#[derive(Parser, Debug)]
#[command(name = "reprise-cli", version)]
pub struct Cli {
    /// Path to the Reprise database (defaults to the standard per-user
    /// location). Point this at a scratch file for tests or an alternate
    /// library.
    #[arg(long, global = true, value_name = "PATH")]
    pub db: Option<PathBuf>,

    /// Emit machine-readable JSON instead of human-readable text.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Create, inspect and manage playlists.
    Playlist {
        #[command(subcommand)]
        action: PlaylistAction,
    },
    /// Search the library by title, artist, album or genre.
    Search {
        /// Text to match (case-insensitive substring).
        query: String,
        /// Maximum number of tracks to return.
        #[arg(long, default_value_t = 50)]
        limit: i64,
        /// Number of leading matches to skip.
        #[arg(long, default_value_t = 0)]
        offset: i64,
    },
    /// Inspect the library as a whole.
    Library {
        #[command(subcommand)]
        action: LibraryAction,
    },
    /// Scan a folder into the library.
    Scan {
        /// Folder to scan. Defaults to the configured library root.
        path: Option<PathBuf>,
    },
    /// Inspect the cross-process change log (debugging aid).
    Events {
        #[command(subcommand)]
        action: EventsAction,
    },
}

#[derive(Subcommand, Debug)]
pub enum PlaylistAction {
    /// List all manual playlists.
    List,
    /// Show a playlist and its tracks.
    Show {
        /// Playlist id.
        id: i64,
    },
    /// Create a manual playlist.
    Create {
        /// Playlist name.
        name: String,
        /// Optional track ids to add, comma-separated (e.g. `--tracks 1,2,3`).
        #[arg(long, value_delimiter = ',')]
        tracks: Vec<i64>,
    },
    /// Rename a playlist.
    Rename {
        /// Playlist id.
        id: i64,
        /// New name.
        name: String,
    },
    /// Delete a playlist. Requires `--yes` to proceed.
    Delete {
        /// Playlist id.
        id: i64,
        /// Confirm the irreversible deletion.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum LibraryAction {
    /// Show track count and total duration.
    Summary,
}

#[derive(Subcommand, Debug)]
pub enum EventsAction {
    /// Print change-log rows newer than `--since` (default 0 = all retained).
    Tail {
        /// Only show rows with an id greater than this.
        #[arg(long, default_value_t = 0)]
        since: i64,
    },
}
