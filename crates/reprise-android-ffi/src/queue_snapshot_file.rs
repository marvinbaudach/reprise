//! Durable single-slot storage for Android's latest playback queue.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use reprise_core::queue::{Queue, QueueSnapshot};
use serde::{Deserialize, Serialize};

pub(super) const FILE_NAME: &str = "android-queue-snapshot.v1";
const LOCK_FILE_NAME: &str = ".android-queue-snapshot.v1.lock";
const TEMP_FILE_NAME: &str = ".android-queue-snapshot.v1.tmp";
const FORMAT_VERSION: u8 = 1;

#[derive(Debug, Deserialize, Serialize)]
struct SnapshotRecord {
    version: u8,
    sequence: u64,
    queue: QueueSnapshot,
}

pub(super) struct QueueSnapshotFile {
    path: PathBuf,
    lock_path: PathBuf,
}

impl QueueSnapshotFile {
    pub(super) fn new(database_path: &Path) -> io::Result<Self> {
        let directory = database_path.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "database path has no parent")
        })?;
        Ok(Self {
            path: directory.join(FILE_NAME),
            lock_path: directory.join(LOCK_FILE_NAME),
        })
    }

    pub(super) fn write(&self, sequence: u64, queue: &Queue) -> io::Result<()> {
        let _lock = claim(&self.lock_path)?;
        let record = SnapshotRecord {
            version: FORMAT_VERSION,
            sequence,
            queue: queue.snapshot(),
        };
        let bytes = serde_json::to_vec(&record).map_err(io::Error::other)?;
        let temporary = self.path.with_file_name(TEMP_FILE_NAME);
        let mut file = File::create(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_data()?;
        drop(file);
        fs::rename(temporary, &self.path)?;
        sync_directory_of(&self.path)
    }

    pub(super) fn read(&self) -> Option<(u64, Queue)> {
        let _lock = match claim(&self.lock_path) {
            Ok(lock) => lock,
            Err(error) => {
                tracing::warn!(%error, "could not lock the Android queue snapshot");
                return None;
            }
        };
        self.read_locked()
    }

    pub(super) fn remove_if_sequence(&self, sequence: u64) {
        let _lock = match claim(&self.lock_path) {
            Ok(lock) => lock,
            Err(error) => {
                tracing::warn!(%error, "could not lock the Android queue snapshot for removal");
                return;
            }
        };
        let Some((stored_sequence, _)) = self.read_locked() else {
            return;
        };
        if stored_sequence != sequence {
            return;
        }
        if let Err(error) = fs::remove_file(&self.path).and_then(|()| sync_directory_of(&self.path))
        {
            tracing::warn!(%error, sequence, "could not remove a committed Android queue snapshot");
        }
    }

    fn read_locked(&self) -> Option<(u64, Queue)> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
            Err(error) => {
                tracing::warn!(%error, "could not read the Android queue snapshot");
                return None;
            }
        };
        let result = serde_json::from_slice::<SnapshotRecord>(&bytes)
            .map_err(|error| error.to_string())
            .and_then(|record| {
                if record.version != FORMAT_VERSION {
                    return Err(format!("unknown version {}", record.version));
                }
                let mut queue = Queue::new();
                queue
                    .restore_snapshot(record.queue)
                    .map_err(|error| error.to_string())?;
                Ok((record.sequence, queue))
            });
        match result {
            Ok(snapshot) => Some(snapshot),
            Err(detail) => {
                tracing::warn!(%detail, "discarded a damaged Android queue snapshot");
                if let Err(error) = fs::remove_file(&self.path) {
                    tracing::warn!(%error, "could not remove a damaged Android queue snapshot");
                } else if let Err(error) = sync_directory_of(&self.path) {
                    tracing::warn!(%error, "could not sync removal of a damaged Android queue snapshot");
                }
                None
            }
        }
    }
}

fn claim(path: &Path) -> io::Result<Option<File>> {
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => {
            file.lock()?;
            Ok(Some(file))
        }
        Err(TryLockError::Error(error)) if error.kind() == io::ErrorKind::Unsupported => {
            tracing::warn!(%error, "the Android queue snapshot is running unlocked");
            Ok(None)
        }
        Err(TryLockError::Error(error)) => Err(error),
    }
}

fn sync_directory_of(file_path: &Path) -> io::Result<()> {
    let directory = file_path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Android queue-snapshot path has no parent",
        )
    })?;
    File::open(directory)?.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot_file(directory: &Path) -> QueueSnapshotFile {
        QueueSnapshotFile::new(&directory.join("reprise.db")).unwrap()
    }

    fn queue(ids: Vec<i64>, position: usize) -> Queue {
        let mut queue = Queue::new();
        queue.set_tracks(ids, position);
        queue
    }

    #[test]
    fn a_new_snapshot_atomically_replaces_the_previous_one() {
        let directory = tempfile::tempdir().unwrap();
        let file = snapshot_file(directory.path());
        file.write(1, &queue(vec![1, 2], 0)).unwrap();

        file.write(2, &queue(vec![3, 4, 5], 1)).unwrap();

        let (sequence, restored) = file.read().unwrap();
        assert_eq!(sequence, 2);
        assert_eq!(restored.snapshot(), queue(vec![3, 4, 5], 1).snapshot());
        assert!(!directory.path().join(TEMP_FILE_NAME).exists());
    }

    #[test]
    fn damaged_snapshots_are_discarded() {
        let cases = [
            br#"{"version":1,"sequence":7,"queue":{"ids":[1]"#.as_slice(),
            br#"{"version":2,"sequence":7,"queue":{"ids":[],"order":[],"position":null,"repeat":"Off","shuffled":false}}"#,
            b"not json".as_slice(),
        ];
        for contents in cases {
            let directory = tempfile::tempdir().unwrap();
            let file = snapshot_file(directory.path());
            fs::write(directory.path().join(FILE_NAME), contents).unwrap();

            assert!(file.read().is_none());
            assert!(!directory.path().join(FILE_NAME).exists());
        }
    }

    #[test]
    fn removal_only_deletes_the_matching_sequence() {
        let directory = tempfile::tempdir().unwrap();
        let file = snapshot_file(directory.path());
        file.write(8, &queue(vec![8], 0)).unwrap();

        file.remove_if_sequence(7);
        assert_eq!(file.read().unwrap().0, 8);

        file.remove_if_sequence(8);
        assert!(file.read().is_none());
    }
}
