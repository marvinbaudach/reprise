//! Where a relative path starts, read one word at a time: `Pink Floyd/the
//! wall/01 in the flesh.flac`, `Music/Pink Floyd`, `bob:pw@host`, a gvfs mount.
//!
//! A relative path has no root to find, and its first segment may hold spaces,
//! so the words before the first slash belong to it too. They are taken back
//! to the last hard delimiter on the line (a `: `, a `=`, an opening bracket or
//! quote) or to the start of the line; a lower-case artist name is as much a
//! part of the path as a capitalised one.
//!
//! A single slash between two words (`I/O`, `read/write`, `3/12`) is not a
//! path. Four shapes are: three or more segments in one word, a folder named
//! `Music`, two capitalised names (or a name and a file), and two words that
//! each carry a slash within [`CHAIN_WINDOW`] words of each other.

use super::redact_assignments::is_known_field;
use super::redact_paths::Cut;

/// What may open a word without being part of the name in it: brackets and
/// quotes of every script.
const WORD_OPENERS: [char; 20] = [
    '(', '[', '{', '<', '"', '\'', '`', '“', '‘', '„', '‚', '«', '‹', '「', '『', '【', '〈', '《',
    '（', '＜',
];

/// What may stand directly before a path without being glued to a word.
const PATH_DELIMITERS: [char; 3] = ['=', ',', ':'];

/// Quote characters that open a quotation when they start a word.
const QUOTES: [char; 10] = ['"', '\'', '`', '“', '‘', '„', '‚', '«', '‹', '「'];

/// Folder names that mark a library.
const MUSIC_SEGMENTS: [&str; 4] = ["music", "musik", "musique", "música"];

/// How many words after a word with a slash another one may follow for the two
/// to read as one path.
const CHAIN_WINDOW: usize = 8;

/// The files a name segment may be: `x.flac` is as much a part of a path as a
/// capital is.
const MAX_EXTENSION: usize = 5;

type Word = (usize, usize);

/// The first relative path, userinfo or gvfs mount on `line` that starts in a
/// word before `limit`.
pub(super) fn relative_trigger(line: &str, limit: usize) -> Option<Cut> {
    let words: Vec<Word> = words_of(line);
    for (index, &(start, end)) in words.iter().enumerate() {
        if start >= limit {
            break;
        }
        let original = &line[start..end];
        if original.contains("://") {
            continue;
        }
        // A backslash in front of a name separates names; in front of an escape
        // letter (`\n`) it is an escape.
        let shaped = backslashes_as_slashes(original);
        let word = shaped.as_str();
        let lead = word.len() - word.trim_start_matches(WORD_OPENERS).len();
        if has_userinfo(&word[lead..]) || is_gvfs_mount(&word[lead..]) {
            return Some(Cut::path(start + lead));
        }
        let relative_start = relative_start(word);
        let tail = &word[relative_start..];
        let by_shape = is_relative_path(tail) || is_capital_slash_word(word);
        if by_shape || is_chained(line, &words, index, tail) {
            let begin = if relative_start > lead {
                // A `key=` inside the word ends what the path can reach back to.
                start + relative_start
            } else {
                reach_back(line, &words[..index], start + lead)
            };
            return Some(Cut::path(begin));
        }
    }
    None
}

/// Whether a backslash in front of `ch` separates two names. In front of one of
/// the letters a `Debug` string escapes with (`\n`, `\r`, `\t`, `\u{..}`,
/// `\x41`, `\0`) it is an escape.
fn is_name_start(ch: char) -> bool {
    ch.is_alphanumeric() && !matches!(ch, 'n' | 'r' | 't' | 'u' | 'x' | '0')
}

