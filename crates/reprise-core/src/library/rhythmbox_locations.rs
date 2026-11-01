//! Where Rhythmbox keeps its library, found without the user naming it.
//!
//! Rhythmbox stores `rhythmdb.xml` and `playlists.xml` side by side. Which
//! directory that is depends on how each of the two applications is packaged:
//! a native Reprise reads the user's own data directory, a Reprise inside a
//! Flatpak sandbox sees a different one and has to look at the host's, and a
//! Rhythmbox installed from Flathub keeps its files in its own sandbox
//! directory. The resolver takes every input explicitly so each branch is a
//! table row in a test, and only [`from_environment`] touches the process.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Overrides the Rhythmbox database file; the importer's long-standing escape hatch.
pub const RHYTHMDB_PATH_ENV: &str = "REPRISE_RHYTHMDB_PATH";
/// Overrides the Rhythmbox playlists file.
pub const PLAYLISTS_PATH_ENV: &str = "REPRISE_RHYTHMBOX_PLAYLISTS_PATH";
/// The host's data directory as a sandboxed process can name it.
pub const HOST_XDG_DATA_HOME_ENV: &str = "HOST_XDG_DATA_HOME";

const RHYTHMDB_FILE: &str = "rhythmdb.xml";
const PLAYLISTS_FILE: &str = "playlists.xml";
const RHYTHMBOX_DIRECTORY: &str = "rhythmbox";
/// Below `$HOME`: the host data directory when nothing else says otherwise.
const DEFAULT_HOST_DATA_DIRECTORY: &str = ".local/share";
/// Below `$HOME`: Rhythmbox installed from Flathub (`org.gnome.Rhythmbox3`).
const FLATHUB_RHYTHMBOX_DIRECTORY: &str = ".var/app/org.gnome.Rhythmbox3/data/rhythmbox";

/// Everything the resolver depends on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocationInputs {
    /// `REPRISE_RHYTHMDB_PATH`: the database file itself.
    pub rhythmdb_override: Option<OsString>,
    /// `REPRISE_RHYTHMBOX_PLAYLISTS_PATH`: the playlists file itself.
    pub playlists_override: Option<OsString>,
    /// Whether this process runs inside a Flatpak sandbox.
    pub in_flatpak: bool,
    pub home: Option<PathBuf>,
    pub host_xdg_data_home: Option<PathBuf>,
    /// The data directory of this process: the user's own when native, the
    /// sandbox's inside a Flatpak.
    pub native_data_dir: PathBuf,
}

/// The two files an import reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RhythmboxLocations {
    pub rhythmdb: PathBuf,
    pub playlists: PathBuf,
}

/// Collects the inputs from the process environment. An empty variable counts
/// as unset, and `HOME` and `HOST_XDG_DATA_HOME` must be absolute paths.
#[must_use]
pub fn inputs_from(
    in_flatpak: bool,
    native_data_dir: PathBuf,
    var: impl Fn(&str) -> Option<OsString>,
) -> LocationInputs {
    let non_empty = |name: &str| var(name).filter(|value| !value.is_empty());
    let absolute = |name: &str| {
        non_empty(name)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    LocationInputs {
        rhythmdb_override: non_empty(RHYTHMDB_PATH_ENV),
        playlists_override: non_empty(PLAYLISTS_PATH_ENV),
        in_flatpak,
        home: absolute("HOME"),
        host_xdg_data_home: absolute(HOST_XDG_DATA_HOME_ENV),
        native_data_dir,
    }
}

/// The directories that may hold Rhythmbox's files, most likely first.
///
/// Inside a Flatpak that is the host's `rhythmbox` directory (`$HOST_XDG_DATA_HOME`,
/// else `$HOME/.local/share`); outside it is the native data directory's. The
/// Flathub Rhythmbox's sandbox directory follows in both cases.
#[must_use]
pub fn candidate_dirs(inputs: &LocationInputs) -> Vec<PathBuf> {
    let primary = if inputs.in_flatpak {
        inputs
            .host_xdg_data_home
            .clone()
            .or_else(|| {
                inputs
                    .home
                    .as_ref()
                    .map(|home| home.join(DEFAULT_HOST_DATA_DIRECTORY))
            })
            .map(|data| data.join(RHYTHMBOX_DIRECTORY))
    } else {
        Some(inputs.native_data_dir.join(RHYTHMBOX_DIRECTORY))
    };
    let flathub = inputs
        .home
        .as_ref()
        .map(|home| home.join(FLATHUB_RHYTHMBOX_DIRECTORY));
    primary.into_iter().chain(flathub).collect()
}

/// Picks the database and playlists files.
///
/// The override wins untouched. Otherwise the first candidate directory for
/// which `is_file` finds a `rhythmdb.xml` wins, and with none the first
/// candidate stays the answer, so a missing library is reported at the place
/// the user would most expect it. `playlists.xml` is the database's sibling
/// unless its own override is set.
pub fn resolve(inputs: &LocationInputs, is_file: impl Fn(&Path) -> bool) -> RhythmboxLocations {
    let rhythmdb = match &inputs.rhythmdb_override {
        Some(path) => PathBuf::from(path),
        None => {
            let databases: Vec<PathBuf> = candidate_dirs(inputs)
                .into_iter()
                .map(|dir| dir.join(RHYTHMDB_FILE))
                .collect();
            databases
                .iter()
                .find(|database| is_file(database))
                .or_else(|| databases.first())
                .cloned()
                .unwrap_or_else(|| {
                    inputs
                        .native_data_dir
                        .join(RHYTHMBOX_DIRECTORY)
                        .join(RHYTHMDB_FILE)
                })
        }
    };
    let playlists = playlists_path(&rhythmdb, inputs.playlists_override.as_deref());
    RhythmboxLocations {
        rhythmdb,
        playlists,
    }
}

/// The playlists file for a database file: the override, else its sibling.
#[must_use]
pub fn playlists_path(rhythmdb: &Path, playlists_override: Option<&std::ffi::OsStr>) -> PathBuf {
    playlists_override.map_or_else(|| rhythmdb.with_file_name(PLAYLISTS_FILE), PathBuf::from)
}

/// [`resolve`] against the real process environment and file system.
#[must_use]
pub fn from_environment(in_flatpak: bool, native_data_dir: PathBuf) -> RhythmboxLocations {
    let inputs = inputs_from(in_flatpak, native_data_dir, |name| std::env::var_os(name));
    resolve(&inputs, Path::is_file)
}

/// The playlists file that goes with `rhythmdb`, honouring the process environment.
#[must_use]
pub fn playlists_path_from_environment(rhythmdb: &Path) -> PathBuf {
    playlists_path(
        rhythmdb,
        std::env::var_os(PLAYLISTS_PATH_ENV)
            .filter(|value| !value.is_empty())
            .as_deref(),
    )
}

#[cfg(test)]
#[path = "rhythmbox_locations_tests.rs"]
mod tests;
