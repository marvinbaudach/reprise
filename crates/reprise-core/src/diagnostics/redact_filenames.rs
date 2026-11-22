//! File names in free text: a word that ends in an extension.
//!
//! Any such word is replaced. A media or image file name (`Time.flac`,
//! `cover.jpg`) may have spaces in it (`Dark Side of the Moon.flac`), so its
//! words in front of it go too, back to where the message starts: see
//! `redact_line_prefix`.

use super::redact_assignments::is_kept_number;
use super::redact_line_prefix::{is_rfc3339_timestamp, message_start};
use super::redact_markers::ELLIPSIS;

/// Extensions of the files a library holds or a player reads next to them. A
/// name that ends in one is a track, a cover, a playlist or a cue sheet, and
/// the words before it are the title.
const MEDIA_EXTENSIONS: [&str; 40] = [
    "aac", "aif", "aiff", "alac", "ape", "avi", "avif", "bmp", "cue", "flac", "gif", "heic",
    "jpeg", "jpg", "lrc", "m3u", "m3u8", "m4a", "m4b", "m4v", "mka", "mkv", "mov", "mp3", "mp4",
    "mpc", "oga", "ogg", "opus", "pls", "png", "svg", "tif", "tiff", "wav", "webm", "webp", "wma",
    "wv", "wvx",
];

/// The longest extension that makes a word a file name.
const MAX_EXTENSION: usize = 12;

pub(super) fn redact_filenames(input: &str) -> String {
    input
        .split_inclusive(['\n', '\r'])
        .map(redact_line)
        .collect()
}

fn redact_line(segment: &str) -> String {
    let line = segment.trim_end_matches(['\n', '\r']);
    let line_break = &segment[line.len()..];
    let floor = message_start(line);
    let mut output = String::with_capacity(segment.len());
    // How much of the output is the prefix of the line, which a media file
    // name never takes back.
    let mut kept = 0;
    let mut offset = 0;
    for token in line.split_inclusive(char::is_whitespace) {
        let body = token.trim_end_matches(char::is_whitespace);
        let whitespace = &token[body.len()..];
        let name = file_name_of(body);
        match name {
            Some(name) if is_media(name) => {
                output.truncate(kept);
                output.push_str(ELLIPSIS);
                output.push_str(whitespace);
            }
            Some(_) => {
                output.push_str(ELLIPSIS);
                output.push_str(whitespace);
            }
            None => output.push_str(token),
        }
        offset += token.len();
        if offset <= floor {
            kept = output.len();
        }
    }
    output.push_str(line_break);
    output
}

/// The file name a word is, without the punctuation that touches it in
/// running text (`Time.flac.`, `“Time.flac”`, `<cover.jpg>`, `*x.mp3*`).
fn file_name_of(body: &str) -> Option<&str> {
    let trimmed = body.trim_matches(|ch: char| !ch.is_alphanumeric());
    (looks_like_filename(trimmed) && !is_rfc3339_timestamp(trimmed) && !is_kept_number(body))
        .then_some(trimmed)
}

/// Whether the word ends in the extension of a track, a cover or a playlist.
pub(super) fn has_media_extension(name: &str) -> bool {
    is_media(name.trim_matches(|ch: char| !ch.is_alphanumeric()))
}

fn is_media(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, extension)| {
        MEDIA_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
    })
}

fn looks_like_filename(token: &str) -> bool {
    let Some((stem, extension)) = token.rsplit_once('.') else {
        return false;
    };
    !stem.is_empty()
        && !is_number(stem.rsplit('/').next().unwrap_or(stem))
        && !is_abbreviation(token)
        && (1..=MAX_EXTENSION).contains(&extension.len())
        && extension.chars().all(|ch| ch.is_ascii_alphanumeric())
}

/// Digits and dots: a version, a ratio, a rate (`0.1.331`, `16/44.1`,
/// `HTTP/1.1` end in one). Nothing in it is a name.
fn is_number(segment: &str) -> bool {
    segment.chars().all(|ch| ch.is_ascii_digit() || ch == '.')
}

/// `e.g`, `i.e`, `p.m`, `U.S.A`: letters between dots, one at a time.
fn is_abbreviation(token: &str) -> bool {
    token
        .split('.')
        .all(|part| part.chars().count() == 1 && part.chars().all(char::is_alphabetic))
}
