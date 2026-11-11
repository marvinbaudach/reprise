//! Composite `Debug` values a kept key may carry in free text.
//!
//! `Option<QueueItem>` is `Some(Track(5))`, a `Vec<i64>` is `[1, 2, 3]`, a
//! `Vec<QueueRow>` is `[NowPlaying, PlayNext(0)]`. They survive when every
//! part is a plain value: an integer, a plain identifier, or an identifier
//! wrapping one (`Some(..)`, `Track(..)`). Anything else, a quote, a path, a
//! dot outside the single-token numbers, a struct body, is not plain, and the
//! caller redacts the whole value.
//!
//! A list contains `, `, so in whitespace-split text it spans several tokens.
//! [`plain_list_span`] reads them as one value or not at all.

use super::redact::is_safe_structured_field;
use super::redact_assignments::{is_plain_identifier, split_assignment, MAX_VALUE_BYTES};

/// Wrappers around one leaf: `Some(Some(Some(1)))` is three. A fourth is a
/// shape no field of ours has, so it is redacted.
const MAX_WRAPPERS: usize = 3;

/// The longest list that survives. A longer one is redacted whole.
const MAX_LIST_ELEMENTS: usize = 64;

/// What may follow a closing bracket without being part of the value.
const AFTER_LIST: [char; 7] = [',', ';', ':', ']', '}', '"', ')'];

/// `Some(Track(5))`: wrappers around a plain leaf, within the depth bound.
pub(super) fn is_plain_wrapped(value: &str) -> bool {
    is_plain_element(value, MAX_WRAPPERS)
}

/// How many tokens form a kept list assignment that starts at `tokens[0]`:
/// `key=[1, 2, 3]` is three. `None` when the token is not a kept key opening a
/// list, or when the list is unclosed, too long, split by anything but one
/// space, or holds a part that is not plain.
pub(super) fn plain_list_span(tokens: &[&str]) -> Option<usize> {
    let first = tokens.first()?.trim_end_matches(char::is_whitespace);
    let assignment = split_assignment(first)?;
    if assignment.separator_char != '=' || !is_safe_structured_field(&assignment.key) {
        return None;
    }
    let mut joined = String::from(&first[assignment.separator + 1..]);
    if !joined.starts_with('[') {
        return None;
    }
    for (index, token) in tokens.iter().take(MAX_LIST_ELEMENTS).enumerate() {
        if index > 0 {
            joined.push_str(token.trim_end_matches(char::is_whitespace));
        }
        if joined.len() > MAX_VALUE_BYTES {
            return None;
        }
        if joined.contains(']') {
            return is_plain_list(&joined).then_some(index + 1);
        }
        if !token.ends_with(' ') || token.ends_with("  ") {
            return None;
        }
        joined.push(' ');
    }
    None
}

/// `[a, b]` plus optional trailing punctuation, every element plain.
fn is_plain_list(joined: &str) -> bool {
    let Some(close) = joined.find(']') else {
        return false;
    };
    if !joined[close + 1..]
        .chars()
        .all(|ch| AFTER_LIST.contains(&ch))
    {
        return false;
    }
    let inner = &joined[1..close];
    inner.is_empty()
        || inner.split(", ").count() <= MAX_LIST_ELEMENTS
            && inner
                .split(", ")
                .all(|element| is_plain_element(element, MAX_WRAPPERS))
}

/// An integer, a plain identifier, or `Ident(<plain element>)`. Dots are
/// refused here: the filename pass reads `Some(Track(1.5))` as a file name.
fn is_plain_element(value: &str, wrappers_left: usize) -> bool {
    if is_integer(value) || is_plain_identifier(value) {
        return true;
    }
    if wrappers_left == 0 {
        return false;
    }
    let Some((wrapper, rest)) = value.split_once('(') else {
        return false;
    };
    let Some(inner) = rest.strip_suffix(')') else {
        return false;
    };
    is_plain_identifier(wrapper) && is_plain_element(inner, wrappers_left - 1)
}

fn is_integer(value: &str) -> bool {
    let digits = value.strip_prefix('-').unwrap_or(value);
    !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit())
}
