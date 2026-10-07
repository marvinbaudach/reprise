//! Non-blocking playback-to-desktop export recording.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
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
    pub(crate) fn spawn(
        database_path: PathBuf,
        reader: Arc<Mutex<Db>>,
        on_change: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let (listens, queued) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("reprise-android-listen-export".to_owned())
            .spawn(move || {
                write_queued_listens(&database_path, &reader, queued, on_change.as_ref());
            });
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
    reader: &Mutex<Db>,
    queued: Receiver<RecordedListen>,
    on_change: &(dyn Fn() + Send + Sync),
) {
    for listen in queued {
        let database = match reader.lock() {
            Ok(database) => database,
            Err(_) => {
                tracing::warn!("no Android listen export: the shared reader was poisoned");
                return;
            }
        };
        let (device_path, segment_start_ms) =
            match reprise_core::device_sync::mobile_import::report_identity_for_track(
                &database,
                listen.track_id,
            ) {
                Ok(Some(identity)) => identity,
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
        drop(database);
        let track = crate::listen_export_journal::ReportedTrack {
            device_path: &device_path,
            segment_start_ms,
        };
        match crate::listen_export_journal::record_listen(
            database_path,
            track,
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

#[cfg(test)]
mod tests {
    use std::sync::{mpsc, Mutex};

    use reprise_core::device_sync::listen_report::ListenReport;

    use super::{write_queued_listens, RecordedListen};

    /// A mono 8-bit WAV of `seconds` of silence.
    fn write_silent_wav(path: &std::path::Path, seconds: u32) {
        const RATE: u32 = 8_000;
        let data_len = RATE * seconds;
        let mut body = Vec::new();
        body.extend_from_slice(b"WAVEfmt ");
        body.extend_from_slice(&16_u32.to_le_bytes());
        body.extend_from_slice(&1_u16.to_le_bytes());
        body.extend_from_slice(&1_u16.to_le_bytes());
        body.extend_from_slice(&RATE.to_le_bytes());
        body.extend_from_slice(&RATE.to_le_bytes());
        body.extend_from_slice(&1_u16.to_le_bytes());
        body.extend_from_slice(&8_u16.to_le_bytes());
        body.extend_from_slice(b"data");
        body.extend_from_slice(&data_len.to_le_bytes());
        body.extend(std::iter::repeat_n(0x80_u8, data_len as usize));
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
        std::fs::write(path, out).unwrap();
    }

    #[test]
    fn cue_17_a_phone_listen_of_the_third_track_of_a_synced_cue_file_is_journalled_with_its_start()
    {
        let sync_tree = tempfile::tempdir().unwrap();
        write_silent_wav(&sync_tree.path().join("album.wav"), 30);
        std::fs::write(
            sync_tree.path().join("album.cue"),
            "FILE \"album.wav\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"One\"\n    INDEX 01 00:00:00\n  \
             TRACK 02 AUDIO\n    TITLE \"Two\"\n    INDEX 01 00:10:00\n  \
             TRACK 03 AUDIO\n    TITLE \"Three\"\n    INDEX 01 00:20:01\n",
        )
        .unwrap();
        let state = tempfile::tempdir().unwrap();
        let database_path = state.path().join("reprise.db");
        let db = reprise_core::db::Db::open_migrated(Some(&database_path)).unwrap();
        reprise_core::library::scanner::scan_folder(&db, sync_tree.path()).unwrap();
        let audio = sync_tree.path().join("album.wav");
        let third =
            reprise_core::queries::track_ids_for_path(&db, &audio.to_string_lossy()).unwrap()[2];
        let (sender, queued) = mpsc::channel();
        sender
            .send(RecordedListen {
                track_id: third,
                at_unix: 1_777_000_000,
                ms_played: 9_000,
            })
            .unwrap();
        drop(sender);

        write_queued_listens(&database_path, &Mutex::new(db), queued, &|| {});

        let report = ListenReport::decode(
            &crate::listen_export_journal::prepare_report(&database_path, None).unwrap(),
        )
        .unwrap();
        assert_eq!(report.listens.len(), 1);
        assert_eq!(report.listens[0].device_path, "album.wav");
        assert_eq!(report.listens[0].segment_start_ms, Some(20_013));
    }
}
