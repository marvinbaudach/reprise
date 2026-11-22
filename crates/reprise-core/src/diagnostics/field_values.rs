//! What a typed log field may show, by its kind and its key.
//!
//! One visitor records every field of an event with the kind tracing gave it:
//! a signed or unsigned integer, a float, a bool (the typed `record_*`
//! methods), a string (`record_str`) or a `Debug` rendering (`record_debug`,
//! `record_error`). For an event from the app's own code, a key in
//! `KEEP_FIELDS` keeps a typed number or bool as written. Text, a string or a
//! `Debug` rendering, is kept only when its whole value has a plain shape: a
//! number (several dots only under `version`), a plain identifier, an option
//! or tuple variant of those, or a list of those (`redact_composite` holds the
//! wrapper and list rules). The value is judged exactly as written: no
//! trailing punctuation, no second token, nothing after the closing bracket.
//! Every other key is written `key=$REDACTED`, whatever its value looks like.
//!
//! The policy is enforced by key, not by value type (`field_policy` says so):
//! a kept key whose text value is one plain identifier (`state=Pink`) passes.
//! The shape check is defense in depth, not a proof.

use super::field_policy::KEEP_FIELDS;
use super::redact_composite::{is_plain_list_value, is_plain_wrapped};

/// What stands in for a value that may not be shown.
pub const REDACTED: &str = "$REDACTED";

/// The longest value that is read at all. A longer one is redacted without
/// being scanned: no field of ours writes more than a few hundred bytes, and a
/// scan that is linear per token is still a cost an attacker can multiply.
pub(super) const MAX_VALUE_BYTES: usize = 4096;

/// Duration suffixes of `Debug` for `Duration`, longest first.
const DURATION_UNITS: [&str; 5] = ["ms", "ns", "µs", "us", "s"];

/// An identifier this long made only of hex digits reads as a hash or a
/// token (`deadbeefcafe`), not as a word.
const HASH_LIKE_MIN_LEN: usize = 8;

/// One field value with the kind tracing recorded it under.
#[derive(Clone, Debug, PartialEq)]
pub enum RecordedValue {
    /// `record_i64` and `record_i128`.
    Int(i128),
    /// `record_u64` and `record_u128`.
    Uint(u128),
    /// `record_f64`.
    Float(f64),
    /// `record_bool`.
    Bool(bool),
    /// `record_str`.
    Str(String),
    /// `record_debug` and `record_error`: the `Debug` rendering.
    Debug(String),
}

/// A field of an event: a key compiled into the call site (tracing's
/// `Field::name()` is `'static`, so a run-time string can never become a key)
/// and its value.
#[derive(Clone, Debug, PartialEq)]
pub struct RecordedField {
    pub key: &'static str,
    pub value: RecordedValue,
}

impl RecordedField {
    pub fn new(key: &'static str, value: RecordedValue) -> Self {
        Self { key, value }
    }

    /// `key=value` when the policy shows the value, else `key=$REDACTED`.
    pub fn rendered(&self) -> String {
        render_field(self.key, &self.value)
    }
}

/// `key=value` for a field of an own event, or `key=$REDACTED`.
pub fn render_field(key: &'static str, value: &RecordedValue) -> String {
    match shown_value(key, value) {
        Some(shown) => format!("{key}={shown}"),
        None => format!("{key}={REDACTED}"),
    }
}

/// The text a field may show, `None` when it must be redacted.
fn shown_value(key: &str, value: &RecordedValue) -> Option<String> {
    if KEEP_FIELDS.binary_search(&key).is_err() {
        return None;
    }
    match value {
        RecordedValue::Int(number) => Some(number.to_string()),
        RecordedValue::Uint(number) => Some(number.to_string()),
        // `Debug` keeps `1.0` a float, as the log has always written it.
        RecordedValue::Float(number) => Some(format!("{number:?}")),
        RecordedValue::Bool(flag) => Some(flag.to_string()),
        RecordedValue::Str(text) | RecordedValue::Debug(text) => {
            is_kept_text(key, text).then(|| text.clone())
        }
    }
}

/// Whether the whole text of a kept key's string or `Debug` value has a plain
/// shape.
pub(super) fn is_kept_text(key: &str, value: &str) -> bool {
    value.len() <= MAX_VALUE_BYTES
        && (is_number_or_version(value, key)
            || is_plain_identifier(value)
            || is_plain_wrapped(value)
            || is_plain_list_value(value))
}

/// `^[A-Za-z_][A-Za-z0-9_]*$`, minus anything that reads as a hash.
pub(super) fn is_plain_identifier(value: &str) -> bool {
    value
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        && !(value.len() >= HASH_LIKE_MIN_LEN && value.chars().all(|ch| ch.is_ascii_hexdigit()))
}

/// A number as `Debug` writes it: an integer, a float with one dot, a float
/// with an exponent, a duration (`12.5ms`), optionally negative and optionally
/// inside `Some(..)`. Only the key `version` may carry several dots.
pub(super) fn is_number_or_version(value: &str, key: &str) -> bool {
    let value = value
        .strip_prefix("Some(")
        .and_then(|inner| inner.strip_suffix(')'))
        .unwrap_or(value);
    let unsigned = value.strip_prefix('-').unwrap_or(value);
    if key == "version" && is_dotted_version(unsigned) {
        return true;
    }
    let number = DURATION_UNITS
        .iter()
        .find_map(|unit| unsigned.strip_suffix(unit))
        .unwrap_or(unsigned);
    is_decimal(number) || is_exponent_float(number)
}

fn is_dotted_version(value: &str) -> bool {
    value
        .split('.')
        .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
}

/// Digits with at most one dot, both sides non-empty.
fn is_decimal(value: &str) -> bool {
    let mut parts = value.split('.');
    let digits = |part: Option<&str>| {
        part.is_some_and(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
    };
    let whole = digits(parts.next());
    match (parts.next(), parts.next()) {
        (None, _) => whole,
        (fraction, None) => whole && digits(fraction),
        _ => false,
    }
}

/// `1.5e16`, `1e-3`, `2E+4`.
fn is_exponent_float(value: &str) -> bool {
    let Some((mantissa, exponent)) = value.split_once(['e', 'E']) else {
        return false;
    };
    let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
    is_decimal(mantissa) && !exponent.is_empty() && exponent.chars().all(|ch| ch.is_ascii_digit())
}
