use std::fs::File;
use std::os::fd::IntoRawFd;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use super::{LibraryError, MusicLibrary, ScanProgressListener, ScanProgressUpdate, TrackWindow};
use crate::source::{SafSource, SafSourceError, SourceChild, SourceFacts};
use crate::WindowRange;

const TREE_URI: &str = "content://reprise.test/tree/music";
const FIRST_TRACK_URI: &str = "content://reprise.test/tree/music/first.flac";
const SECOND_TRACK_URI: &str = "content://reprise.test/tree/music/second.flac";

// On the broken implementation the reader waits for the scan's mutex forever,
// so a deadline is the only way to distinguish "never" from "in a moment".
// A working reader answers in microseconds; two minutes avoids load flakes.
const READER_DEADLINE: Duration = Duration::from_secs(120);

struct QuietProgress;

impl ScanProgressListener for QuietProgress {
    fn on_progress(&self, _progress: ScanProgressUpdate) {}
}

struct FixtureSource {
    track_count: usize,
    rendezvous: Option<ScanRendezvous>,
}

struct ScanRendezvous {
    entered: AtomicBool,
    scan_is_inside: mpsc::SyncSender<()>,
    reader_answer: Mutex<mpsc::Receiver<Result<TrackWindow, LibraryError>>>,
    observed_answer: Arc<Mutex<Option<Result<TrackWindow, LibraryError>>>>,
}

impl FixtureSource {
    fn plain(track_count: usize) -> Self {
        Self {
            track_count,
            rendezvous: None,
        }
    }

    fn blocking(
        track_count: usize,
        scan_is_inside: mpsc::SyncSender<()>,
        reader_answer: mpsc::Receiver<Result<TrackWindow, LibraryError>>,
        observed_answer: Arc<Mutex<Option<Result<TrackWindow, LibraryError>>>>,
    ) -> Self {
        Self {
            track_count,
            rendezvous: Some(ScanRendezvous {
                entered: AtomicBool::new(false),
                scan_is_inside,
                reader_answer: Mutex::new(reader_answer),
                observed_answer,
            }),
        }
    }

    fn track_uris(&self) -> impl Iterator<Item = &'static str> {
        [FIRST_TRACK_URI, SECOND_TRACK_URI]
            .into_iter()
            .take(self.track_count)
    }
}

impl SafSource for FixtureSource {
    fn residence_token(&self, _uri: String) -> Result<Option<i64>, SafSourceError> {
        Ok(Some(41))
    }

    fn probe(
        &self,
        uri: String,
        _follow_links: bool,
    ) -> Result<Option<SourceFacts>, SafSourceError> {
        let facts = if uri == TREE_URI {
            Some(SourceFacts {
                display_name: Some("Music".to_owned()),
                is_file: false,
                is_directory: true,
                size_bytes: None,
                modified_unix_ms: Some(1_775_000_000_000),
                document_id: "music".to_owned(),
            })
        } else if self.track_uris().any(|track_uri| track_uri == uri) {
            Some(track_facts(&uri))
        } else {
            None
        };
        Ok(facts)
    }

    fn list_children(&self, uri: String) -> Result<Vec<SourceChild>, SafSourceError> {
        if uri != TREE_URI {
            return Ok(Vec::new());
        }
        if let Some(rendezvous) = &self.rendezvous {
            if !rendezvous.entered.swap(true, Ordering::SeqCst) {
                rendezvous.scan_is_inside.send(()).unwrap();
                let answer = rendezvous
                    .reader_answer
                    .lock()
                    .unwrap()
                    .recv_timeout(READER_DEADLINE)
                    .ok();
                *rendezvous.observed_answer.lock().unwrap() = answer;
            }
        }
        Ok(self.track_uris().map(track_child).collect())
    }

    fn open_read_fd(&self, uri: String) -> Result<i32, SafSourceError> {
        if !self.track_uris().any(|track_uri| track_uri == uri) {
            return Err(SafSourceError::Io {
                detail: format!("unexpected document: {uri}"),
            });
        }
        File::open(audio_fixture())
            .map(IntoRawFd::into_raw_fd)
            .map_err(|error| SafSourceError::Io {
                detail: error.to_string(),
            })
    }
}

fn track_facts(uri: &str) -> SourceFacts {
    SourceFacts {
        display_name: Some(track_name(uri).to_owned()),
        is_file: true,
        is_directory: false,
        size_bytes: Some(12_066),
        modified_unix_ms: Some(1_775_000_123_456),
        document_id: track_name(uri).to_owned(),
    }
}

fn track_child(uri: &str) -> SourceChild {
    let facts = track_facts(uri);
    SourceChild {
        uri: uri.to_owned(),
        display_name: facts.display_name,
        is_file: facts.is_file,
        is_directory: facts.is_directory,
        size_bytes: facts.size_bytes,
        modified_unix_ms: facts.modified_unix_ms,
        document_id: facts.document_id,
    }
}

fn track_name(uri: &str) -> &str {
    uri.rsplit('/').next().unwrap()
}

fn audio_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../android/app/src/main/assets/sine.flac")
}

fn full_window() -> WindowRange {
    WindowRange {
        offset: 0,
        limit: 500,
    }
}

fn read_while_scanning() -> (Option<Result<TrackWindow, LibraryError>>, TrackWindow) {
    let directory = tempfile::tempdir().unwrap();
    let library = Arc::new(
        MusicLibrary::open(
            directory.path().to_str().unwrap(),
            directory.path().join("cache").to_str().unwrap(),
        )
        .unwrap(),
    );
    library
        .set_tree_uri(TREE_URI.to_owned(), Box::new(FixtureSource::plain(1)))
        .unwrap();
    library.scan(Box::new(QuietProgress)).unwrap();

    let (inside_tx, inside_rx) = mpsc::sync_channel(1);
    let (answer_tx, answer_rx) = mpsc::sync_channel(1);
    let observed_answer = Arc::new(Mutex::new(None));
    library
        .set_tree_uri(
            TREE_URI.to_owned(),
            Box::new(FixtureSource::blocking(
                2,
                inside_tx,
                answer_rx,
                Arc::clone(&observed_answer),
            )),
        )
        .unwrap();

    let reader_library = Arc::clone(&library);
    let reader = thread::spawn(move || {
        inside_rx.recv().unwrap();
        let _ = answer_tx.send(reader_library.list_tracks(full_window()));
    });
    library.scan(Box::new(QuietProgress)).unwrap();
    reader.join().unwrap();

    let during_scan = observed_answer.lock().unwrap().take();
    let after_scan = library.list_tracks(full_window()).unwrap();
    (during_scan, after_scan)
}

#[test]
fn a_browse_read_completes_while_a_scan_holds_the_writer() {
    let (during_scan, _) = read_while_scanning();

    assert!(
        matches!(during_scan, Some(Ok(_))),
        "a library read did not complete while a scan held the writer"
    );
}

#[test]
fn a_read_during_a_scan_sees_the_library_as_it_was_before_the_scan_committed() {
    let (during_scan, after_scan) = read_while_scanning();
    let during_scan = during_scan
        .expect("a library read did not complete while a scan held the writer")
        .expect("a library read failed while a scan held the writer");

    assert_eq!(during_scan.total, 1);
    assert_eq!(after_scan.total, 2);
}