/// The word with every backslash that separates two names written as a slash.
/// Both are one byte long, so every offset still holds.
fn backslashes_as_slashes(word: &str) -> String {
    let mut shaped = String::with_capacity(word.len());
    let mut previous: Option<char> = None;
    let mut characters = word.chars().peekable();
    while let Some(ch) = characters.next() {
        let separator = ch == '\\'
            && previous.is_some_and(char::is_alphanumeric)
            && characters.peek().is_some_and(|next| is_name_start(*next));
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

/// Back over the words in front of the first slash, to where the path's first
/// segment begins.
fn reach_back(line: &str, before: &[Word], mut begin: usize) -> usize {
    for &(start, end) in before.iter().rev() {
        let previous = &line[start..end];
        if previous.ends_with([':', ';']) {
            break;
        }
        if let Some(at) = previous.rfind(['=', '(', '[', '{', '<']) {
            begin = start + at + 1;
            break;
        }
        if let Some(quote) = previous.chars().next().filter(|ch| QUOTES.contains(ch)) {
            begin = start + quote.len_utf8();
            break;
        }
        begin = start;
    }
    begin
}

/// `Music/Pink Floyd`, or three non-empty segments that are not all numbers
/// (`3/12`, `12/11/2026`).
fn is_relative_path(tail: &str) -> bool {
    let segments: Vec<&str> = tail.split('/').collect();
    if segments.len() < 2
        || segments
            .iter()
            .all(|s| s.chars().all(|c| c.is_ascii_digit()))
    {
        return false;
    }
    let first = segments.first().copied().unwrap_or_default().to_lowercase();
    segments.len() >= 3 && segments.iter().all(|s| !s.is_empty())
        || MUSIC_SEGMENTS.contains(&first.as_str())
            && segments.get(1).is_some_and(|s| !s.is_empty())
}

/// `Floyd/The`, `Albums/Pink`: a slash between two capitalised names is a
/// relative path with spaces in it (`Pink Floyd/The Wall/01 Time.flac`), not a
/// pair like `I/O` or `and/or`. A script without case counts as capitalised.
fn is_capital_slash_word(word: &str) -> bool {
    let word = word.trim_start_matches(WORD_OPENERS);
    // An absolute path or a `$VAR` is read by its own pass.
    if !word.starts_with(char::is_alphanumeric) {
        return false;
    }
    let name = |segment: &str| {
        segment.chars().next().is_some_and(is_capital)
            && segment.chars().filter(|c| c.is_alphanumeric()).count() >= 2
    };
    let file = |segment: &str| {
        segment.rsplit_once('.').is_some_and(|(stem, extension)| {
            !stem.is_empty() && (1..=MAX_EXTENSION).contains(&extension.len())
        })
    };
    word.char_indices().any(|(index, ch)| {
        if ch != '/' || index == 0 {
            return false;
        }
        let before = word[..index]
            .rsplit(['/', '(', '[', '{'])
            .next()
            .unwrap_or("");
        let after = word[index + 1..].split('/').next().unwrap_or("");
        name(before) && (name(after) || file(after))
    })
}

fn is_capital(ch: char) -> bool {
    ch.is_uppercase() || (ch.is_alphabetic() && !ch.is_lowercase())
}

/// A word with a slash after a name of its own (`Floyd/the`, `wall/01`), and
/// another one within [`CHAIN_WINDOW`] words after it.
fn is_chained(line: &str, words: &[Word], index: usize, tail: &str) -> bool {
    is_slash_word(tail)
        && words
            .iter()
            .skip(index + 1)
            .take(CHAIN_WINDOW)
            .any(|&(start, end)| {
                let next = backslashes_as_slashes(&line[start..end]);
                !next.contains("://") && is_slash_word(&next[relative_start(&next)..])
            })
}

fn is_slash_word(tail: &str) -> bool {
    let mut segments = tail.split('/');
    let (Some(first), Some(second)) = (segments.next(), segments.next()) else {
        return false;
    };
    first.starts_with(char::is_alphanumeric)
        && first.chars().filter(|ch| ch.is_alphanumeric()).count() >= 2
        && first.chars().any(char::is_alphabetic)
        && !second.is_empty()
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
