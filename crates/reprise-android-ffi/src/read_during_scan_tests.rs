use std::fs::File;
use std::os::fd::IntoRawFd;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use super::{
    AndroidArtworkSize, AndroidPlaybackListener, AndroidPlaybackSession, AndroidPlaybackSnapshot,
    AndroidStoredTheme, AndroidThemeChoice, LibraryError, MusicLibrary, ScanProgressListener,
    ScanProgressUpdate, TrackWindow,
};
use crate::playback::{
    AndroidPlaybackError, AndroidPlaybackPort, AndroidPlaybackState, AndroidTransitionMode,
    PlaybackEventBridge,
};
use crate::source::{SafSource, SafSourceError, SourceChild, SourceFacts};
use crate::WindowRange;
use crate::{AndroidEqualizerPoint, AndroidEqualizerSnapshot};

const TREE_URI: &str = "content://reprise.test/tree/music";
const FIRST_TRACK_URI: &str = "content://reprise.test/tree/music/first.flac";
const SECOND_TRACK_URI: &str = "content://reprise.test/tree/music/second.flac";

// On the broken implementation the reader waits for the scan's mutex forever,
// so a deadline is the only way to distinguish "never" from "in a moment".
// A working reader answers in microseconds; two minutes avoids load flakes.
const READER_DEADLINE: Duration = Duration::from_secs(120);

// This bounds only the scan-to-reader rendezvous signal. It arrives in
// microseconds on a working fixture; ten seconds leaves ample room for a loaded
// parallel test runner while failing far sooner than the reader deadline.
const SCAN_RENDEZVOUS_DEADLINE: Duration = Duration::from_secs(10);

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
    reader_answer: Mutex<mpsc::Receiver<ReaderAnswer>>,
    observed_answer: Arc<Mutex<Option<ReaderAnswer>>>,
}

enum ReaderAnswer {
    Browse(Result<TrackWindow, LibraryError>),
    Artwork(Result<Option<String>, LibraryError>),
    Playback(Result<AndroidPlaybackSession, AndroidPlaybackError>),
}

enum ReaderRendezvousOutcome {
    ReadAttempted,
    ScanNeverReachedRendezvous,
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
        reader_answer: mpsc::Receiver<ReaderAnswer>,
        observed_answer: Arc<Mutex<Option<ReaderAnswer>>>,
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
        if inside_rx.recv_timeout(SCAN_RENDEZVOUS_DEADLINE).is_err() {
            return ReaderRendezvousOutcome::ScanNeverReachedRendezvous;
        }
        let _ = answer_tx.send(ReaderAnswer::Browse(
            reader_library.list_tracks(full_window()),
        ));
        ReaderRendezvousOutcome::ReadAttempted
    });
    library.scan(Box::new(QuietProgress)).unwrap();
    let reader_outcome = reader.join().unwrap();
    assert!(
        matches!(reader_outcome, ReaderRendezvousOutcome::ReadAttempted),
        "scan never reached the read-during-scan rendezvous; the fixture scanner did not call list_children for the configured tree"
    );

    let during_scan = match observed_answer.lock().unwrap().take() {
        Some(ReaderAnswer::Browse(answer)) => Some(answer),
        Some(ReaderAnswer::Artwork(_)) => panic!("browse rendezvous received artwork"),
        Some(ReaderAnswer::Playback(_)) => panic!("browse rendezvous received playback"),
        None => None,
    };
    let after_scan = library.list_tracks(full_window()).unwrap();
    (during_scan, after_scan)
}

fn artwork_while_scanning() -> Option<Result<Option<String>, LibraryError>> {
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
        if inside_rx.recv_timeout(SCAN_RENDEZVOUS_DEADLINE).is_err() {
            return ReaderRendezvousOutcome::ScanNeverReachedRendezvous;
        }
        let _ = answer_tx.send(ReaderAnswer::Artwork(
            reader_library.track_artwork(FIRST_TRACK_URI, AndroidArtworkSize::List),
        ));
        ReaderRendezvousOutcome::ReadAttempted
    });
    library.scan(Box::new(QuietProgress)).unwrap();
    let reader_outcome = reader.join().unwrap();
    assert!(
        matches!(reader_outcome, ReaderRendezvousOutcome::ReadAttempted),
        "scan never reached the read-during-scan rendezvous; the fixture scanner did not call list_children for the configured tree"
    );

    let answer = match observed_answer.lock().unwrap().take() {
        Some(ReaderAnswer::Artwork(answer)) => Some(answer),
        Some(ReaderAnswer::Browse(_)) => panic!("artwork rendezvous received browse answer"),
        Some(ReaderAnswer::Playback(_)) => panic!("artwork rendezvous received playback"),
        None => None,
    };
    answer
}

