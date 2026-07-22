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

    /// Directory holding staged AI renders (defaults to the standard per-user
    /// location). Only the `instrumental` and `jobs` commands use it; app, CLI
    /// and worker must agree on it. Point it at a scratch dir for tests.
    #[arg(long, global = true, value_name = "PATH")]
    pub staging_dir: Option<PathBuf>,

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
    /// Create and manage AI instrumental (vocal-removal) versions
    /// (experimental).
    Instrumental {
        #[command(subcommand)]
        action: InstrumentalAction,
    },
    /// Inspect and run AI-audio jobs.
    Jobs {
        #[command(subcommand)]
        action: JobsAction,
    },
    /// Control the running Reprise app's playback over MPRIS (Linux; requires
    /// the `mpris` feature).
    #[cfg(feature = "mpris")]
    Playback {
        #[command(subcommand)]
        action: PlaybackAction,
    },
}

/// Playback transport actions (MPRIS). Only compiled with the `mpris` feature.
#[cfg(feature = "mpris")]
#[derive(Subcommand, Debug, Clone, Copy)]
pub enum PlaybackAction {
    /// Toggle play/pause.
    PlayPause,
    /// Skip to the next track.
    Next,
    /// Return to the previous track.
    Previous,
    /// Show the current playback status and track.
    Status,
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

#[derive(Subcommand, Debug)]
pub enum InstrumentalAction {
    /// Register instrumental jobs for one or more tracks. Multiple ids form one
    /// batch. A track already covered by an open/staged/saved job is a skip
    /// with a reference to the existing work (Beschluss 16), not a second
    /// render.
    Create {
        /// Source track ids to convert.
        #[arg(value_name = "TRACK_ID", required = true)]
        track_ids: Vec<i64>,
        /// Promote the finished render into the library — the default, because
        /// automation wants the end result (Beschluss 15). Conflicts with
        /// `--stage`; only observable together with `--wait` (or a later
        /// `instrumental save`).
        #[arg(long, conflicts_with = "stage")]
        save: bool,
        /// Leave the finished render in staging for an explicit later
        /// `instrumental save`/`discard` decision instead of saving it.
        #[arg(long)]
        stage: bool,
        /// Wait for each job to finish, reporting the outcome. Needs a running
        /// worker (the app or `reprise-cli jobs work`); without `--wait` the
        /// command returns as soon as the jobs are queued.
        #[arg(long)]
        wait: bool,
        /// With `--wait`, give up after this many seconds if the jobs make no
        /// progress (they stay queued for a worker to pick up later).
        #[arg(long, value_name = "SECS", default_value_t = 3600)]
        wait_timeout: u64,
    },
    /// Promote finished, staged renders into the library (the save decision).
    Save {
        /// Job ids of finished, unsaved renders to promote.
        #[arg(value_name = "JOB_ID", required = true)]
        job_ids: Vec<i64>,
    },
    /// Discard finished, staged renders (deletes the staging file).
    Discard {
        /// Job ids of finished, unsaved renders to discard.
        #[arg(value_name = "JOB_ID", required = true)]
        job_ids: Vec<i64>,
    },
}

#[derive(Subcommand, Debug)]
pub enum JobsAction {
    /// List AI-audio jobs with their state, progress and result track id.
    Status {
        /// Restrict to one batch id (as returned by `instrumental create`).
        #[arg(long, value_name = "BATCH_ID")]
        batch: Option<String>,
    },
    /// Run a standalone worker that claims and renders queued jobs (requires
    /// the `worker` feature).
    #[cfg(feature = "worker")]
    Work(crate::commands::worker::WorkerArgs),
}
