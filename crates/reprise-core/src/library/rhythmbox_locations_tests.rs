use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::*;

const HOME: &str = "/home/tester";
const NATIVE: &str = "/home/tester/.local/share";
const SANDBOX: &str = "/home/tester/.var/app/io.github.marvinbaudach.Reprise/data";
const FLATHUB_DIR: &str = "/home/tester/.var/app/org.gnome.Rhythmbox3/data/rhythmbox";

fn inputs(in_flatpak: bool, home: Option<&str>, host: Option<&str>, data: &str) -> LocationInputs {
    LocationInputs {
        in_flatpak,
        home: home.map(PathBuf::from),
        host_xdg_data_home: host.map(PathBuf::from),
        native_data_dir: PathBuf::from(data),
        ..LocationInputs::default()
    }
}

fn paths(list: &[&str]) -> Vec<PathBuf> {
    list.iter().map(PathBuf::from).collect()
}

fn existing(files: &[&str]) -> impl Fn(&Path) -> bool {
    let files: HashSet<PathBuf> = files.iter().map(PathBuf::from).collect();
    move |path| files.contains(path)
}

#[test]
fn candidate_directories_follow_the_packaging_in_order() {
    struct Row {
        name: &'static str,
        inputs: LocationInputs,
        expected: Vec<PathBuf>,
    }
    let rows = [
        Row {
            name: "native: own data directory, then Rhythmbox from Flathub",
            inputs: inputs(false, Some(HOME), None, NATIVE),
            expected: paths(&["/home/tester/.local/share/rhythmbox", FLATHUB_DIR]),
        },
        Row {
            name: "native without HOME: own data directory only",
            inputs: inputs(false, None, None, NATIVE),
            expected: paths(&["/home/tester/.local/share/rhythmbox"]),
        },
        Row {
            name: "native ignores HOST_XDG_DATA_HOME",
            inputs: inputs(false, Some(HOME), Some("/elsewhere"), NATIVE),
            expected: paths(&["/home/tester/.local/share/rhythmbox", FLATHUB_DIR]),
        },
        Row {
            name: "flatpak with HOST_XDG_DATA_HOME: the host's directory, then Flathub",
            inputs: inputs(true, Some(HOME), Some("/host/data"), SANDBOX),
            expected: paths(&["/host/data/rhythmbox", FLATHUB_DIR]),
        },
        Row {
            name: "flatpak without HOST_XDG_DATA_HOME: $HOME/.local/share, then Flathub",
            inputs: inputs(true, Some(HOME), None, SANDBOX),
            expected: paths(&["/home/tester/.local/share/rhythmbox", FLATHUB_DIR]),
        },
        Row {
            name: "flatpak never offers the sandbox's own directory",
            inputs: inputs(true, Some(HOME), None, SANDBOX),
            expected: paths(&["/home/tester/.local/share/rhythmbox", FLATHUB_DIR]),
        },
        Row {
            name: "flatpak without HOME but with HOST_XDG_DATA_HOME: the host's only",
            inputs: inputs(true, None, Some("/host/data"), SANDBOX),
            expected: paths(&["/host/data/rhythmbox"]),
        },
        Row {
            name: "flatpak with neither: nothing to probe",
            inputs: inputs(true, None, None, SANDBOX),
            expected: Vec::new(),
        },
    ];

    for row in rows {
        assert_eq!(candidate_dirs(&row.inputs), row.expected, "{}", row.name);
    }
}

