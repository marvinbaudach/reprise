//! Paths and URIs in free text: where one starts, and how far its name reaches.
//!
//! A path name may hold spaces, commas and brackets (`Earth, Wind & Fire`,
//! `Wish You Were Here [FLAC]`), and a log line cannot tell where the name ends
//! and the sentence goes on. Two extents answer that:
//!
//! - **Library** (a path under the music or home folder, a quoted path, a URI,
//!   a Windows or `$VAR` path, a remote root): the name takes the rest of the
//!   line. It stops only at a closing quote, or in front of a word that starts
//!   something else: a new path, a URI, a known `key=` (the typed fields after
//!   the message stay readable).
//! - **Sentence** (any other absolute path): the name ends at the first word
//!   without a `/` of its own, unless a word within [`LOOKAHEAD_WORDS`] carries
//!   one, in which case everything up to it is part of the name.
//!
//! Over-redaction is the accepted failure here; a leaked name is not.

use super::redact_assignments::starts_known_assignment;

const ELLIPSIS: &str = "…";

/// Words a sentence-extent path looks ahead for a further segment.
const LOOKAHEAD_WORDS: usize = 8;

/// Schemes whose URIs are written percent-encoded, so that a trailing `)`, `,`
/// or `:` belongs to the sentence and not to the address.
const WEB_SCHEMES: [&str; 5] = ["http", "https", "ws", "wss", "ftp"];

/// Characters that may follow a web address without being part of it.
const WEB_TERMINATORS: [char; 8] = ['.', ',', ';', ':', ')', ']', '}', '"'];

/// Characters that end a sentence-extent path name.
const DELIMITERS: [char; 5] = [',', ';', ')', ']', '}'];

/// What may stand directly before a path without being glued to a word.
const PATH_OPENERS: [char; 10] = ['(', '[', '{', '"', '\'', '`', '<', '=', ',', ':'];

/// Quote characters: a path opened right after one ends at the matching one.
const QUOTES: [char; 3] = ['"', '\'', '`'];

/// Top-level folders that name a user's data. A slash glued to a word
/// (`…123Z/home/x/Pink Floyd`) that opens one of these starts a path.
const USER_DATA_ROOTS: [&str; 9] = [
    "/home/",
    "/Users/",
    "/media/",
    "/mnt/",
    "/run/",
    "/srv/",
    "/root/",
    "/Volumes/",
    "/net/",
];

/// Folder names that mark a library: a path holding one takes the whole line.
const MUSIC_SEGMENTS: [&str; 4] = ["music", "musik", "musique", "música"];

#[derive(Clone, Copy, PartialEq)]
enum Extent {
    Sentence,
    Library,
}

fn is_gap(ch: char) -> bool {
    ch.is_whitespace() && !is_line_break(ch)
}

fn is_line_break(ch: char) -> bool {
    matches!(ch, '\n' | '\r')
}