struct QuietPlaybackPort;

impl AndroidPlaybackPort for QuietPlaybackPort {
    fn set_event_bridge(
        &self,
        _bridge: Arc<PlaybackEventBridge>,
    ) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn play_path(&self, _path: String) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn play_uri(&self, _uri: String) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn toggle_pause(&self) -> Result<AndroidPlaybackState, AndroidPlaybackError> {
        Ok(AndroidPlaybackState::Paused)
    }

    fn seek_to(&self, _position_ms: i64) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn set_volume(&self, _volume: f64) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn set_equalizer(
        &self,
        _enabled: bool,
        _curve: Vec<AndroidEqualizerPoint>,
    ) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn equalizer_snapshot(&self) -> Result<Option<AndroidEqualizerSnapshot>, AndroidPlaybackError> {
        Ok(None)
    }

    fn set_audio_effects(&self) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn set_spectrum_enabled(&self, _enabled: bool) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn stop(&self) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn set_next(&self, _uri: Option<String>) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn set_transition(&self, _mode: AndroidTransitionMode) -> Result<(), AndroidPlaybackError> {
        Ok(())
    }

    fn current_generation(&self) -> Result<u64, AndroidPlaybackError> {
        Ok(0)
    }
}

struct QuietPlaybackListener;

impl AndroidPlaybackListener for QuietPlaybackListener {
    fn on_playback_changed(&self, _snapshot: AndroidPlaybackSnapshot) {}

    fn on_listen_report_changed(&self) {}
}

fn playback_session_while_scanning() -> Option<Result<(), AndroidPlaybackError>> {
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
        if inside_rx.recv_timeout(SCAN_RENDEZVOUS_DEADLINE).is_err() {
            return ReaderRendezvousOutcome::ScanNeverReachedRendezvous;
        }
        let opened = AndroidPlaybackSession::new(
            reader_library,
            Box::new(QuietPlaybackPort),
            Box::new(QuietPlaybackListener),
        );
        let _ = answer_tx.send(ReaderAnswer::Playback(opened));
        ReaderRendezvousOutcome::ReadAttempted
    });
    library.scan(Box::new(QuietProgress)).unwrap();
    assert!(matches!(
        reader.join().unwrap(),
        ReaderRendezvousOutcome::ReadAttempted
    ));

    let answer = match observed_answer.lock().unwrap().take() {
        Some(ReaderAnswer::Playback(answer)) => Some(answer.map(drop)),
        Some(ReaderAnswer::Browse(_)) => panic!("playback rendezvous received browse answer"),
        Some(ReaderAnswer::Artwork(_)) => panic!("playback rendezvous received artwork"),
        None => None,
    };
    answer
}

#[test]
fn playback_session_reads_complete_while_a_scan_holds_the_writer() {
    let during_scan = playback_session_while_scanning();

    assert!(
        matches!(during_scan, Some(Ok(()))),
        "opening playback did not complete while a scan held the writer"
    );
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

#[test]
fn track_artwork_answers_while_a_scan_holds_the_writer() {
    let during_scan = artwork_while_scanning();

    assert!(
        matches!(during_scan, Some(Ok(_))),
        "track artwork did not answer while a scan held the writer"
    );
}

#[test]
fn a_write_on_the_writer_handle_is_visible_to_the_next_read_on_the_reader_handle() {
    let directory = tempfile::tempdir().unwrap();
    let library = MusicLibrary::open(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
    )
    .unwrap();

    // The theme is only a public write/read vehicle; this guards the two
    // database handles sharing committed state, not theme semantics.
    library.set_theme(AndroidThemeChoice::Dynamic).unwrap();

    assert_eq!(
        library.appearance_settings().unwrap().theme,
        AndroidStoredTheme::Dynamic,
        "a committed writer change was not visible to the next reader call"
    );
}
