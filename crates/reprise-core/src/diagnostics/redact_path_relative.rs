//! Where a relative path starts, read one word at a time: `Pink Floyd/the
//! wall/01 in the flesh.flac`, `Pink Floyd/wish you were here`, `bob:pw@host`,
//! a gvfs mount.
//!
//! A relative path has no root to find, and its first segment may hold spaces,
//! punctuation and brackets, so the words before the first slash belong to it
//! too. They are taken back to where the message starts (`redact_line_prefix`):
//! no punctuation ends a name, and a leading verb is the price of that.
//!
//! The decision is a closed list, not a guess about names: a word with a slash
//! in it is a path unless it is one of the few shapes that are known not to be
//! one (`I/O`, `read/write`, `TCP/IP`, `audio/flac`, `MB/s`, `24/7`,
//! `12/11/2026`). A name with a slash that nobody listed is redacted.

use super::redact_assignments::is_known_field;
use super::redact_line_prefix::message_start;
use super::redact_paths::Cut;
use super::redact_slash_words::has_slash_in_a_name;

/// What may open a word without being part of the name in it: brackets and
/// quotes of every script.
const WORD_OPENERS: [char; 20] = [
    '(', '[', '{', '<', '"', '\'', '`', '“', '‘', '„', '‚', '«', '‹', '「', '『', '【', '〈', '《',
    '（', '＜',
];

/// What may stand directly before a path without being glued to a word.
const PATH_DELIMITERS: [char; 3] = ['=', ',', ':'];

type Word = (usize, usize);

/// The first relative path, userinfo or gvfs mount on `line` that starts in a
/// word before `limit`.
pub(super) fn relative_trigger(line: &str, limit: usize) -> Option<Cut> {
    let floor = message_start(line);
    for &(start, end) in &words_of(line) {
        if start >= limit {
            break;
        }
        let original = &line[start..end];
        // The prefix of a file-mirror line is ours, and a URI has its own pass.
        if start < floor || original.contains("://") {
            continue;
        }
        let shaped = backslashes_as_slashes(original);
        let word = shaped.as_str();
        let lead = word.len() - word.trim_start_matches(WORD_OPENERS).len();
        if has_userinfo(&word[lead..]) || is_gvfs_mount(&word[lead..]) {
            return Some(Cut::path(start + lead));
        }
        let tail_start = relative_start(word);
        // An absolute path that starts in this word is read by its own pass.
        if start + tail_start >= limit {
            continue;
        }
        if has_slash_in_a_name(&word[tail_start..]) {
            return Some(Cut::path(floor));
        }
    }
    None
}

/// The word with every backslash that separates two names written as a slash.
/// A backslash in front of one of the letters a `Debug` string escapes with
/// (`\n`, `\t`, `\0`) separates too: `Pink\the wall` and `..\nirvana\nevermind` are
/// as likely folders as line breaks, and a line break read as a folder costs a
/// line, a folder read as a line break costs a name. Both are one byte long, so
/// every offset still holds.
fn backslashes_as_slashes(word: &str) -> String {
    let mut shaped = String::with_capacity(word.len());
    let mut previous: Option<char> = None;
    let mut characters = word.chars().peekable();
    while let Some(ch) = characters.next() {
        let separator = ch == '\\'
            && previous.is_some()
            && characters.peek().is_some_and(|next| next.is_alphanumeric());
        shaped.push(if separator { '/' } else { ch });
        previous = Some(ch);
    }
    shaped
}

fn words_of(line: &str) -> Vec<Word> {
    let mut words = Vec::new();
    let mut start = None;
    for (index, ch) in line.char_indices() {
        match (start, ch.is_whitespace()) {
            (None, false) => start = Some(index),
            (Some(from), true) => {
                words.push((from, index));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        words.push((from, line.len()));
    }
    words
}

/// Where the word's path starts: after the last opener in front of its first
/// slash (`to=Pink/Floyd/x` starts at `Pink`).
fn relative_start(word: &str) -> usize {
    let first_slash = word.find('/').unwrap_or(word.len());
    word[..first_slash]
        .char_indices()
        .filter(|(_, ch)| WORD_OPENERS.contains(ch) || PATH_DELIMITERS.contains(ch))
        .map(|(index, ch)| index + ch.len_utf8())
        .next_back()
        .unwrap_or(0)
}

/// `bob:pw@example.com/x`: credentials in front of a host, without a scheme.
fn has_userinfo(tail: &str) -> bool {
    let Some(at) = tail.find('@') else {
        return false;
    };
    let before = &tail[..at];
    // `bob:pw@host` (credentials) and `bob@host:path` (an scp-style remote).
    (before.contains(':') || tail[at + 1..].contains(':'))
        && !before.contains(['/', '\\', '=', '…'])
        && before.chars().next().is_some_and(char::is_alphanumeric)
        && tail[at + 1..]
            .chars()
            .next()
            .is_some_and(char::is_alphanumeric)
}

/// `mtp:host=Pixel`, `smb-share:server=nas`: a gvfs mount name.
fn is_gvfs_mount(tail: &str) -> bool {
    ["host", "server"].iter().any(|key| {
        tail.split_once(&format!(":{key}="))
            .is_some_and(|(scheme, _)| {
                !scheme.is_empty()
                    && scheme
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
                    && !is_known_field(&scheme.to_ascii_lowercase())
            })
    })
}