fn is_word(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// The end of the sentence-extent first token: a delimiter, whitespace or the end.
fn span_end(input: &str, start: usize) -> usize {
    input[start..]
        .char_indices()
        .find_map(|(offset, ch)| {
            (DELIMITERS.contains(&ch) || ch.is_whitespace()).then_some(start + offset)
        })
        .unwrap_or(input.len())
}

/// The next word at or after `from` on the same line: a line break ends the
/// search, so no scan ever reaches past the line it started on.
fn next_word(input: &str, from: usize) -> Option<(usize, usize)> {
    let rest = &input[from..];
    let start = from + rest.len() - rest.trim_start_matches(is_gap).len();
    let end = input[start..]
        .find(char::is_whitespace)
        .map_or(input.len(), |offset| start + offset);
    (end > start).then_some((start, end))
}

/// A word that begins something other than the continuation of a path name.
fn is_stop_word(word: &str) -> bool {
    word.contains("://") || starts_new_path(word) || starts_known_assignment(word)
}

fn starts_new_path(word: &str) -> bool {
    let word = word.trim_start_matches(PATH_OPENERS);
    word.starts_with('/')
        || word.starts_with("~/")
        || word.starts_with("\\\\")
        || word.starts_with("$HOME/")
        || is_drive_start(word)
}

fn is_drive_start(word: &str) -> bool {
    let mut chars = word.chars();
    matches!(
        (chars.next(), chars.next(), chars.next()),
        (Some(letter), Some(':'), Some('\\' | '/')) if letter.is_ascii_alphabetic()
    )
}

/// Whether a word carries a `/` that is neither at its start nor right after an
/// opener, and is not `://` or the slash of a number pair (`3/12`).
fn has_glued_slash(word: &str) -> bool {
    word.char_indices().any(|(index, ch)| {
        if ch != '/' || index == 0 || word[index..].starts_with("//") {
            return false;
        }
        let before = word[..index].chars().next_back();
        let after = word[index + 1..].chars().next();
        let digits = |side: Option<char>| side.is_some_and(|c| c.is_ascii_digit());
        before.is_some_and(|c| !PATH_OPENERS.contains(&c)) && !(digits(before) && digits(after))
    })
}

/// The end of the name of a sentence-extent path whose first token ends at `end`.
fn sentence_end(input: &str, mut end: usize) -> usize {
    loop {
        let mut probe = end;
        while let Some(ch) = input[probe..].chars().next() {
            if DELIMITERS.contains(&ch) || is_gap(ch) {
                probe += ch.len_utf8();
            } else {
                break;
            }
        }
        if probe == end {
            return end;
        }
        let mut cursor = probe;
        let mut extended = None;
        for _ in 0..LOOKAHEAD_WORDS {
            let Some((start, word_end)) = next_word(input, cursor) else {
                break;
            };
            let word = &input[start..word_end];
            if is_stop_word(word) {
                break;
            }
            if has_glued_slash(word) {
                let slash = word
                    .char_indices()
                    .filter(|(_, ch)| *ch == '/')
                    .map(|(index, _)| start + index)
                    .next_back()
                    .unwrap_or(start);
                extended = Some(span_end(input, slash + 1).max(slash + 1));
                break;
            }
            cursor = word_end;
        }
        match extended {
            Some(new_end) if new_end > end => end = new_end,
            _ => return end,
        }
    }
}

/// Where a path that takes the rest of the line ends. `web` ends it before a
/// trailing run of sentence punctuation, which is not part of an address.
fn library_end(input: &str, from: usize, web: bool) -> usize {
    let mut end = input[from..]
        .find(char::is_whitespace)
        .map_or(input.len(), |offset| from + offset);
    if let Some(cut) = web_cut(input, from, end, web) {
        return cut;
    }
    while let Some((start, word_end)) = next_word(input, end) {
        if is_stop_word(&input[start..word_end]) {
            return end;
        }
        if let Some(cut) = web_cut(input, start, word_end, web) {
            return cut;
        }
        end = word_end;
    }
    end
}

/// Where a web address ends inside `start..end` when the word closes it with
/// sentence punctuation (`https://host/x):`), or `None` when it goes on.
fn web_cut(input: &str, start: usize, end: usize, web: bool) -> Option<usize> {
    if !web {
        return None;
    }
    let kept = start + input[start..end].trim_end_matches(WEB_TERMINATORS).len();
    (kept < end).then_some(kept)
}

/// The closing quote of a path opened right after `quote`, within the line.
/// `escaped` is true when the opener was `\"` (a `Debug` string inside a string).
fn closing_quote(input: &str, from: usize, quote: char, escaped: bool) -> Option<usize> {
    let mut skip = false;
    for (offset, ch) in input[from..].char_indices() {
        if is_line_break(ch) {
            return None;
        }
        let index = from + offset;
        if ch == quote {
            if escaped {
                let at_backslash = input[..index].ends_with('\\');
                return Some(if at_backslash { index - 1 } else { index });
            }
            if !skip {
                return Some(index);
            }
        }
        skip = ch == '\\' && !skip;
    }
    None
}

/// Where the path that starts at `start` and whose name begins at `rest` ends.
fn path_end(input: &str, start: usize, rest: usize, extent: Extent) -> usize {
    if let Some(quote) = input[..start]
        .chars()
        .next_back()
        .filter(|c| QUOTES.contains(c))
    {
        let escaped = input[..start - quote.len_utf8()].ends_with('\\');
        if let Some(end) = closing_quote(input, rest, quote, escaped) {
            return end;
        }
        return library_end(input, rest, false);
    }
    match extent {
        Extent::Library => library_end(input, rest, false),
        Extent::Sentence => sentence_end(input, span_end(input, rest)),
    }
}

/// A path whose segments name a music folder is a library path.
fn names_a_library(path: &str) -> bool {
    path.split(['/', '\\']).any(|segment| {
        let segment = segment.to_lowercase();
        MUSIC_SEGMENTS.contains(&segment.as_str())
    })
}

/// Replaces every URI after its scheme: `mtp://…`.
pub(super) fn redact_uris(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    while cursor < input.len() {
        let Some(relative) = input[cursor..].find("://") else {
            output.push_str(&input[cursor..]);
            break;
        };
        let separator = cursor + relative;
        let scheme_start = input[..separator]
            .char_indices()
            .rev()
            .find(|(_, ch)| !is_scheme_char(*ch))
            .map_or(0, |(index, ch)| index + ch.len_utf8());
        let scheme = &input[scheme_start..separator];
        let rest = separator + 3;
        if !valid_scheme(scheme) {
            output.push_str(&input[cursor..rest]);
            cursor = rest;
            continue;
        }
        output.push_str(&input[cursor..scheme_start]);
        output.push_str(scheme);
        output.push_str("://…");
        // Our own marker: a second pass over redacted text must not reach
        // into the words after it.
        if input[rest..].starts_with(ELLIPSIS) {
            cursor = rest + ELLIPSIS.len();
            continue;
        }
        let web = WEB_SCHEMES
            .iter()
            .any(|known| scheme.eq_ignore_ascii_case(known));
        cursor = match input[..scheme_start].chars().next_back() {
            Some(quote) if QUOTES.contains(&quote) => {
                let escaped = input[..scheme_start - quote.len_utf8()].ends_with('\\');
                closing_quote(input, rest, quote, escaped)
                    .unwrap_or_else(|| library_end(input, rest, web))
            }
            _ => library_end(input, rest, web),
        };
    }
    output
}

fn valid_scheme(scheme: &str) -> bool {
    scheme
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && scheme.chars().all(is_scheme_char)
}

fn is_scheme_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.')
}