#[test]
fn the_first_directory_holding_the_database_wins() {
    struct Row {
        name: &'static str,
        inputs: LocationInputs,
        present: &'static [&'static str],
        expected: &'static str,
    }
    let rows = [
        Row {
            name: "native data directory holds it",
            inputs: inputs(false, Some(HOME), None, NATIVE),
            present: &["/home/tester/.local/share/rhythmbox/rhythmdb.xml"],
            expected: "/home/tester/.local/share/rhythmbox/rhythmdb.xml",
        },
        Row {
            name: "both hold it: the earlier candidate wins",
            inputs: inputs(false, Some(HOME), None, NATIVE),
            present: &[
                "/home/tester/.local/share/rhythmbox/rhythmdb.xml",
                "/home/tester/.var/app/org.gnome.Rhythmbox3/data/rhythmbox/rhythmdb.xml",
            ],
            expected: "/home/tester/.local/share/rhythmbox/rhythmdb.xml",
        },
        Row {
            name: "only the Flathub Rhythmbox holds it",
            inputs: inputs(false, Some(HOME), None, NATIVE),
            present: &["/home/tester/.var/app/org.gnome.Rhythmbox3/data/rhythmbox/rhythmdb.xml"],
            expected: "/home/tester/.var/app/org.gnome.Rhythmbox3/data/rhythmbox/rhythmdb.xml",
        },
        Row {
            name: "flatpak finds the host's database",
            inputs: inputs(true, Some(HOME), Some("/host/data"), SANDBOX),
            present: &["/host/data/rhythmbox/rhythmdb.xml"],
            expected: "/host/data/rhythmbox/rhythmdb.xml",
        },
        Row {
            name: "flatpak falls through to the Flathub Rhythmbox",
            inputs: inputs(true, Some(HOME), None, SANDBOX),
            present: &["/home/tester/.var/app/org.gnome.Rhythmbox3/data/rhythmbox/rhythmdb.xml"],
            expected: "/home/tester/.var/app/org.gnome.Rhythmbox3/data/rhythmbox/rhythmdb.xml",
        },
        Row {
            name: "a directory without the database file does not win",
            inputs: inputs(false, Some(HOME), None, NATIVE),
            present: &[
                "/home/tester/.local/share/rhythmbox/playlists.xml",
                "/home/tester/.var/app/org.gnome.Rhythmbox3/data/rhythmbox/rhythmdb.xml",
            ],
            expected: "/home/tester/.var/app/org.gnome.Rhythmbox3/data/rhythmbox/rhythmdb.xml",
        },
        Row {
            name: "none exists: the first candidate is reported",
            inputs: inputs(true, Some(HOME), Some("/host/data"), SANDBOX),
            present: &[],
            expected: "/host/data/rhythmbox/rhythmdb.xml",
        },
        Row {
            name: "no candidate at all: the native directory stays the answer",
            inputs: inputs(true, None, None, SANDBOX),
            present: &[],
            expected:
                "/home/tester/.var/app/io.github.marvinbaudach.Reprise/data/rhythmbox/rhythmdb.xml",
        },
    ];

    for row in rows {
        let found = resolve(&row.inputs, existing(row.present));
        assert_eq!(found.rhythmdb, PathBuf::from(row.expected), "{}", row.name);
    }
}

#[test]
fn the_database_override_wins_and_nothing_is_probed() {
    let probed = RefCell::new(Vec::new());
    let mut with_override = inputs(false, Some(HOME), None, NATIVE);
    with_override.rhythmdb_override = Some(OsString::from("/custom/library.xml"));

    let found = resolve(&with_override, |path| {
        probed.borrow_mut().push(path.to_path_buf());
        true
    });

    assert_eq!(found.rhythmdb, PathBuf::from("/custom/library.xml"));
    assert!(probed.borrow().is_empty(), "probed {:?}", probed.borrow());
}

#[test]
fn the_playlists_file_is_the_siblings_of_the_chosen_database_unless_overridden() {
    let flathub_db = "/home/tester/.var/app/org.gnome.Rhythmbox3/data/rhythmbox/rhythmdb.xml";
    let base = inputs(false, Some(HOME), None, NATIVE);

    let sibling = resolve(&base, existing(&[flathub_db]));
    assert_eq!(
        sibling.playlists,
        PathBuf::from("/home/tester/.var/app/org.gnome.Rhythmbox3/data/rhythmbox/playlists.xml")
    );

    let mut overridden = base.clone();
    overridden.playlists_override = Some(OsString::from("/custom/playlists.xml"));
    let found = resolve(&overridden, existing(&[flathub_db]));
    assert_eq!(found.playlists, PathBuf::from("/custom/playlists.xml"));
    assert_eq!(found.rhythmdb, PathBuf::from(flathub_db));

    let mut database_override = base;
    database_override.rhythmdb_override = Some(OsString::from("/custom/library.xml"));
    let found = resolve(&database_override, |_| false);
    assert_eq!(found.playlists, PathBuf::from("/custom/playlists.xml"));
    assert_eq!(found.rhythmdb, PathBuf::from("/custom/library.xml"));
}

