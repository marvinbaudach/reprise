use std::fs::File;
use std::os::fd::IntoRawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use reprise_core::db::Db;
use reprise_core::queries::{self, WindowRange};

use super::{MusicLibrary, ScanProgressListener, ScanProgressUpdate};
use crate::play_recorder::{PlayRecorder, RecordedPlay};
use crate::source::{SafSource, SafSourceError, SourceChild, SourceFacts};
use crate::writer_backoff::try_lock_writer;

const TREE_URI: &str = "content://reprise.test/tree/music";
const FIRST_DIRECTORY_URI: &str = "content://reprise.test/tree/music/first";
const SECOND_DIRECTORY_URI: &str = "content://reprise.test/tree/music/second";
const FIRST_BATCH_TRACKS: usize = 14;
const DEADLINE: Duration = Duration::from_secs(10);

struct QuietProgress;

impl ScanProgressListener for QuietProgress {
    fn on_progress(&self, _progress: ScanProgressUpdate) {}
}

struct FixtureSource {
    entered_second_directory: mpsc::SyncSender<()>,
    release_second_directory: Mutex<mpsc::Receiver<()>>,
    entered: AtomicBool,
}

impl FixtureSource {
    fn track_uri(index: usize) -> String {
        format!("{FIRST_DIRECTORY_URI}/track-{index:02}.flac")
    }

    fn facts(uri: &str, is_file: bool) -> SourceFacts {
        SourceFacts {
            display_name: Some(uri.rsplit('/').next().unwrap().to_owned()),
            is_file,
            is_directory: !is_file,
            size_bytes: is_file.then_some(12_066),
            modified_unix_ms: Some(1_775_000_123_456),
            document_id: uri.to_owned(),
        }
    }

    fn child(uri: &str, is_file: bool) -> SourceChild {
        let facts = Self::facts(uri, is_file);
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
        let facts = if uri == TREE_URI || uri == FIRST_DIRECTORY_URI || uri == SECOND_DIRECTORY_URI
        {
            Some(Self::facts(&uri, false))
        } else if (0..FIRST_BATCH_TRACKS).any(|index| Self::track_uri(index) == uri) {
            Some(Self::facts(&uri, true))
        } else {
            None
        };
        Ok(facts)
    }

    fn list_children(&self, uri: String) -> Result<Vec<SourceChild>, SafSourceError> {
        if uri == TREE_URI {
            return Ok(vec![
                Self::child(FIRST_DIRECTORY_URI, false),
                Self::child(SECOND_DIRECTORY_URI, false),
            ]);
        }
        if uri == FIRST_DIRECTORY_URI {
            return Ok((0..FIRST_BATCH_TRACKS)
                .map(|index| Self::child(&Self::track_uri(index), true))
                .collect());
        }
        if uri == SECOND_DIRECTORY_URI {
            if !self.entered.swap(true, Ordering::SeqCst) {
                self.entered_second_directory.send(()).unwrap();
                self.release_second_directory
                    .lock()
                    .unwrap()
                    .recv_timeout(DEADLINE)
                    .unwrap();
            }
            return Ok(Vec::new());
        }
        Ok(Vec::new())
    }

    fn open_read_fd(&self, uri: String) -> Result<i32, SafSourceError> {
        if !(0..FIRST_BATCH_TRACKS).any(|index| Self::track_uri(index) == uri) {
            return Err(SafSourceError::Io {
                detail: format!("unexpected document: {uri}"),
            });
        }
        File::open(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../android/app/src/main/assets/sine.flac"),
        )
        .map(IntoRawFd::into_raw_fd)
        .map_err(|error| SafSourceError::Io {
            detail: error.to_string(),
        })
    }
}

fn tracks(database_path: &std::path::Path) -> Vec<reprise_core::models::Track> {
    let database = Db::open_ready(database_path).unwrap();
    queries::query_library_text_search(
        &database,
        "",
        WindowRange {
            offset: 0,
            limit: 500,
        },
    )
    .unwrap()
    .rows
}

fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + DEADLINE;
    while !condition() {
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    true
}

#[test]
fn a_play_write_lands_while_the_scan_waits_on_the_second_directory() {
    let directory = tempfile::tempdir().unwrap();
    let library = Arc::new(
        MusicLibrary::open(
            directory.path().to_str().unwrap(),
            directory.path().join("cache").to_str().unwrap(),
        )
        .unwrap(),
    );
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    library
        .set_tree_uri(
            TREE_URI.to_owned(),
            Box::new(FixtureSource {
                entered_second_directory: entered_tx,
                release_second_directory: Mutex::new(release_rx),
                entered: AtomicBool::new(false),
            }),
        )
        .unwrap();
    let scan_library = Arc::clone(&library);
    let scan = std::thread::spawn(move || scan_library.scan(Box::new(QuietProgress)));

    entered_rx.recv_timeout(DEADLINE).unwrap();
    let writer_was_available = match try_lock_writer(&library.writer) {
        Ok(writer) => writer.is_some(),
        Err(_) => panic!("the shared writer was poisoned"),
    };
    let first_batch = tracks(&library.database_path);
    assert_eq!(first_batch.len(), FIRST_BATCH_TRACKS);
    let track_id = first_batch[0].id;
    let recorder = PlayRecorder::spawn(library.database_path.clone(), library.writer_handle(), 0);
    recorder.record(RecordedPlay {
        track_id,
        at_unix: 1_700_000_000,
    });
    let play_landed_before_release = wait_until(|| {
        tracks(&library.database_path)
            .into_iter()
            .find(|track| track.id == track_id)
            .is_some_and(|track| track.play_count == 1)
    });

    release_tx.send(()).unwrap();
    let summary = scan.join().unwrap().unwrap();
    drop(recorder);

    assert!(
        writer_was_available,
        "the old whole-walk guard keeps try_lock_writer busy here"
    );
    assert!(
        play_landed_before_release,
        "the play writer did not commit while the source call was parked"
    );
    assert_eq!(summary.added as usize, FIRST_BATCH_TRACKS);
}
