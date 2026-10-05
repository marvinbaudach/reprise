use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::{CueError, CueFile, CueSheet, Frames};

// Keep this in sync with library::scanner::AUDIO_EXTENSIONS. That constant is
// private, and the scanner belongs to the parallel r128 strand in wave 1.
const AUDIO_EXTENSIONS: [&str; 7] = ["mp3", "flac", "ogg", "opus", "m4a", "aac", "wav"];

/// One audio track of a CUE sheet, cut out of the file it lives in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CueSegment {
    pub path: PathBuf,
    pub segment_index: i64,
    pub start_ms: i64,
    pub end_ms: i64,
    pub title: String,
    pub performer: String,
    pub track_no: u32,
    pub album: String,
    pub album_artist: String,
    pub date: Option<String>,
    pub genre: Option<String>,
}

/// Extensions a `FILE` line may name besides the ones the scanner reads.
const REFERENCED_ONLY_EXTENSIONS: [&str; 9] = [
    "ape", "wv", "tta", "wma", "aiff", "aif", "alac", "dsf", "mpc",
];

/// Finds the candidate that a `FILE` name refers to.
///
/// Backslashes in `name` count as separators. An exact match wins over a
/// case-insensitive one, and a file with the same stem but another audio
/// extension is the last resort, preferring flac, then wav, then the rest.
/// Ties are broken by sorted path. A name without a known audio extension,
/// such as `Vol.1`, keeps its whole name as the stem. An absolute `name`,
/// Unix, UNC or with a Windows drive, resolves exactly when it is itself in
/// `existing`; otherwise only its file name is looked up in `sheet_dir`.
pub fn resolve_file(sheet_dir: &Path, name: &str, existing: &[PathBuf]) -> Option<PathBuf> {
    let name = name.replace('\\', "/");
    let referenced = sheet_dir.join(&name);
    if let Some(exact) = existing.iter().find(|path| *path == &referenced) {
        return Some(exact.clone());
    }
    let referenced = if is_absolute(&name) {
        let base = name.rsplit('/').next().filter(|base| !base.is_empty())?;
        sheet_dir.join(base)
    } else {
        referenced
    };

    let candidates: Vec<Candidate> = existing.iter().map(Candidate::new).collect();
    if let Some(exact) = candidates.iter().find(|c| *c.path == referenced) {
        return Some(exact.path.clone());
    }
    let folded = fold(&referenced);
    if let Some(found) = candidates
        .iter()
        .filter(|c| c.folded == folded)
        .map(|c| c.path)
        .min()
    {
        return Some(found.clone());
    }
    let stem = stem_key(&referenced);
    candidates
        .iter()
        .filter_map(|c| Some((c.rank?, c.path)).filter(|_| c.stem == stem))
        .min()
        .map(|(_, path)| path.clone())
}

/// An existing file with the keys the lookup passes compare, folded once.
struct Candidate<'a> {
    path: &'a PathBuf,
    folded: String,
    stem: String,
    rank: Option<u8>,
}

impl<'a> Candidate<'a> {
    fn new(path: &'a PathBuf) -> Self {
        Self {
            path,
            folded: fold(path),
            stem: stem_key(path),
            rank: extension_rank(path),
        }
    }
}

fn is_absolute(name: &str) -> bool {
    let mut chars = name.chars();
    let drive = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.next() == Some(':')
        && chars.next() == Some('/');
    drive || Path::new(name).is_absolute()
}

fn fold(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}

/// The folded path without its extension, when that extension is an audio one.
fn stem_key(path: &Path) -> String {
    if lowercase_extension(path).is_some_and(|extension| is_audio_extension(&extension)) {
        fold(&path.with_extension(""))
    } else {
        fold(path)
    }
}

fn lowercase_extension(path: &Path) -> Option<String> {
    Some(path.extension()?.to_string_lossy().to_lowercase())
}

fn is_audio_extension(extension: &str) -> bool {
    AUDIO_EXTENSIONS.contains(&extension) || REFERENCED_ONLY_EXTENSIONS.contains(&extension)
}

/// 0 for flac, 1 for wav, 2 for any other audio extension, `None` otherwise.
fn extension_rank(path: &Path) -> Option<u8> {
    let extension = lowercase_extension(path)?;
    match extension.as_str() {
        "flac" => Some(0),
        "wav" => Some(1),
        other => AUDIO_EXTENSIONS.contains(&other).then_some(2),
    }
}

/// Cuts the audio tracks of `sheet` into segments.
///
/// `resolve` maps each `FILE` block to the audio file it was resolved to and
/// that file's duration in milliseconds; `None` means the file is missing.
/// Files holding no audio track are never resolved. Each segment ends where
/// the next track of its file starts, so a pregap belongs to the track before
/// it, and the last track of a file ends at that file's duration. A sheet
/// without any audio track is an error, never an empty list.
pub fn segments(
    sheet: &CueSheet,
    resolve: impl Fn(&CueFile) -> Option<(PathBuf, i64)>,
) -> Result<Vec<CueSegment>, CueError> {
    if !sheet
        .files
        .iter()
        .any(|file| file.tracks.iter().any(|track| track.is_audio))
    {
        return Err(CueError::NoAudioTracks);
    }
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for file in &sheet.files {
        if !file.tracks.iter().any(|track| track.is_audio) {
            continue;
        }
        let (path, duration_ms) = resolve(file).ok_or_else(|| CueError::MissingDuration {
            file: file.name.clone(),
        })?;
        if duration_ms < 0 {
            return Err(CueError::InvalidDuration { path, duration_ms });
        }
        if !seen.insert(path.clone()) {
            return Err(CueError::DuplicateFile { path });
        }

        let mut segment_index = 0;
        for (index, track) in file.tracks.iter().enumerate() {
            if !track.is_audio {
                continue;
            }
            let start_ms = frames_to_ms(track.index01);
            if start_ms >= duration_ms {
                return Err(CueError::IndexPastEnd {
                    path,
                    track: track.number,
                    position_ms: start_ms,
                    duration_ms,
                });
            }
            let end_ms = file.tracks.get(index + 1).map_or(duration_ms, |next| {
                frames_to_ms(next.index01).min(duration_ms)
            });
            segment_index += 1;
            result.push(CueSegment {
                path: path.clone(),
                segment_index,
                start_ms,
                end_ms,
                title: track.title.clone(),
                performer: track.performer.clone(),
                track_no: track.number,
                album: sheet.title.clone(),
                album_artist: sheet.performer.clone(),
                date: sheet.date.clone(),
                genre: sheet.genre.clone(),
            });
        }
    }
    Ok(result)
}

fn frames_to_ms(frames: Frames) -> i64 {
    i64::from(frames.0) * 1_000 / 75
}
