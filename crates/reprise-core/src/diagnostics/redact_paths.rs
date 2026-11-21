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

/// What may open a word without being part of the name in it.
const WORD_OPENERS: [char; 6] = ['(', '[', '{', '"', '\'', '`'];

/// Quote characters: a path opened right after one ends at the matching one.
const QUOTES: [char; 3] = ['"', '\'', '`'];

/// Top-level folders that name a user's data. A slash glued to a word
/// (`…123Z/home/x/Pink Floyd`) that opens one of these starts a path.
const USER_DATA_ROOTS: [&str; 12] = [
    "/home/",
    "/Users/",
    "/media/",
    "/mnt/",
    "/run/",
    "/srv/",
    "/root/",
    "/Volumes/",
    "/net/",
    "/data/",
    "/storage/",
    "/sdcard/",
];

/// The longest stretch scanned for the closing quote of a quoted path. A name
/// longer than this is no file name, and a scan per unclosed quote must not
/// reach to the end of a long line.
const MAX_QUOTED_BYTES: usize = 256;

/// A gvfs mount (`mtp:host=…`, `smb-share:server=…`) or one of its folders.
const GVFS_MARKERS: [&str; 3] = ["/gvfs/", "mtp:host=", "gphoto2:host="];

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
        if !matches!(ch, '/' | '\\') || index == 0 || word[index..].starts_with("//") {
            return false;
        }
        // A backslash is a separator only in front of a name: `\n` and `\"`
        // in a `Debug` string are escapes.
        if ch == '\\' && !word[index + 1..].starts_with(char::is_alphanumeric) {
            return false;
        }
        let before = word[..index].chars().next_back();
        let after = word[index + 1..].chars().next();
        let digits = |side: Option<char>| side.is_some_and(|c| c.is_ascii_digit());
        before.is_some_and(|c| !PATH_OPENERS.contains(&c)) && !(digits(before) && digits(after))
    })
}