#[test]
fn inputs_come_from_the_variables_and_ignore_empty_or_relative_values() {
    let vars: HashMap<&str, OsString> = HashMap::from([
        ("HOME", OsString::from(HOME)),
        ("HOST_XDG_DATA_HOME", OsString::from("relative/data")),
        ("REPRISE_RHYTHMDB_PATH", OsString::from("")),
        ("REPRISE_RHYTHMBOX_PLAYLISTS_PATH", OsString::from("/p.xml")),
    ]);

    let read = inputs_from(true, PathBuf::from(SANDBOX), |name| vars.get(name).cloned());

    assert_eq!(read.home, Some(PathBuf::from(HOME)));
    assert_eq!(
        read.host_xdg_data_home, None,
        "a relative path is not a path"
    );
    assert_eq!(read.rhythmdb_override, None, "an empty override is unset");
    assert_eq!(read.playlists_override, Some(OsString::from("/p.xml")));
    assert!(read.in_flatpak);
    assert_eq!(read.native_data_dir, PathBuf::from(SANDBOX));
}

#[test]
fn start_7_inside_a_flatpak_the_hosts_rhythmbox_directory_is_probed_first() {
    let flatpak = inputs(true, Some(HOME), Some("/host/data"), SANDBOX);

    let dirs = candidate_dirs(&flatpak);

    assert_eq!(dirs.first(), Some(&PathBuf::from("/host/data/rhythmbox")));
    assert!(
        !dirs.contains(&Path::new(SANDBOX).join("rhythmbox")),
        "the sandbox's own directory never holds Rhythmbox's library"
    );
}

#[test]
fn start_7_a_rhythmbox_from_flathub_is_found_in_its_sandbox_directory() {
    let native = resolve(
        &inputs(false, Some(HOME), None, NATIVE),
        existing(&["/home/tester/.var/app/org.gnome.Rhythmbox3/data/rhythmbox/rhythmdb.xml"]),
    );
    let flatpak = resolve(
        &inputs(true, Some(HOME), None, SANDBOX),
        existing(&["/home/tester/.var/app/org.gnome.Rhythmbox3/data/rhythmbox/rhythmdb.xml"]),
    );

    for found in [native, flatpak] {
        assert_eq!(
            found.rhythmdb,
            Path::new(FLATHUB_DIR).join("rhythmdb.xml"),
            "both packagings reach the Flathub Rhythmbox"
        );
    }
}

#[test]
fn start_7_the_first_directory_with_a_database_wins_and_the_override_still_beats_it() {
    let base = inputs(false, Some(HOME), None, NATIVE);
    let both = [
        "/home/tester/.local/share/rhythmbox/rhythmdb.xml",
        "/home/tester/.var/app/org.gnome.Rhythmbox3/data/rhythmbox/rhythmdb.xml",
    ];
    assert_eq!(
        resolve(&base, existing(&both)).rhythmdb,
        PathBuf::from(both[0])
    );

    let mut overridden = base;
    overridden.rhythmdb_override = Some(OsString::from("/custom/library.xml"));
    assert_eq!(
        resolve(&overridden, existing(&both)).rhythmdb,
        PathBuf::from("/custom/library.xml")
    );
}

#[test]
fn start_7_the_playlists_file_sits_beside_the_database_that_was_found() {
    let found = resolve(
        &inputs(true, Some(HOME), Some("/host/data"), SANDBOX),
        existing(&["/host/data/rhythmbox/rhythmdb.xml"]),
    );

    assert_eq!(
        found.playlists,
        PathBuf::from("/host/data/rhythmbox/playlists.xml")
    );
}
