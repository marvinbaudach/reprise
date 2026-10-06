//! A sheet the scan cannot see, because listing its directory, probing it or
//! reading it failed, is not a sheet that went away. The tracks it cut stay as
//! they are, with their ids and ratings, until a scan can see the sheet again.

use std::path::{Path, PathBuf};

use super::super::{scan_folder_with_source, tests::completed};
use super::{bump_mtime, ids_of, issue, segments_of, write_wav, Album};
use crate::library::source::{self, LibraryLinkMode, LibraryPathPresence, UnixLibrarySource};

#[derive(Clone, Copy, Debug)]
enum Fault {
    /// The directory of the audio cannot be listed.
    Listing,
    /// The sheet is listed, but whether it is there cannot be established.
    SheetProbe,
    /// The sheet is there, but opening it fails.
    SheetRead,
}

/// The Unix source with one kind of failure injected for `.cue` files.
struct FlakySource(Fault);

fn is_sheet(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "cue")
}

impl source::LibrarySource for FlakySource {
    fn residence_token(&self, at: &Path) -> Option<i64> {
        UnixLibrarySource.residence_token(at)
    }
    fn mount_point(&self, at: &Path) -> Option<PathBuf> {
        UnixLibrarySource.mount_point(at)
    }
    fn display_name(&self, at: &Path) -> Option<String> {
        UnixLibrarySource.display_name(at)
    }
    fn container_name(&self, at: &Path) -> Option<String> {
        UnixLibrarySource.container_name(at)
    }
    fn relative_path(&self, root: &Path, at: &Path) -> Option<PathBuf> {
        UnixLibrarySource.relative_path(root, at)
    }
    fn open_read(&self, at: &Path) -> std::io::Result<source::LibraryReadHandle> {
        if matches!(self.0, Fault::SheetRead) && is_sheet(at) {
            return Err(std::io::Error::other("injected read failure"));
        }
        UnixLibrarySource.open_read(at)
    }
    fn probe(&self, at: &Path, links: LibraryLinkMode) -> LibraryPathPresence {
        if matches!(self.0, Fault::SheetProbe) && is_sheet(at) {
            return LibraryPathPresence::Unknown;
        }
        UnixLibrarySource.probe(at, links)
    }
    fn read_directory(&self, directory: &Path) -> Option<Vec<source::LibraryDirectoryEntry>> {
        if matches!(self.0, Fault::Listing) {
            return None;
        }
        UnixLibrarySource.read_directory(directory)
    }
    fn walk(
        &self,
        root: &Path,
        order: source::LibraryWalkOrder,
        visitor: &mut dyn source::LibraryWalkVisitor,
    ) {
        UnixLibrarySource.walk(root, order, visitor);
    }
}

/// An album scanned once, with a rating on its second track.
fn rated_album() -> (Album, Vec<i64>) {
    let album = Album::new();
    album.scan();
    album
        .db
        .conn()
        .execute("UPDATE tracks SET rating = 4 WHERE segment_index = 2", [])
        .unwrap();
    let ids = ids_of(album.db.conn(), &album.audio());
    (album, ids)
}

fn scan_with(album: &Album, fault: Fault) {
    completed(scan_folder_with_source(&FlakySource(fault), &album.db, album.dir.path()).unwrap());
}

fn assert_untouched(album: &Album, ids: &[i64], why: &str) {
    assert_eq!(ids_of(album.db.conn(), &album.audio()), ids, "{why}: ids");
    assert_eq!(
        segments_of(album.db.conn(), &album.audio()).len(),
        3,
        "{why}"
    );
    let rating: i64 = album
        .db
        .conn()
        .query_row(
            "SELECT rating FROM tracks WHERE segment_index = 2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(rating, 4, "{why}: rating");
    assert_eq!(
        issue(album.db.conn(), &album.sheet()),
        None,
        "{why}: a sheet that could not be seen is not a broken sheet"
    );
}

#[test]
fn cue_1b_a_sheet_the_scan_cannot_see_leaves_its_tracks_alone() {
    for fault in [Fault::Listing, Fault::SheetProbe, Fault::SheetRead] {
        let (album, ids) = rated_album();
        // A changed sheet is read again, which is where a read can fail.
        bump_mtime(&album.sheet());

        scan_with(&album, fault);

        assert_untouched(&album, &ids, &format!("{fault:?}"));
        album.scan();
        assert_eq!(
            ids_of(album.db.conn(), &album.audio()),
            ids,
            "{fault:?}: a later scan that sees the sheet keeps the rows"
        );
    }
}

#[test]
fn cue_1b_a_sheet_that_cannot_be_read_when_its_audio_changed_leaves_its_tracks_alone() {
    let (album, ids) = rated_album();
    write_wav(&album.audio(), 30);
    bump_mtime(&album.audio());

    scan_with(&album, Fault::SheetRead);

    assert_untouched(&album, &ids, "audio changed, sheet unreadable");
}

#[test]
fn a_plain_file_in_a_directory_that_cannot_be_listed_is_still_read_again() {
    let dir = tempfile::tempdir().unwrap();
    let audio = dir.path().join("plain.wav");
    write_wav(&audio, 10);
    let db = crate::db::Db::open_in_memory().unwrap();
    completed(scan_folder_with_source(&UnixLibrarySource, &db, dir.path()).unwrap());

    write_wav(&audio, 20);
    bump_mtime(&audio);
    let report =
        completed(scan_folder_with_source(&FlakySource(Fault::Listing), &db, dir.path()).unwrap());

    assert_eq!(report.updated, 1);
    assert_eq!(
        segments_of(db.conn(), &audio),
        [(0, None, None, "plain".to_string())]
    );
    let duration: i64 = db
        .conn()
        .query_row("SELECT duration_ms FROM tracks", [], |row| row.get(0))
        .unwrap();
    assert_eq!(duration, 20_000);
}
