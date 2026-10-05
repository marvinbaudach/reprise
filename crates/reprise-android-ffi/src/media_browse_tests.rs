use std::collections::HashMap;
use std::path::PathBuf;

use reprise_core::db::Db;
use reprise_core::library::{playlists, stats};
use reprise_core::queries;

use crate::{MusicLibrary, WindowRange};

const SINE: &str = "../../android/app/src/main/assets/sine.flac";

/// A scanned library of `names.len()` tracks, one file each, so every read
/// here runs against the same rows the app would have scanned.
struct Fixture {
    directory: tempfile::TempDir,
    library: MusicLibrary,
    ids: HashMap<&'static str, i64>,
}

impl Fixture {
    fn new(names: &[&'static str]) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let music = directory.path().join("music");
        std::fs::create_dir(&music).unwrap();
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(SINE);
        for name in names {
            std::fs::copy(&fixture, music.join(format!("{name}.flac"))).unwrap();
        }
        let database =
            Db::open_migrated(Some(&directory.path().join(crate::DATABASE_FILE_NAME))).unwrap();
        reprise_core::library::scanner::scan_folder(&database, &music).unwrap();
        drop(database);
        let library = MusicLibrary::open(
            directory.path().to_str().unwrap(),
            directory.path().join("cache").to_str().unwrap(),
        )
        .unwrap();
        let rows = library
            .list_tracks(WindowRange {
                offset: 0,
                limit: 100,
            })
            .unwrap()
            .rows;
        let ids = names
            .iter()
            .map(|name| {
                let row = rows
                    .iter()
                    .find(|row| row.uri.ends_with(&format!("{name}.flac")))
                    .unwrap();
                (*name, row.id)
            })
            .collect();
        Self {
            directory,
            library,
            ids,
        }
    }

    fn id(&self, name: &str) -> i64 {
        self.ids[name]
    }

    fn database(&self) -> Db {
        Db::open_migrated(Some(&self.directory.path().join(crate::DATABASE_FILE_NAME))).unwrap()
    }

    fn played(&self, name: &str, at: i64) {
        stats::record_play(&self.database(), self.id(name), at).unwrap();
    }

    fn remove(&self, name: &str) {
        queries::tombstone_tracks(&self.database(), &[self.id(name)], 10).unwrap();
    }

    fn playlist(&self, name: &str, members: &[&str]) -> i64 {
        let ids: Vec<i64> = members.iter().map(|member| self.id(member)).collect();
        playlists::create_with_tracks(&self.database(), name, &ids).unwrap()
    }
}

#[test]
fn playlists_list_in_the_users_order_with_their_stored_counts() {
    let fixture = Fixture::new(&["one", "two"]);
    fixture.playlist("Road", &["one", "two"]);
    fixture.playlist("Gym", &["two"]);

    let rows = fixture.library.list_playlists().unwrap();

    assert_eq!(
        rows.iter()
            .map(|row| (row.name.as_str(), row.track_count))
            .collect::<Vec<_>>(),
        [("Road", 2), ("Gym", 1)]
    );
}

#[test]
fn a_role_playlist_is_not_a_browse_row() {
    let fixture = Fixture::new(&["one"]);
    fixture.playlist("Road", &["one"]);
    playlists::ensure_role_playlist(&fixture.database(), "Hidden", "ai_conversion").unwrap();

    let names: Vec<_> = fixture
        .library
        .list_playlists()
        .unwrap()
        .into_iter()
        .map(|row| row.name)
        .collect();

    assert_eq!(names, ["Road"]);
}

#[test]
fn playlist_reads_keep_playlist_order_and_drop_removed_files() {
    let fixture = Fixture::new(&["one", "two", "three"]);
    let playlist = fixture.playlist("Road", &["three", "one", "two"]);
    fixture.remove("one");

    let ids = fixture.library.playlist_track_ids(playlist).unwrap();
    let rows = fixture.library.playlist_tracks(playlist).unwrap();

    assert_eq!(ids, [fixture.id("three"), fixture.id("two")]);
    assert_eq!(rows.iter().map(|row| row.id).collect::<Vec<_>>(), ids);
    assert!(fixture.library.playlist_track_ids(999).unwrap().is_empty());
    assert!(fixture.library.playlist_tracks(999).unwrap().is_empty());
}

#[test]
fn a_playlist_may_hold_a_track_twice() {
    let fixture = Fixture::new(&["one", "two"]);
    let playlist = fixture.playlist("Loop", &["one", "two", "one"]);

    let ids = fixture.library.playlist_track_ids(playlist).unwrap();

    assert_eq!(
        ids,
        [fixture.id("one"), fixture.id("two"), fixture.id("one")]
    );
}

#[test]
fn recently_played_lists_played_tracks_newest_first_and_honours_the_limit() {
    let fixture = Fixture::new(&["oldest", "never", "newest", "middle"]);
    fixture.played("oldest", 10);
    fixture.played("newest", 30);
    fixture.played("middle", 20);

    assert_eq!(
        fixture.library.recently_played_track_ids(10).unwrap(),
        [
            fixture.id("newest"),
            fixture.id("middle"),
            fixture.id("oldest")
        ]
    );
    assert_eq!(
        fixture
            .library
            .recently_played_tracks(2)
            .unwrap()
            .iter()
            .map(|row| row.id)
            .collect::<Vec<_>>(),
        [fixture.id("newest"), fixture.id("middle")]
    );
    assert!(fixture
        .library
        .recently_played_track_ids(0)
        .unwrap()
        .is_empty());
    assert!(fixture
        .library
        .recently_played_track_ids(-5)
        .unwrap()
        .is_empty());
}

#[test]
fn recently_played_skips_a_removed_track() {
    let fixture = Fixture::new(&["gone", "here"]);
    fixture.played("gone", 10);
    fixture.played("here", 5);
    fixture.remove("gone");

    assert_eq!(
        fixture.library.recently_played_track_ids(10).unwrap(),
        [fixture.id("here")]
    );
}

#[test]
fn recently_played_is_empty_when_nothing_was_played() {
    let fixture = Fixture::new(&["one", "two"]);

    assert!(fixture
        .library
        .recently_played_track_ids(50)
        .unwrap()
        .is_empty());
}

#[test]
fn a_uri_resolves_to_its_present_row_and_a_stale_uri_to_none() {
    let fixture = Fixture::new(&["seven", "eight"]);
    let uri = fixture
        .library
        .track_by_id(fixture.id("seven"))
        .unwrap()
        .unwrap()
        .uri;
    let removed_uri = fixture
        .library
        .track_by_id(fixture.id("eight"))
        .unwrap()
        .unwrap()
        .uri;
    fixture.remove("eight");

    let found = fixture.library.track_by_uri(uri).unwrap();

    assert_eq!(found.map(|row| row.id), Some(fixture.id("seven")));
    assert_eq!(fixture.library.track_by_uri(removed_uri).unwrap(), None);
    assert_eq!(
        fixture
            .library
            .track_by_uri("content://tree/none.flac".into())
            .unwrap(),
        None
    );
}
