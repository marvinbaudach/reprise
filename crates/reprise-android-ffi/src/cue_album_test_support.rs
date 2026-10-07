//! A library holding one CUE album: a thirty-second file cut into three
//! tracks by a sheet beside it. The audio is silence the tests write
//! themselves; what matters is how the tracks are cut.

use std::path::Path;
use std::sync::Arc;

use reprise_core::db::Db;
use reprise_core::library::scanner::scan_folder;
use reprise_core::queries;

use crate::MusicLibrary;

const SAMPLE_RATE: u32 = 8_000;
pub(crate) const ALBUM_SECONDS: u32 = 30;
pub(crate) const ALBUM_FILE: &str = "album.wav";

const THREE_TRACKS: &str = "PERFORMER \"Joy Division\"
TITLE \"Unknown Pleasures\"
FILE \"album.wav\" WAVE
  TRACK 01 AUDIO
    TITLE \"Disorder\"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE \"Day of the Lords\"
    INDEX 01 00:10:00
  TRACK 03 AUDIO
    TITLE \"Candidate\"
    INDEX 01 00:20:00
";

/// A mono 8-bit WAV of `seconds` of silence.
fn write_wav(path: &Path, seconds: u32) {
    let data_len = SAMPLE_RATE * seconds;
    let mut body = Vec::new();
    body.extend_from_slice(b"WAVEfmt ");
    body.extend_from_slice(&16_u32.to_le_bytes());
    body.extend_from_slice(&1_u16.to_le_bytes());
    body.extend_from_slice(&1_u16.to_le_bytes());
    body.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    body.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
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

/// The scanned album: the library, the album file's path and its three
/// track ids in play order.
pub(crate) struct CueAlbum {
    pub(crate) library: Arc<MusicLibrary>,
    pub(crate) path: String,
    pub(crate) track_ids: Vec<i64>,
    pub(crate) _directory: tempfile::TempDir,
}

pub(crate) fn cue_album() -> CueAlbum {
    let directory = tempfile::tempdir().unwrap();
    let music = directory.path().join("music");
    std::fs::create_dir(&music).unwrap();
    let audio = music.join(ALBUM_FILE);
    write_wav(&audio, ALBUM_SECONDS);
    std::fs::write(music.join("album.cue"), THREE_TRACKS).unwrap();
    let database =
        Db::open_migrated(Some(&directory.path().join(crate::DATABASE_FILE_NAME))).unwrap();
    scan_folder(&database, &music).unwrap();
    let path = audio.to_string_lossy().into_owned();
    let track_ids = queries::track_ids_for_path(&database, &path).unwrap();
    assert_eq!(
        track_ids.len(),
        3,
        "the sheet cuts the file into three tracks"
    );
    drop(database);
    let library = Arc::new(
        MusicLibrary::open(
            directory.path().to_str().unwrap(),
            directory.path().join("cache").to_str().unwrap(),
        )
        .unwrap(),
    );
    CueAlbum {
        library,
        path,
        track_ids,
        _directory: directory,
    }
}
