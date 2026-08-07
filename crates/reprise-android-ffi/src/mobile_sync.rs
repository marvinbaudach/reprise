use crate::{LibraryError, MusicLibrary};

#[uniffi::export]
impl MusicLibrary {
    /// Lazily imports desktop rendering data for the track being presented.
    /// Missing and malformed sidecars are ordinary no-data outcomes.
    pub fn import_track_analysis(&self, track_id: i64) -> Result<(), LibraryError> {
        let state = self.lock()?;
        let tree = state.tree.as_ref().ok_or(LibraryError::TreeNotConfigured)?;
        reprise_core::device_sync::mobile_import::import_analysis_for_track(
            tree.source.as_ref(),
            &state.db,
            track_id,
        )
        .map(|_| ())
        .map_err(|error| LibraryError::Database {
            detail: error.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::os::fd::IntoRawFd;
    use std::path::PathBuf;

    use reprise_core::device_sync::analysis_sidecar::AnalysisSidecar;
    use reprise_core::spectrogram::{TrackSourceFingerprint, TrackSpectrogram};

    use crate::source::{SafSource, SafSourceError, SourceChild, SourceFacts};
    use crate::{MusicLibrary, ScanProgressListener, ScanProgressUpdate, WindowRange};

    const ROOT: &str = "content://provider/tree/music";
    const TRACK: &str = "content://provider/document/audio-71.flac";
    const SIDECAR: &str = "content://provider/document/analysis-91.reprise-analysis";

    struct QuietProgress;

    impl ScanProgressListener for QuietProgress {
        fn on_progress(&self, _progress: ScanProgressUpdate) {}
    }

    struct SyncedTrackSource {
        audio: PathBuf,
        sidecar: PathBuf,
    }

    impl SafSource for SyncedTrackSource {
        fn residence_token(&self, _uri: String) -> Result<Option<i64>, SafSourceError> {
            Ok(Some(71))
        }

        fn probe(
            &self,
            uri: String,
            _follow_links: bool,
        ) -> Result<Option<SourceFacts>, SafSourceError> {
            assert_eq!(uri, ROOT);
            Ok(Some(SourceFacts {
                display_name: Some("Music".into()),
                is_file: false,
                is_directory: true,
                size_bytes: None,
                modified_unix_ms: None,
                document_id: "opaque-root".into(),
            }))
        }

        fn list_children(&self, uri: String) -> Result<Vec<SourceChild>, SafSourceError> {
            assert_eq!(uri, ROOT);
            Ok(vec![
                SourceChild {
                    uri: TRACK.into(),
                    display_name: Some("Artist - Song.flac".into()),
                    is_file: true,
                    is_directory: false,
                    size_bytes: Some(std::fs::metadata(&self.audio).unwrap().len()),
                    modified_unix_ms: Some(1_775_000_000_000),
                    document_id: "opaque-track".into(),
                },
                SourceChild {
                    uri: SIDECAR.into(),
                    display_name: Some("Artist - Song.reprise-analysis".into()),
                    is_file: true,
                    is_directory: false,
                    size_bytes: Some(std::fs::metadata(&self.sidecar).unwrap().len()),
                    modified_unix_ms: Some(1_775_000_000_000),
                    document_id: "opaque-sidecar".into(),
                },
            ])
        }

        fn open_read_fd(&self, uri: String) -> Result<i32, SafSourceError> {
            let path = match uri.as_str() {
                TRACK => &self.audio,
                SIDECAR => &self.sidecar,
                _ => {
                    return Err(SafSourceError::Io {
                        detail: format!("unexpected document {uri}"),
                    });
                }
            };
            File::open(path)
                .map(IntoRawFd::into_raw_fd)
                .map_err(|error| SafSourceError::Io {
                    detail: error.to_string(),
                })
        }
    }

    #[test]
    fn music_library_trigger_imports_a_discovered_saf_sidecar() {
        let directory = tempfile::tempdir().unwrap();
        let audio = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../android/app/src/main/assets/sine.flac");
        let sidecar_path = directory.path().join("sidecar.bin");
        let sidecar = AnalysisSidecar::new(
            TrackSourceFingerprint {
                mtime_seconds: 1,
                size_bytes: 2,
                device: Some(3),
                inode: Some(4),
            },
            TrackSpectrogram::from_cells(vec![17; 48]).unwrap(),
            vec![19, 23],
        )
        .encode()
        .unwrap();
        std::fs::write(&sidecar_path, sidecar).unwrap();
        let library = MusicLibrary::open(
            directory.path().to_str().unwrap(),
            directory.path().join("cache").to_str().unwrap(),
        )
        .unwrap();
        library
            .set_tree_uri(
                ROOT.into(),
                Box::new(SyncedTrackSource {
                    audio,
                    sidecar: sidecar_path,
                }),
            )
            .unwrap();
        library.scan(Box::new(QuietProgress)).unwrap();
        let track = library
            .list_tracks(WindowRange {
                offset: 0,
                limit: 1,
            })
            .unwrap()
            .rows
            .remove(0);

        library.import_track_analysis(track.id).unwrap();

        let state = library.lock().unwrap();
        assert_eq!(
            reprise_core::db::get_waveform_peaks(&state.db, track.id).unwrap(),
            Some(vec![19, 23])
        );
        assert_eq!(
            reprise_core::db::get_track_spectrogram(&state.db, track.id)
                .unwrap()
                .unwrap()
                .cells(),
            &[17; 48]
        );
    }
}
