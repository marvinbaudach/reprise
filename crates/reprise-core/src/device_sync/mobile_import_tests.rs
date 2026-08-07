use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use crate::library::source::{
    LibraryDirectoryEntry, LibraryLinkMode, LibraryPathPresence, LibraryReadHandle, LibrarySource,
    LibraryWalkOrder, LibraryWalkVisitor,
};
use crate::spectrogram::{TrackSourceFingerprint, TrackSpectrogram};
use crate::waveform::TrackRenderData;

use super::*;

#[derive(Default)]
struct MemorySource {
    files: HashMap<PathBuf, Vec<u8>>,
}

impl MemorySource {
    fn with(path: &str, bytes: Vec<u8>) -> Self {
        Self {
            files: HashMap::from([(PathBuf::from(path), bytes)]),
        }
    }
}

impl LibrarySource for MemorySource {
    fn residence_token(&self, _at: &Path) -> Option<i64> {
        None
    }

    fn mount_point(&self, _at: &Path) -> Option<PathBuf> {
        None
    }

    fn display_name(&self, at: &Path) -> Option<String> {
        at.file_name()?.to_str().map(str::to_owned)
    }

    fn container_name(&self, _at: &Path) -> Option<String> {
        None
    }

    fn relative_path(&self, root: &Path, at: &Path) -> Option<PathBuf> {
        at.strip_prefix(root).ok().map(Path::to_path_buf)
    }

    fn open_read(&self, at: &Path) -> std::io::Result<LibraryReadHandle> {
        self.files
            .get(at)
            .cloned()
            .map(|bytes| LibraryReadHandle::new(Cursor::new(bytes)))
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))
    }

    fn probe(&self, at: &Path, _links: LibraryLinkMode) -> LibraryPathPresence {
        if self.files.contains_key(at) {
            panic!("these tests never probe sidecars before playback")
        }
        LibraryPathPresence::Unknown
    }

    fn read_directory(&self, _directory: &Path) -> Option<Vec<LibraryDirectoryEntry>> {
        None
    }

    fn walk(&self, _root: &Path, _order: LibraryWalkOrder, _visitor: &mut dyn LibraryWalkVisitor) {}
}

fn phone_source() -> TrackSourceFingerprint {
    TrackSourceFingerprint {
        mtime_seconds: 100,
        size_bytes: 200,
        device: None,
        inode: None,
    }
}

fn desktop_source(version: i64) -> TrackSourceFingerprint {
    TrackSourceFingerprint {
        mtime_seconds: version,
        size_bytes: version * 10,
        device: Some(version * 100),
        inode: Some(version * 1_000),
    }
}

fn render(cell: u8, peak: u8) -> TrackRenderData {
    TrackRenderData {
        waveform_peaks: vec![peak; 4],
        spectrogram: TrackSpectrogram::from_cells(vec![cell; 48]).unwrap(),
    }
}

fn sidecar(source: TrackSourceFingerprint, data: &TrackRenderData) -> Vec<u8> {
    AnalysisSidecar::new(
        source,
        data.spectrogram.clone(),
        data.waveform_peaks.clone(),
    )
    .encode()
    .unwrap()
}

fn seeded() -> Db {
    let db = Db::open_in_memory().unwrap();
    db.conn()
        .execute(
            "INSERT INTO tracks \
             (id, path, title, added_at, file_mtime, file_size) \
             VALUES (1, '/phone/song.flac', 'Song', 0, ?1, ?2)",
            [phone_source().mtime_seconds, phone_source().size_bytes],
        )
        .unwrap();
    crate::db_mobile_sync::register_sidecar(
        db.conn(),
        "/phone/song.flac",
        Path::new("/phone/song.reprise-analysis"),
    )
    .unwrap();
    db
}

fn assert_render(db: &Db, expected: &TrackRenderData) {
    assert_eq!(
        crate::db_spectrogram::get_waveform_peaks(db, 1).unwrap(),
        Some(expected.waveform_peaks.clone())
    );
    assert_eq!(
        crate::db_spectrogram::get_track_spectrogram(db, 1).unwrap(),
        Some(expected.spectrogram.clone())
    );
}

#[test]
fn same_desktop_fingerprint_is_not_written_again_but_a_new_one_is() {
    let db = seeded();
    let first = render(7, 3);
    let first_source = MemorySource::with(
        "/phone/song.reprise-analysis",
        sidecar(desktop_source(1), &first),
    );
    assert_eq!(
        import_analysis_for_track(&first_source, &db, 1).unwrap(),
        AnalysisImportOutcome::Imported
    );

    let sentinel = render(90, 91);
    crate::db_spectrogram::set_track_render_data(&db, 1, phone_source(), &sentinel).unwrap();
    assert_eq!(
        import_analysis_for_track(&first_source, &db, 1).unwrap(),
        AnalysisImportOutcome::AlreadyImported
    );
    assert_render(&db, &sentinel);

    let replacement = render(11, 13);
    let replacement_source = MemorySource::with(
        "/phone/song.reprise-analysis",
        sidecar(desktop_source(2), &replacement),
    );
    assert_eq!(
        import_analysis_for_track(&replacement_source, &db, 1).unwrap(),
        AnalysisImportOutcome::Imported
    );
    assert_render(&db, &replacement);
}

#[test]
fn missing_sidecar_leaves_existing_render_data_untouched() {
    let db = seeded();
    let existing = render(21, 34);
    crate::db_spectrogram::set_track_render_data(&db, 1, phone_source(), &existing).unwrap();

    assert_eq!(
        import_analysis_for_track(&MemorySource::default(), &db, 1).unwrap(),
        AnalysisImportOutcome::Missing
    );
    assert_render(&db, &existing);
}

#[test]
fn corrupt_sidecar_is_nonfatal_and_leaves_existing_render_data_untouched() {
    let db = seeded();
    let existing = render(55, 89);
    crate::db_spectrogram::set_track_render_data(&db, 1, phone_source(), &existing).unwrap();
    let corrupt = MemorySource::with("/phone/song.reprise-analysis", b"not a sidecar".to_vec());

    assert_eq!(
        import_analysis_for_track(&corrupt, &db, 1).unwrap(),
        AnalysisImportOutcome::Invalid
    );
    assert_render(&db, &existing);
}