/// Replaces `prefix` and the path name after it (the music or home folder).
pub(super) fn redact_prefixed_path(input: &str, prefix: &str, replacement: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    while let Some(relative) = input[cursor..].find(prefix) {
        let start = cursor + relative;
        output.push_str(&input[cursor..start]);
        output.push_str(replacement);
        cursor = path_end(input, start, start + prefix.len(), Extent::Library);
    }
    output.push_str(&input[cursor..]);
    output
}

/// Replaces every absolute path, a remote root (`nas:/export/music`) and a
/// `/home/…` glued to the word before it.
pub(super) fn redact_absolute_paths(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    while cursor < input.len() {
        let Some(relative) = input[cursor..].find('/') else {
            output.push_str(&input[cursor..]);
            break;
        };
        let start = cursor + relative;
        if input[..start].ends_with(':') && input[start..].starts_with("//…") {
            let end = start + "//…".len();
            output.push_str(&input[cursor..end]);
            cursor = end;
            continue;
        }
        let before = input[..start].chars().next_back();
        let glued = before.is_some_and(is_word);
        if glued
            && !USER_DATA_ROOTS
                .iter()
                .any(|root| input[start..].starts_with(root))
        {
            output.push_str(&input[cursor..start + 1]);
            cursor = start + 1;
            continue;
        }
        if input[start..].starts_with("/…") {
            // Our own marker (`$HOME/…` was written by an earlier pass).
            output.push_str(&input[cursor..start + 1]);
            cursor = start + 1;
            continue;
        }
        let (begin, extent) = match remote_root_start(input, cursor, start) {
            Some(begin) => (begin, Extent::Library),
            None if glued => (start, Extent::Library),
            None => (start, Extent::Sentence),
        };
        let rest = start + 1;
        let extent = if extent == Extent::Sentence
            && names_a_library(&input[start..span_end(input, rest)])
        {
            Extent::Library
        } else {
            extent
        };
        output.push_str(&input[cursor..begin]);
        output.push_str(ELLIPSIS);
        cursor = path_end(input, begin, rest, extent);
    }
    output
}

