use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::{CueError, CueFile, CueSheet, Frames};

// Keep this in sync with library::scanner::AUDIO_EXTENSIONS. That constant is
// private, and the scanner belongs to the parallel r128 strand in wave 1.
// The order is the preference when a sheet names an extension that is not on
// disk: lossless containers first, then the rest.
const EXTENSION_PREFERENCE: [&str; 7] = ["flac", "wav", "mp3", "ogg", "opus", "m4a", "aac"];

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

/// Finds the candidate that a `FILE` name refers to.
///
/// Backslashes in `name` count as separators. An exact match wins over a
/// case-insensitive one, and a file with the same stem but another audio
/// extension is the last resort, preferring flac, then wav, then the rest.
/// Ties are broken by sorted path. An absolute `name` only resolves when it is
/// itself in `existing`.
pub fn resolve_file(sheet_dir: &Path, name: &str, existing: &[PathBuf]) -> Option<PathBuf> {
    let referenced = sheet_dir.join(name.replace('\\', "/"));
    if let Some(exact) = existing.iter().find(|path| *path == &referenced) {
        return Some(exact.clone());
    }
    let folded = fold(&referenced);
    if let Some(found) = existing.iter().filter(|path| fold(path) == folded).min() {
        return Some(found.clone());
    }
    let stem = fold(&referenced.with_extension(""));
    existing
        .iter()
        .filter_map(|path| {
            let rank = extension_rank(path)?;
            (fold(&path.with_extension("")) == stem).then_some((rank, path))
        })
        .min()
        .map(|(_, path)| path.clone())
}

fn fold(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}

fn extension_rank(path: &Path) -> Option<usize> {
    let extension = path.extension()?.to_string_lossy().to_lowercase();
    EXTENSION_PREFERENCE
        .iter()
        .position(|candidate| *candidate == extension)
}

/// Cuts the audio tracks of `sheet` into segments.
///
/// `resolve` maps each `FILE` block to the audio file it was resolved to and
/// that file's duration in milliseconds; `None` means the file is missing.
/// Files holding no audio track are never resolved. Each segment ends where
/// the next track of its file starts, so a pregap belongs to the track before
/// it, and the last track of a file ends at that file's duration.
pub fn segments(
    sheet: &CueSheet,
    resolve: impl Fn(&CueFile) -> Option<(PathBuf, i64)>,
) -> Result<Vec<CueSegment>, CueError> {
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
