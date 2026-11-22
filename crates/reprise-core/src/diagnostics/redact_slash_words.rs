//! Which words with a slash in them are not paths.
//!
//! A relative path has no root, so the shape is all there is: a word with a
//! slash in it is read as a path, and these are the exceptions, kept short on
//! purpose. A pair of ordinary words that is not on the list is redacted, and
//! that is the price of not guessing which names are ordinary.

/// Pairs of words and types that are written with a slash and carry no name.
/// Lower case: the comparison folds the case, so `TCP/IP` and `I/O` match.
const ORDINARY_PAIRS: [&str; 57] = [
    "and/or",
    "application/json",
    "application/octet-stream",
    "application/ogg",
    "application/x-id3",
    "application/xml",
    "audio/aac",
    "audio/flac",
    "audio/mp4",
    "audio/mpeg",
    "audio/ogg",
    "audio/opus",
    "audio/wav",
    "audio/webm",
    "audio/x-ape",
    "audio/x-flac",
    "audio/x-m4a",
    "audio/x-opus",
    "audio/x-raw",
    "audio/x-vorbis",
    "audio/x-wav",
    "audio/x-wavpack",
    "client/server",
    "disable/enable",
    "either/or",
    "enable/disable",
    "he/she",
    "his/her",
    "i/o",
    "image/gif",
    "image/jpeg",
    "image/png",
    "image/svg+xml",
    "image/webp",
    "input/output",
    "ipv4/ipv6",
    "n/a",
    "off/on",
    "on/off",
    "pass/fail",
    "pause/play",
    "play/pause",
    "read/write",
    "start/stop",
    "stop/start",
    "tcp/ip",
    "tcp/udp",
    "text/html",
    "text/plain",
    "text/xml",
    "true/false",
    "udp/ip",
    "video/mp4",
    "video/webm",
    "video/x-raw",
    "w/o",
    "yes/no",
];

/// The word after the slash of a rate: `MB/s`, `km/h`, `12/min`.
const PER_UNITS: [&str; 6] = ["s", "sec", "min", "h", "hr", "ms"];

/// The longest word in front of the slash of a rate that is a unit
/// (`kbit/s`, `frames/s`).
const MAX_UNIT_LETTERS: usize = 6;

/// Protocols that name a version after the slash: `HTTP/1.1`.
const VERSIONED_PROTOCOLS: [&str; 5] = ["http", "rtsp", "rtp", "sip", "icy"];

/// Whether the word, from where its path would start, has a slash in a name:
/// a word with a slash that is none of the known ordinary shapes.
pub(super) fn has_slash_in_a_name(word: &str) -> bool {
    let word = word.trim_end_matches(TRAILING);
    // `../Pink Floyd/x` and `\nirvana\x` have a root of their own and a name
    // behind it.
    let bare = word.trim_start_matches(['.', '\\']);
    let word = if bare.len() == word.len() {
        word
    } else {
        bare.trim_start_matches('/')
    };
    word.starts_with(char::is_alphanumeric) && word.contains('/') && !is_ordinary(word)
}

/// Punctuation that ends a word in running text without being part of it.
const TRAILING: [char; 9] = [',', ';', '.', '!', '?', ')', ']', '}', '"'];

fn is_ordinary(word: &str) -> bool {
    let segments: Vec<&str> = word
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.iter().all(|segment| is_number(segment)) {
        return true;
    }
    if ORDINARY_PAIRS.contains(&word.to_lowercase().as_str()) {
        return true;
    }
    let [first, second] = segments.as_slice() else {
        return false;
    };
    // The slash must not be the first or the last character of the word: a
    // directory (`Pink Floyd/`) is not a rate.
    word.split('/').all(|segment| !segment.is_empty())
        && (is_rate(first, second) || is_versioned_protocol(first, second))
}

/// `12`, `44.1`, `1,5`, `12:30`, `-3`, `50%`: digits and their punctuation.
fn is_number(segment: &str) -> bool {
    segment.chars().any(|ch| ch.is_ascii_digit())
        && segment
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, '.' | ',' | ':' | '-' | '%'))
}

fn is_rate(per: &str, unit: &str) -> bool {
    PER_UNITS.contains(&unit.to_lowercase().as_str())
        && (is_number(per)
            || per.chars().count() <= MAX_UNIT_LETTERS && per.chars().all(char::is_alphabetic))
}

fn is_versioned_protocol(protocol: &str, version: &str) -> bool {
    VERSIONED_PROTOCOLS.contains(&protocol.to_lowercase().as_str()) && is_number(version)
}
