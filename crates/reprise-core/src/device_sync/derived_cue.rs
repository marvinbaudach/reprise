//! The CUE sheet Reprise writes beside a synced CUE file on the device.
//!
//! A CUE file reaches the device once, transcoded per the profile like any
//! file, so the sheet that cut it on the desktop no longer fits: its `FILE`
//! line names the source, and a sheet embedded in a FLAC does not survive a
//! transcode. The derived sheet names the device file and places every track
//! of the file still in the library at its own start, so the phone cuts the
//! file the way the desktop did (CUE-15, CUE-16). It has no source on the
//! desktop: the plan carries its bytes, and nothing is ever written into the
//! music collection.

use super::cue_files::CueSyncFile;

/// CD frames per second, the unit of a sheet's `INDEX`.
const FRAMES_PER_SECOND: i64 = 75;
const MS_PER_SECOND: i64 = 1_000;
const SECONDS_PER_MINUTE: i64 = 60;

/// The derived sheet's path beside `device_path`, the audio file it describes.
pub fn sheet_path(device_path: &str) -> String {
    let stem_end = device_path
        .rfind('.')
        .filter(|dot| !device_path[*dot..].contains('/'))
        .unwrap_or(device_path.len());
    format!("{}.cue", &device_path[..stem_end])
}

/// The sheet for `file` on the device, where its audio is `device_path`.
pub fn render(file: &CueSyncFile, device_path: &str) -> String {
    let file_name = device_path.rsplit('/').next().unwrap_or(device_path);
    let mut sheet = String::new();
    if let Some(year) = file.year {
        sheet.push_str(&format!("REM DATE {year}\n"));
    }
    if !file.genre.trim().is_empty() {
        sheet.push_str(&format!("REM GENRE {}\n", quoted(&file.genre)));
    }
    if !file.album_artist.trim().is_empty() {
        sheet.push_str(&format!("PERFORMER {}\n", quoted(&file.album_artist)));
    }
    if !file.album.trim().is_empty() {
        sheet.push_str(&format!("TITLE {}\n", quoted(&file.album)));
    }
    sheet.push_str(&format!("FILE {} WAVE\n", quoted(file_name)));
    for (position, track) in file.tracks.iter().enumerate() {
        let number = track
            .track_no
            .unwrap_or_else(|| u32::try_from(position + 1).unwrap_or(u32::MAX));
        sheet.push_str(&format!("  TRACK {number:02} AUDIO\n"));
        sheet.push_str(&format!("    TITLE {}\n", quoted(&track.title)));
        if !track.performer.trim().is_empty() && track.performer != file.album_artist {
            sheet.push_str(&format!("    PERFORMER {}\n", quoted(&track.performer)));
        }
        sheet.push_str(&format!("    INDEX 01 {}\n", index_time(track.start_ms)));
    }
    sheet
}

/// A value as a sheet quotes it: the parser keeps quotes inside a value that
/// ends in one, and a line break would end the statement, so only control
/// characters are dropped.
fn quoted(value: &str) -> String {
    let clean: String = value.chars().filter(|c| !c.is_control()).collect();
    format!("\"{clean}\"")
}

/// `MM:SS:FF` for a start in milliseconds. The parser turns frames into
/// milliseconds by rounding down, so rounding up here gives back the very
/// frame a start was read from, and the phone reads the same start again.
fn index_time(start_ms: i64) -> String {
    let frames_total = (start_ms.max(0) * FRAMES_PER_SECOND + MS_PER_SECOND - 1) / MS_PER_SECOND;
    let frames = frames_total % FRAMES_PER_SECOND;
    let seconds_total = frames_total / FRAMES_PER_SECOND;
    let seconds = seconds_total % SECONDS_PER_MINUTE;
    let minutes = seconds_total / SECONDS_PER_MINUTE;
    format!("{minutes:02}:{seconds:02}:{frames:02}")
}

#[cfg(test)]
#[path = "derived_cue_tests.rs"]
mod tests;
