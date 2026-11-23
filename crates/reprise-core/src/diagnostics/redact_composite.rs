//! Composite `Debug` values a kept key may carry as text.
//!
//! `Option<QueueItem>` is `Some(Track(5))`, a `Vec<i64>` is `[1, 2, 3]`, a
//! `Vec<QueueRow>` is `[NowPlaying, PlayNext(0)]`. They survive when every
//! part is a plain value: an integer, a plain identifier, or an identifier
//! wrapping one (`Some(..)`, `Track(..)`). Anything else, a quote, a path, a
//! dot outside the single-token numbers, a struct body, is not plain, and
//! `field_values` redacts the whole value. `field_values` also decides which
//! keys may show a word at all: under a key that carries ids only the integer
//! list qualifies.

use super::field_values::is_plain_identifier;

/// Wrappers around one leaf: `Some(Some(Some(1)))` is three. A fourth is a
/// shape no field of ours has, so it is redacted.
const MAX_WRAPPERS: usize = 3;

/// The longest list that survives. A longer one is redacted whole.
const MAX_LIST_ELEMENTS: usize = 64;

/// `Some(Track(5))`: wrappers around a plain leaf, within the depth bound.
pub(super) fn is_plain_wrapped(value: &str) -> bool {
    is_plain_element(value, MAX_WRAPPERS)
}

/// A whole `Debug` list value: `[]`, `[1, 2, 3]`, `[NowPlaying, PlayNext(0)]`.
/// It opens with `[`, closes with the only `]` and has nothing after it.
pub(super) fn is_plain_list_value(value: &str) -> bool {
    is_list_of(value, |element| is_plain_element(element, MAX_WRAPPERS))
}

/// A whole `Debug` list of integers only: `[]`, `[1, 2, 3]`. What a kept key
/// that carries ids may show of a list, whatever the key's text policy.
pub(super) fn is_integer_list_value(value: &str) -> bool {
    is_list_of(value, is_integer)
}

/// `[a, b, c]` with the elements joined by exactly `, `, each accepted by
/// `element`, within the element bound.
fn is_list_of(value: &str, element: impl Fn(&str) -> bool) -> bool {
    let Some(inner) = value
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    else {
        return false;
    };
    if inner.contains(']') {
        return false;
    }
    inner.is_empty()
        || inner.split(", ").count() <= MAX_LIST_ELEMENTS && inner.split(", ").all(element)
}

/// An integer, a plain identifier, or `Ident(<plain element>)`. Dots are
/// refused here: an element is a variant name or an id, and `Some(Track(1.5))`
/// or `a.flac` would be a file name, which no such field carries.
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