/// A word that reads as a part of a name: it opens with a capital or a digit,
/// or it is a joining `&`, `-` or `+`. A sentence goes on in lower case.
fn is_name_word(word: &str) -> bool {
    let word = word.trim_start_matches(WORD_OPENERS);
    word.chars()
        .next()
        .is_some_and(|ch| ch.is_uppercase() || ch.is_ascii_digit())
        || matches!(word, "&" | "-" | "–" | "—" | "+")
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
        for index in 0..LOOKAHEAD_WORDS {
            let Some((start, word_end)) = next_word(input, cursor) else {
                break;
            };
            let word = &input[start..word_end];
            if is_stop_word(word) {
                break;
            }
            // The word right after the name that opens like a name is its
            // next part (`/mnt/usb/Pink Floyd`): a name may end in a space.
            if index == 0 && is_name_word(word) {
                let first = word.chars().next().map_or(1, char::len_utf8);
                extended = Some(span_end(input, start).max(start + first));
                break;
            }
            if has_glued_slash(word) {
                let slash = word
                    .char_indices()
                    .filter(|(_, ch)| matches!(ch, '/' | '\\'))
                    .map(|(index, _)| start + index)
                    .next_back();
                // A word with a glued separator has one, and it is ASCII.
                let after = slash.map_or(word_end, |slash| slash + 1);
                extended = Some(span_end(input, after).max(after));
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

/// The closing quote of a path opened right after `quote`, within the line
/// and [`MAX_QUOTED_BYTES`]. `escaped` is true when the opener was `\"` (a
/// `Debug` string inside a string).
///
/// A quote the name goes on after is an apostrophe, not a closing quote
/// (`'/music/Guns N' Roses'`): see [`name_continues`]. When every quote reads
/// that way there is no closing quote, and the caller takes the rest of the line.
fn closing_quote(input: &str, from: usize, quote: char, escaped: bool) -> Option<usize> {
    let mut skip = false;
    for (offset, ch) in input[from..].char_indices() {
        if offset >= MAX_QUOTED_BYTES || is_line_break(ch) {
            return None;
        }
        let index = from + offset;
        if ch == quote && (escaped || !skip) {
            let end = if escaped && input[..index].ends_with('\\') {
                index - 1
            } else {
                index
            };
            if !name_continues(input, index + quote.len_utf8(), quote) {
                return Some(end);
            }
        }
        skip = ch == '\\' && !skip;
    }
    None
}

/// Whether the text after a candidate closing `quote` still reads as the rest
/// of a path name: it follows the quote without a gap (`N'x`, `'/y`), the next
/// word opens like a name (`N' Roses`) or holds the quote that closes it, or a
/// word within [`LOOKAHEAD_WORDS`] carries a further segment.
fn name_continues(input: &str, after: usize, quote: char) -> bool {
    if input[after..]
        .chars()
        .next()
        .is_some_and(|ch| is_word(ch) || ch == '/')
    {
        return true;
    }
    let mut cursor = after;
    for index in 0..LOOKAHEAD_WORDS {
        let Some((start, end)) = next_word(input, cursor) else {
            return false;
        };
        let word = &input[start..end];
        if is_stop_word(word) {
            return false;
        }
        if index == 0 && (is_name_word(word) || word.contains(quote)) {
            return true;
        }
        if has_glued_slash(word) {
            return true;
        }
        cursor = end;
    }
    false
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
    GVFS_MARKERS.iter().any(|marker| path.contains(marker))
        || path.split(['/', '\\']).any(|segment| {
            let segment = segment.to_lowercase();
            MUSIC_SEGMENTS.contains(&segment.as_str())
        })
}

/// Replaces every URI after its scheme: `mtp://…`.
pub(super) fn redact_uris(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    for (separator, separator_len) in scheme_separators(input) {
        if separator < cursor {
            continue;
        }
        let scheme_start = input[cursor..separator]
            .char_indices()
            .rev()
            .find(|(_, ch)| !is_scheme_char(*ch))
            .map_or(cursor, |(index, ch)| cursor + index + ch.len_utf8());
        let scheme = &input[scheme_start..separator];
        let rest = separator + separator_len;
        if !valid_scheme(scheme) {
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
    output.push_str(&input[cursor..]);
    output
}

/// Every `://`, and its percent-encoded form `%3A%2F%2F`, as (offset, length)
/// in order.
fn scheme_separators(input: &str) -> Vec<(usize, usize)> {
    const ENCODED: &str = "%3a%2f%2f";
    let mut found: Vec<(usize, usize)> =
        input.match_indices("://").map(|(at, _)| (at, 3)).collect();
    // ASCII case folding keeps every offset.
    let folded = input.to_ascii_lowercase();
    found.extend(
        folded
            .match_indices(ENCODED)
            .map(|(at, _)| (at, ENCODED.len())),
    );
    found.sort_unstable();
    found
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
        // A path in the user's data (their disks, phones and shares) takes the
        // rest of its name, whatever it is called: `/mnt/usb/Pink Floyd`.
        let takes_the_line = extent == Extent::Library
            || USER_DATA_ROOTS
                .iter()
                .any(|root| input[start..].starts_with(root))
            || input[..start].ends_with("file:")
            || names_a_library(&input[start..span_end(input, rest)]);
        let extent = if takes_the_line {
            Extent::Library
        } else {
            Extent::Sentence
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
    // Where the current run of name-like words (`Pink Floyd/The Wall`) began.
    let mut run_start: Option<usize> = None;
    let mut since_absolute = usize::MAX;
    while position < input.len() {
        let rest = &input[position..];
        let start = position + rest.len() - rest.trim_start().len();
        if start >= input.len() {
            break;
        }
        if input[position..start].contains(['\n', '\r']) {
            run_start = None;
            since_absolute = usize::MAX;
        }
        let end = input[start..]
            .find(char::is_whitespace)
            .map_or(input.len(), |offset| start + offset);
        position = end;
        let word = &input[start..end];
        // The words after an absolute path belong to its name; the absolute
        // pass reads them, not this one.
        since_absolute = if starts_absolute(word) {
            0
        } else {
            since_absolute.saturating_add(1)
        };
        let found = path_root_in(word)
            .map(|(offset, extent)| (start + offset, extent))
            .or_else(|| {
                (since_absolute > LOOKAHEAD_WORDS && is_capital_slash_word(word)).then(|| {
                    let lead = word.len() - word.trim_start_matches(WORD_OPENERS).len();
                    let begin = run_start.unwrap_or(start + lead).max(cursor);
                    (begin, Extent::Sentence)
                })
            });
        let Some((begin, extent)) = found else {
            run_start = if is_name_word(word) {
                run_start.or(Some(start))
            } else {
                None
            };
            continue;
        };
        output.push_str(&input[cursor..begin]);
        output.push_str(ELLIPSIS);
        let first = input[begin..].chars().next().map_or(1, char::len_utf8);
        cursor = path_end(input, begin, begin, extent).max(begin + first);
        position = cursor;
        run_start = None;
    }
    output.push_str(&input[cursor..]);
    output
}

/// `Floyd/The`, `Albums/Pink`: a slash between two capitalised names is a
/// relative path with spaces in it (`Pink Floyd/The Wall/01 Time.flac`), not a
/// pair like `I/O` or `and/or`.
fn is_capital_slash_word(word: &str) -> bool {
    let word = word.trim_start_matches(WORD_OPENERS);
    // An absolute path or a `$VAR` is read by its own pass.
    if !word.starts_with(char::is_alphanumeric) {
        return false;
    }
    let name = |segment: &str| {
        segment.chars().next().is_some_and(char::is_uppercase)
            && segment.chars().filter(|c| c.is_alphanumeric()).count() >= 2
    };
    // A file name (`x.flac`) is as much a part of a path as a capital.
    let file = |segment: &str| {
        segment
            .rsplit_once('.')
            .is_some_and(|(stem, extension)| !stem.is_empty() && (1..=5).contains(&extension.len()))
    };
    word.char_indices().any(|(index, ch)| {
        if !matches!(ch, '/' | '\\') || index == 0 {
            return false;
        }
        let before = word[..index]
            .rsplit(['/', '\\', '(', '[', '{'])
            .next()
            .unwrap_or("");
        let after = word[index + 1..].split(['/', '\\']).next().unwrap_or("");
        name(before) && (name(after) || file(after))
    })
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
    let lead = word.len() - word.trim_start_matches(WORD_OPENERS).len();
    if has_userinfo(&word[lead..]) {
        return Some((lead, Extent::Sentence));
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
    if tail.starts_with("\\\\?\\") || tail.starts_with("\\\\.\\") {
        return true;
    }
    if tail
        .strip_prefix("\\\\")
        .is_some_and(|name| name.starts_with(char::is_alphanumeric))
    {
        return true;
    }
    if GVFS_MARKERS[1..]
        .iter()
        .any(|marker| tail.starts_with(marker))
    {
        return true;
    }
    if is_drive_start(tail) {
        return true;
    }
    // Our own markers: `$IP/24` is an address with a prefix length.
    if ["$IP", "$MAC", "$HOST", "$USER"]
        .iter()
        .any(|marker| tail.starts_with(marker))
    {
        return false;
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

/// A word that opens an absolute path (`/mnt/x`, `"/mnt/x`, `$HOME/x`, `~/x`).
fn starts_absolute(word: &str) -> bool {
    let word = word.trim_start_matches(PATH_OPENERS);
    word.starts_with(['/', '~', '$', '\\']) || is_drive_start(word)
}