/// `nas:/export/music` and `user@host:/music`: the host is part of the secret.
/// Returns where the host starts. A known field before the colon
/// (`path:/run/media/x`) is a key, not a host, and a single letter is a drive.
fn remote_root_start(input: &str, floor: usize, slash: usize) -> Option<usize> {
    let before = input[..slash].strip_suffix(':')?;
    let host_start = before
        .char_indices()
        .rev()
        .find(|(_, ch)| !(is_word(*ch) || matches!(ch, '.' | '-' | '@')))
        .map_or(0, |(index, ch)| index + ch.len_utf8())
        .max(floor);
    let host = &before[host_start..];
    let single_letter = host.len() == 1 && host.chars().all(|ch| ch.is_ascii_alphabetic());
    let named = host.chars().next().is_some_and(char::is_alphanumeric);
    let a_key = super::redact_assignments::is_known_field(&host.to_ascii_lowercase());
    (named && !single_letter && !a_key).then_some(host_start)
}

/// `$HOME/…`, `${HOME}/…`, `~/…`, `~bob/…`, `\\nas\music`, `C:\Users\…`, and a
/// relative `Music/Pink Floyd/…`: a word that starts one of these replaces the
/// word and the rest of the name.
pub(super) fn redact_path_roots(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    let mut position = 0;
    while position < input.len() {
        let rest = &input[position..];
        let start = position + rest.len() - rest.trim_start().len();
        if start >= input.len() {
            break;
        }
        let end = input[start..]
            .find(char::is_whitespace)
            .map_or(input.len(), |offset| start + offset);
        position = end;
        let Some((offset, extent)) = path_root_in(&input[start..end]) else {
            continue;
        };
        let begin = start + offset;
        output.push_str(&input[cursor..begin]);
        output.push_str(ELLIPSIS);
        let first = input[begin..].chars().next().map_or(1, char::len_utf8);
        cursor = path_end(input, begin, begin, extent).max(begin + first);
        position = cursor;
    }
    output.push_str(&input[cursor..]);
    output
}

/// Where in `word` a rooted or relative path starts, and how far it reaches.
fn path_root_in(word: &str) -> Option<(usize, Extent)> {
    if word.contains(ELLIPSIS) || word.contains("://") {
        return None;
    }
    // A rooted path may follow any opener (`state=$HOME/x`); a relative one
    // starts after the last opener in front of its first slash.
    let opener_end =
        |(index, ch): (usize, char)| PATH_OPENERS.contains(&ch).then_some(index + ch.len_utf8());
    let first_slash = word.find('/').unwrap_or(word.len());
    let relative_start = word[..first_slash]
        .char_indices()
        .filter_map(opener_end)
        .next_back()
        .unwrap_or(0);
    let rooted = std::iter::once(0)
        .chain(word.char_indices().filter_map(opener_end))
        .find(|offset| is_rooted(&word[*offset..]));
    if let Some(offset) = rooted {
        return Some((offset, Extent::Library));
    }
    let tail = &word[relative_start..];
    is_relative_path(tail).then(|| {
        let glued_root = USER_DATA_ROOTS.iter().any(|root| tail.contains(root));
        let extent = if names_a_library(tail) || glued_root {
            Extent::Library
        } else {
            Extent::Sentence
        };
        (relative_start, extent)
    })
}

fn is_rooted(tail: &str) -> bool {
    if tail.starts_with("\\\\") && tail.len() > 2 {
        return true;
    }
    if is_drive_start(tail) {
        return true;
    }
    let after_sigil = if let Some(rest) = tail.strip_prefix("${") {
        rest.split_once('}').map(|(_, after)| after)
    } else if let Some(rest) = tail.strip_prefix('$') {
        let name = rest.len()
            - rest
                .trim_start_matches(|c: char| c.is_ascii_alphanumeric() || c == '_')
                .len();
        (name > 0).then(|| &rest[name..])
    } else {
        tail.strip_prefix('~').map(|rest| {
            rest.trim_start_matches(|c: char| {
                c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')
            })
        })
    };
    after_sigil.is_some_and(|after| {
        (after.starts_with('/') || after.starts_with('\\')) && !after[1..].starts_with(ELLIPSIS)
    })
}

/// `Music/Pink Floyd/…`, `a/b/c`: a word with two slashes, or one that opens
/// with a music folder name. A pair of numbers (`3/12`, `12/11/2026`) is not.
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
