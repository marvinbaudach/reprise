use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{CueError, CueSheet};

// Keep this in sync with library::scanner::AUDIO_EXTENSIONS. That constant is
// private, and the scanner belongs to the parallel r128 strand in wave 1.
const AUDIO_EXTENSIONS: [&str; 7] = ["mp3", "flac", "ogg", "opus", "m4a", "aac", "wav"];

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

pub fn resolve_file(sheet_dir: &Path, name: &str, existing: &[PathBuf]) -> Option<PathBuf> {
    let referenced = sheet_dir.join(name);
    existing
        .iter()
        .find(|path| *path == &referenced)
        .or_else(|| {
            existing
                .iter()
                .find(|path| path_eq_ignore_case(path, &referenced))
        })
        .or_else(|| {
            AUDIO_EXTENSIONS.iter().find_map(|extension| {
                let encoded = referenced.with_extension(extension);
                existing
                    .iter()
                    .find(|path| path_eq_ignore_case(path, &encoded))
            })
        })
        .cloned()
}

fn path_eq_ignore_case(left: &Path, right: &Path) -> bool {
    left.to_string_lossy().to_lowercase() == right.to_string_lossy().to_lowercase()
}

pub fn segments(
    sheet: &CueSheet,
    durations: &HashMap<PathBuf, i64>,
) -> Result<Vec<CueSegment>, CueError> {
    let mut result = Vec::new();
    let mut path_indices = HashMap::<PathBuf, i64>::new();
    for file in &sheet.files {
        let (path, duration_ms) =
            duration_for_file(&file.name, durations).ok_or_else(|| CueError::MissingDuration {
                file: file.name.clone(),
            })?;
        if *duration_ms < 0 {
            return Err(CueError::InvalidDuration {
                path: path.clone(),
                duration_ms: *duration_ms,
            });
        }
        for track in &file.tracks {
            let position_ms = frames_to_ms(track.index01);
            if position_ms > *duration_ms {
                return Err(CueError::IndexPastEnd {
                    path: path.clone(),
                    track: track.number,
                    position_ms,
                    duration_ms: *duration_ms,
                });
            }
        }

        for (index, track) in file.tracks.iter().enumerate() {
            let end_ms = file
                .tracks
                .get(index + 1)
                .map_or(*duration_ms, |next| frames_to_ms(next.index01));
            let segment_index = path_indices.entry(path.clone()).or_default();
            *segment_index += 1;
            result.push(CueSegment {
                path: path.clone(),
                segment_index: *segment_index,
                start_ms: frames_to_ms(track.index01),
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

fn duration_for_file<'a>(
    name: &str,
    durations: &'a HashMap<PathBuf, i64>,
) -> Option<(&'a PathBuf, &'a i64)> {
    let referenced = Path::new(name);
    durations
        .get_key_value(referenced)
        .or_else(|| {
            durations
                .iter()
                .filter(|(path, _)| path.ends_with(referenced))
                .min_by(|(left, _), (right, _)| left.cmp(right))
        })
        .or_else(|| {
            durations
                .iter()
                .filter(|(path, _)| path_ends_with_ignore_case(path, referenced))
                .min_by(|(left, _), (right, _)| left.cmp(right))
        })
}

fn path_ends_with_ignore_case(path: &Path, suffix: &Path) -> bool {
    let mut path_components = path.components().rev();
    suffix.components().rev().all(|suffix_component| {
        path_components.next().is_some_and(|path_component| {
            path_component.as_os_str().to_string_lossy().to_lowercase()
                == suffix_component
                    .as_os_str()
                    .to_string_lossy()
                    .to_lowercase()
        })
    })
}

fn frames_to_ms(frames: super::Frames) -> i64 {
    i64::from(frames.0) * 1_000 / 75
}
