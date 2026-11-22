//! Where a path starts, read one character at a time: an absolute path (under
//! any root, after any quote, glued to a `key=`), a URI, a `~`, `$VAR` or
//! `%VAR%` root, a drive or UNC path and a remote root.

use super::redact_assignments::{is_known_field, is_typed_field_list};
use super::redact_markers::{is_word, ELLIPSIS};
use super::redact_paths::Cut;

/// Top-level folders that name a user's data. A slash glued to a word
/// (`…123Z/home/x/Pink Floyd`) that opens one of these starts a path.
const GLUED_ROOTS: [&str; 12] = [
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

/// The longest `${NAME}` that is read.
const MAX_BRACED_NAME: usize = 64;

/// The longest prefix length (`$IP/24`, `$IP/128`) that is left alone.
const MAX_PREFIX_DIGITS: usize = 3;

/// The first path start on `line`, reading left to right.
pub(super) fn first_trigger(line: &str) -> Option<Cut> {
    let mut previous: Option<char> = None;
    let mut skip_to = 0;
    for (index, ch) in line.char_indices() {
        if index < skip_to {
            previous = Some(ch);
            continue;
        }
        let at_boundary = previous.is_none_or(|before| !is_word(before));
        let hit = match ch {
            ':' => match uri_outcome(line, index) {
                Some(Ok(cut)) => Some(cut),
                Some(Err(past_marker)) => {
                    skip_to = past_marker;
                    None
                }
                None => None,
            },
            '/' => slash_cut(line, index, previous),
            '\\' if at_boundary => backslash_cut(line, index),
            '~' if at_boundary => tilde_cut(line, index),
            '$' if at_boundary => dollar_cut(line, index),
            '%' if at_boundary => percent_cut(line, index),
            letter if at_boundary && letter.is_ascii_alphabetic() => drive_cut(line, index),
            _ => None,
        };
        if hit.is_some() {
            return hit;
        }
        previous = Some(ch);
    }
    None
}

fn is_separator(ch: Option<char>) -> bool {
    matches!(ch, Some('/' | '\\'))
}

/// `scheme://…` as a cut; for our own marker `scheme://…`, where the scan
/// resumes behind it; `None` when the colon opens no URI.
fn uri_outcome(line: &str, colon: usize) -> Option<Result<Cut, usize>> {
    let rest = line[colon..].strip_prefix("://")?;
    let scheme_start = line[..colon]
        .char_indices()
        .rev()
        .find(|(_, ch)| !is_scheme_char(*ch))
        .map_or(0, |(index, ch)| index + ch.len_utf8());
    let scheme = &line[scheme_start..colon];
    if !scheme.starts_with(|first: char| first.is_ascii_alphabetic()) {
        return None;
    }
    if rest.strip_prefix(ELLIPSIS).is_some_and(ends_a_marker) {
        return Some(Err(colon + "://".len() + ELLIPSIS.len()));
    }
    Some(Ok(Cut {
        begin: scheme_start,
        replacement: format!("{scheme}://{ELLIPSIS}"),
    }))
}

fn is_scheme_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.')
}

/// What may follow a marker's ellipsis: nothing, or the `; ` the file mirror
/// writes between a redacted message and its typed fields, and nothing but
/// those. Any other word after it is text a forged marker would hide.
fn ends_a_marker(after: &str) -> bool {
    after.is_empty()
        || after
            .strip_prefix("; ")
            .is_some_and(|fields| !fields.trim().is_empty() && is_typed_field_list(fields))
}

fn slash_cut(line: &str, slash: usize, previous: Option<char>) -> Option<Cut> {
    let next = line[slash + 1..].chars().next()?;
    if next.is_whitespace() {
        return None;
    }
    if previous.is_some_and(is_word) {
        // `and/or` is not a path; `…123Z/home/x` is.
        return GLUED_ROOTS
            .iter()
            .any(|root| line[slash..].starts_with(root))
            .then(|| Cut::path(slash));
    }
    let begin = if previous == Some(':') {
        remote_root_start(line, slash)
    } else {
        slash
    };
    Some(Cut::path(begin))
}

