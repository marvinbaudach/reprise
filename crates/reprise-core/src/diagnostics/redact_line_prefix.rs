//! The prefix the file mirror writes in front of every message, and the place
//! a path or a file name may reach back to.
//!
//! A relative path or a media file name has no root: the words in front of it
//! may be part of its first segment (`copied Bach: Goldberg Variations/…`), and
//! no punctuation can be trusted to end them, because a name holds all of it.
//! So the redaction reaches back to where the message starts. A line the file
//! mirror wrote starts with a timestamp, a level and a target; those are ours
//! and stay.

/// The levels a log line carries, as the file mirror writes them.
const LEVELS: [&str; 5] = ["TRACE", "DEBUG", "INFO", "WARN", "ERROR"];

/// Where the message of `line` starts: after an exact
/// `<timestamp> <LEVEL> <target>: ` prefix, else at the start of the line. A
/// line that opens with a timestamp alone starts its message behind it.
pub(super) fn message_start(line: &str) -> usize {
    let mut words = Words::new(line);
    let Some((_, stamp_end)) = words
        .next()
        .filter(|&(from, to)| from == 0 && is_rfc3339_timestamp(&line[from..to]))
    else {
        return 0;
    };
    let after_stamp = words.skip_whitespace(stamp_end);
    let Some((level_start, level_end)) = words.next() else {
        return after_stamp;
    };
    if !LEVELS.contains(&&line[level_start..level_end]) {
        return after_stamp;
    }
    match words.next() {
        Some((target_start, target_end)) if line[target_start..target_end].ends_with(':') => {
            words.skip_whitespace(target_end)
        }
        _ => after_stamp,
    }
}

/// Whitespace-separated words of a line, with the offsets they start at.
struct Words<'a> {
    line: &'a str,
    cursor: usize,
}

impl<'a> Words<'a> {
    fn new(line: &'a str) -> Self {
        let cursor = line.len() - line.trim_start().len();
        Self { line, cursor }
    }

    fn next(&mut self) -> Option<(usize, usize)> {
        let rest = &self.line[self.cursor..];
        let start = self.cursor + (rest.len() - rest.trim_start().len());
        let word = &self.line[start..];
        let length = word.find(char::is_whitespace).unwrap_or(word.len());
        if length == 0 {
            return None;
        }
        self.cursor = start + length;
        Some((start, start + length))
    }

    /// The offset of the first non-white character at or after `from`.
    fn skip_whitespace(&self, from: usize) -> usize {
        let rest = &self.line[from..];
        from + (rest.len() - rest.trim_start().len())
    }
}

/// `2026-11-08T12:00:00.123Z`, with an optional fraction and either `Z` or a
/// numeric offset. Every file-mirror line starts with one, and `.123Z` reads
/// as a file extension, so this exact shape is exempt from the filename pass.
/// Nothing looser is: a name that merely resembles a time is still a name.
pub(super) fn is_rfc3339_timestamp(token: &str) -> bool {
    let bytes = token.as_bytes();
    let digits = |range: std::ops::Range<usize>| {
        bytes
            .get(range)
            .is_some_and(|part| part.iter().all(u8::is_ascii_digit))
    };
    let separators = [(4, b'-'), (7, b'-'), (10, b'T'), (13, b':'), (16, b':')];
    if !(digits(0..4)
        && digits(5..7)
        && digits(8..10)
        && digits(11..13)
        && digits(14..16)
        && digits(17..19))
        || !separators
            .iter()
            .all(|(index, expected)| bytes.get(*index) == Some(expected))
    {
        return false;
    }
    let mut end = 19;
    if bytes.get(end) == Some(&b'.') {
        let fraction = bytes[end + 1..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        if fraction == 0 {
            return false;
        }
        end += 1 + fraction;
    }
    match &bytes[end..] {
        b"Z" => true,
        [b'+' | b'-', ..] => {
            digits(end + 1..end + 3)
                && bytes.get(end + 3) == Some(&b':')
                && digits(end + 4..end + 6)
                && bytes.len() == end + 6
        }
        _ => false,
    }
}
