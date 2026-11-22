//! The user's name and the machine's name, wherever they stand.

use super::redact_assignments::is_known_field;
use super::redact_markers::marker_spans;

/// A name this long (in characters) is replaced wherever its letters occur,
/// inside an identifier or glued to other letters (`marvinbaudach`): a name
/// that long is not a word of ours. A shorter one is replaced only when no
/// letter touches it on either side, so `al` is not cut out of `alpha`.
const SUBSTRING_MIN_CHARS: usize = 5;

/// Replaces `name` (the user name or the machine's host name) with `marker`
/// wherever it stands, whatever its case (`Marvin-PC` for the user `marvin`),
/// in an identifier too (`marvin_laptop`, `marvin2`, `user_marvin`), except
/// as a known field's key (`id=`): replacing that would rename the field, and
/// the line would read differently the second time the report redacts it. A
/// name that is not a known key is replaced in key position too (`marvin=3`).
/// A marker of our own is never read as text: the name `user` does not turn
/// `$USER` into `$$USER`, however many times a line is redacted.
pub(super) fn replace_name(input: &str, name: &str, marker: &str) -> String {
    let spans = marker_spans(input);
    let spare_key = name.is_ascii() && is_known_field(&name.to_ascii_lowercase());
    let substring = name.chars().count() >= SUBSTRING_MIN_CHARS;
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    let mut scan = 0;
    while let Some((start, end)) = next_name_hit(input, name, scan) {
        let left = input[..start].chars().next_back();
        let right = input[end..].chars().next();
        let bounded = substring
            || (left.is_none_or(|ch| !ch.is_alphabetic())
                && right.is_none_or(|ch| !ch.is_alphabetic()));
        let in_marker = spans.iter().any(|&(from, to)| start < to && end > from);
        if bounded && !in_marker && !(spare_key && right == Some('=')) {
            output.push_str(&input[cursor..start]);
            output.push_str(marker);
            cursor = end;
        }
        scan = end.max(start + input[start..].chars().next().map_or(1, char::len_utf8));
    }
    output.push_str(&input[cursor..]);
    output
}

/// A host name written in full (`my-nas.example.org`) is also known by its
/// short name (`my-nas`): both go, the long one first.
pub(super) fn host_names(hostname: &str) -> Vec<&str> {
    let mut names = vec![hostname];
    if let Some((short, _)) = hostname.split_once('.') {
        if !short.is_empty() {
            names.push(short);
        }
    }
    names
}

/// The next place at or after `from` where `name` stands, as (start, end),
/// compared case-insensitively over Unicode (`MÜLLER` is `müller`).
fn next_name_hit(input: &str, name: &str, from: usize) -> Option<(usize, usize)> {
    let first = name.chars().next()?;
    for (offset, ch) in input[from..].char_indices() {
        if !chars_match(ch, first) {
            continue;
        }
        let start = from + offset;
        let mut rest = input[start..].char_indices();
        let mut matched = true;
        let mut end = start;
        for wanted in name.chars() {
            match rest.next() {
                Some((at, got)) if chars_match(got, wanted) => end = start + at + got.len_utf8(),
                _ => {
                    matched = false;
                    break;
                }
            }
        }
        if matched {
            return Some((start, end));
        }
    }
    None
}

fn chars_match(left: char, right: char) -> bool {
    left == right || left.to_lowercase().eq(right.to_lowercase())
}