/// `nas:/export/music` and `user@host:/music`: the host is part of the
/// secret. Where the host starts; a known field before the colon
/// (`path:/run/media/x`) is a key, not a host.
fn remote_root_start(line: &str, slash: usize) -> usize {
    let Some(before) = line[..slash].strip_suffix(':') else {
        return slash;
    };
    let host_start = before
        .char_indices()
        .rev()
        .find(|(_, ch)| !(is_word(*ch) || matches!(ch, '.' | '-' | '@')))
        .map_or(0, |(index, ch)| index + ch.len_utf8());
    let host = &before[host_start..];
    let named = host.starts_with(char::is_alphanumeric);
    if named && !is_known_field(&host.to_ascii_lowercase()) {
        host_start
    } else {
        slash
    }
}

/// `\\server\share`, `\\?\C:\…` and `\\.\pipe`.
fn backslash_cut(line: &str, at: usize) -> Option<Cut> {
    let after = line[at..].strip_prefix("\\\\")?;
    after
        .starts_with(|ch: char| ch.is_alphanumeric() || matches!(ch, '?' | '.'))
        .then(|| Cut::path(at))
}

/// `~/x` and `~bob/x`.
fn tilde_cut(line: &str, at: usize) -> Option<Cut> {
    let rest = &line[at + 1..];
    let name = rest
        .trim_start_matches(|ch: char| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'));
    is_separator(name.chars().next()).then(|| Cut::path(at))
}

/// `$HOME/x`, `${XDG_MUSIC_DIR}/x`, and any other `$NAME/`: a variable in
/// front of a path. Our own `$HOME/…` marker at the end of its line, and the
/// prefix length of `$IP/24`, are left alone.
fn dollar_cut(line: &str, at: usize) -> Option<Cut> {
    let rest = &line[at + 1..];
    let (name, after, braced) = if let Some(braced) = rest.strip_prefix('{') {
        let (close, _) = braced
            .char_indices()
            .take(MAX_BRACED_NAME + 1)
            .find(|(_, ch)| *ch == '}')?;
        (&braced[..close], &braced[close + 1..], true)
    } else {
        let length = rest.len()
            - rest
                .trim_start_matches(|ch: char| ch.is_ascii_alphanumeric() || ch == '_')
                .len();
        (&rest[..length], &rest[length..], false)
    };
    if name.is_empty() || !after.starts_with(['/', '\\']) {
        return None;
    }
    if !braced && is_inert_marker_path(&line[at..]) {
        return None;
    }
    Some(Cut::path(at))
}

/// `$HOME/…` and `$XDG_MUSIC_DIR/…` as the redactor writes them, `$IP/24`.
fn is_inert_marker_path(text: &str) -> bool {
    ["$HOME/", "$XDG_MUSIC_DIR/"].iter().any(|marker| {
        text.strip_prefix(marker)
            .and_then(|after| after.strip_prefix(ELLIPSIS))
            .is_some_and(ends_a_marker)
    }) || text.strip_prefix("$IP/").is_some_and(|after| {
        let digits = after.chars().take_while(char::is_ascii_digit).count();
        (1..=MAX_PREFIX_DIGITS).contains(&digits)
            && !after[digits..].starts_with(char::is_alphanumeric)
    })
}

/// `%USERPROFILE%\Music`.
fn percent_cut(line: &str, at: usize) -> Option<Cut> {
    let rest = &line[at + 1..];
    let length = rest
        .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .unwrap_or(rest.len());
    let name = &rest[..length];
    (name.starts_with(|first: char| first.is_ascii_alphabetic())
        && rest[length..].starts_with('%')
        && rest[length + 1..].starts_with(['/', '\\']))
    .then(|| Cut::path(at))
}

/// `C:\Users` and `c:/Users`.
fn drive_cut(line: &str, at: usize) -> Option<Cut> {
    let mut rest = line[at..].chars();
    let _letter = rest.next();
    (rest.next() == Some(':') && is_separator(rest.next())).then(|| Cut::path(at))
}
