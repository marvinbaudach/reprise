//! Non-blocking playback-to-desktop export recording.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;

use reprise_core::db::Db;

#[derive(Clone, Copy, Debug)]
pub(crate) struct RecordedListen {
    pub(crate) track_id: i64,
    pub(crate) at_unix: i64,
    pub(crate) ms_played: u64,
}

pub(crate) struct ListenExportRecorder {
    listens: Option<Sender<RecordedListen>>,
    worker: Option<JoinHandle<()>>,
}

impl ListenExportRecorder {
    pub(crate) fn spawn(database_path: PathBuf, on_change: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (listens, queued) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("reprise-android-listen-export".to_owned())
            .spawn(move || write_queued_listens(&database_path, queued, on_change.as_ref()));
        match worker {
            Ok(worker) => Self {
                listens: Some(listens),
                worker: Some(worker),
            },
            Err(error) => {
                tracing::warn!(%error, "no Android listen export: writer thread did not start");
                Self {
                    listens: None,
                    worker: None,
                }
            }
        }
    }

    pub(crate) fn record(&self, listen: RecordedListen) {
        let Some(listens) = self.listens.as_ref() else {
            tracing::warn!(
                track_id = listen.track_id,
                "dropped an Android listen export: no writer thread"
            );
            return;
        };
        if let Err(error) = listens.send(listen) {
            tracing::warn!(%error, track_id = listen.track_id, "dropped an Android listen export: the writer thread is gone");
        }
    }
}

impl Drop for ListenExportRecorder {
    fn drop(&mut self) {
        self.listens = None;
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                tracing::warn!("the Android listen-export writer thread panicked");
            }
        }
    }
}

fn write_queued_listens(
    database_path: &Path,
    queued: Receiver<RecordedListen>,
    on_change: &(dyn Fn() + Send + Sync),
) {
    let db = match Db::open_ready(database_path) {
        Ok(db) => db,
        Err(error) => {
            tracing::warn!(%error, "no Android listen export: could not open the library");
            return;
        }
    };
    for listen in queued {
        let device_path = match reprise_core::device_sync::mobile_import::device_path_for_track(
            &db,
            listen.track_id,
        ) {
            Ok(Some(path)) => path,
            Ok(None) => {
                tracing::warn!(
                    track_id = listen.track_id,
                    "dropped an Android listen export: no synchronized device path"
                );
                continue;
            }
            Err(error) => {
                tracing::warn!(%error, track_id = listen.track_id, "dropped an Android listen export: device path lookup failed");
                continue;
            }
        };
        match crate::listen_export_journal::record_listen(
            database_path,
            &device_path,
            listen.at_unix,
            listen.ms_played,
        ) {
            Ok(_) => on_change(),
            Err(error) => {
                tracing::warn!(%error, track_id = listen.track_id, "dropped an Android listen export: journal write failed");
            }
        }
    }
}
