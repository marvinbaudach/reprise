//! The `key=value` pass of the free-text redaction.
//!
//! A kept key (see `field_policy`) keeps its value only when the value is a
//! number or a plain identifier. Any other key is redacted together with
//! everything up to the next `key=` token or the end of the line, because a
//! value with spaces would otherwise leak every word after the first.

use super::field_policy::{KEEP_FIELDS, REDACT_FIELDS};
use super::redact::is_safe_structured_field;

const REDACTED: &str = "$REDACTED";

/// Characters that may follow a value without being part of it: `(elapsed_ms=1.5),`.
/// Nothing is trimmed from the front: a value that opens with a quote or a
/// bracket is not a plain value.
const TRAILING_PUNCTUATION: [char; 6] = [',', ';', ':', ']', '}', '"'];

/// Keys that mark a colon-separated secret: `password: hunter2`.
const SECRET_KEY_NEEDLES: [&str; 7] = [
    "password",
    "token",
    "secret",
    "credential",
    "username",
    "user_name",
    "api_key",
];

/// Duration suffixes of `Debug` for `Duration`, longest first.
const DURATION_UNITS: [&str; 5] = ["ms", "ns", "µs", "us", "s"];

/// An identifier this long made only of hex digits reads as a hash or a
/// token (`deadbeefcafe`), not as a word.
const HASH_LIKE_MIN_LEN: usize = 8;

/// Where a token splits into a key and a value, as both passes read it.
pub(super) struct Assignment {
    /// Byte index of the first `=` or `:`.
    pub(super) separator: usize,
    pub(super) separator_char: char,
    /// The text before it, reduced to a lowercase identifier.
    pub(super) key: String,
}

/// The one reading of "key then value" that the key pass and the filename
/// pass share, so a token cannot be a kept `key=value` to one and something
/// else to the other.
pub(super) fn split_assignment(token: &str) -> Option<Assignment> {
    let (separator, separator_char) = token
        .char_indices()
        .find(|(_, ch)| matches!(ch, '=' | ':'))?;
    let key = token[..separator]
        .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .to_ascii_lowercase();
    Some(Assignment {
        separator,
        separator_char,
        key,
    })
}

/// How far a redaction reaches into the words after it.
#[derive(Clone, Copy, PartialEq)]
enum Reach {
    /// Only the token itself.
    Token,
    /// Up to the next token that opens a known `key=` assignment, or the end
    /// of the line.
    UntilAssignment,
    /// To the end of the line: a secret has no reliable end.
    EndOfLine,
}

/// Redacts every `key=value` whose key or value may not leave the machine.
///
/// A redacted `key=` and a kept key with a value that is not plain both own
/// the words after them, because a value with spaces would otherwise leak
/// every word after the first.
pub(super) fn redact_sensitive_assignments(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    // The whitespace after the last token that was folded into a redaction:
    // it is written only once something follows on the same line.
    let mut pending_whitespace = "";
    let mut reach = Reach::Token;
    for token in input.split_inclusive(char::is_whitespace) {
        let body = token.trim_end_matches(char::is_whitespace);
        let whitespace = &token[body.len()..];
        let swallowed = match reach {
            Reach::Token => false,
            Reach::UntilAssignment => !starts_known_assignment(body),
            Reach::EndOfLine => true,
        };
        if swallowed {
            pending_whitespace = whitespace;
        } else {
            output.push_str(pending_whitespace);
            pending_whitespace = "";
            match redact_assignment(body) {
                Some(redaction) => {
                    output.push_str(&redaction.text);
                    pending_whitespace = whitespace;
                    reach = redaction.reach;
                }
                None => {
                    output.push_str(body);
                    output.push_str(whitespace);
                    reach = Reach::Token;
                }
            }
        }
        if whitespace.contains(['\n', '\r']) {
            reach = Reach::Token;
        }
    }
    output.push_str(pending_whitespace);
    output
}

struct Redaction {
    text: String,
    reach: Reach,
}

fn redact_assignment(body: &str) -> Option<Redaction> {
    let Assignment {
        separator,
        separator_char,
        key,
    } = split_assignment(body)?;
    let reach = if separator_char == '=' {
        if is_safe_structured_field(&key) && is_plain_value(&body[separator + 1..], &key) {
            return None;
        }
        Reach::UntilAssignment
    } else if SECRET_KEY_NEEDLES.iter().any(|needle| key.contains(needle)) {
        Reach::EndOfLine
    } else {
        return None;
    };
    Some(Redaction {
        text: format!("{}{REDACTED}", &body[..=separator]),
        reach,
    })
}

/// Whether a token opens the next assignment, ending a swallowed value. Only
/// a well-formed key that the policy knows does: `abc==` (base64 padding) or
/// `a=b` inside swallowed text is part of the value, not a new assignment.
fn starts_known_assignment(body: &str) -> bool {
    let Some(assignment) = split_assignment(body) else {
        return false;
    };
    let key = assignment.key.as_str();
    assignment.separator_char == '='
        && body.len() > assignment.separator + 1
        && key.starts_with(|ch: char| ch.is_ascii_lowercase() || ch == '_')
        && key
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '_' | '.'))
        && (KEEP_FIELDS.binary_search(&key).is_ok() || REDACT_FIELDS.binary_search(&key).is_ok())
}

/// A value a kept key may carry in free text: a number or a plain identifier.
fn is_plain_value(value: &str, key: &str) -> bool {
    let value = trim_value(value);
    is_number_or_version(value, key) || is_plain_identifier(value)
}

/// The value without the punctuation that wraps it in running text. A closing
/// parenthesis is dropped only when no opening one in the value matches it, so
/// `Some(1.5)` stays whole and `(elapsed_ms=1.5)` loses only the last `)`.
fn trim_value(value: &str) -> &str {
    let mut value = value;
    loop {
        let shorter = value.trim_end_matches(TRAILING_PUNCTUATION);
        let unbalanced_close =
            shorter.ends_with(')') && shorter.matches(')').count() > shorter.matches('(').count();
        let shorter = if unbalanced_close {
            &shorter[..shorter.len() - 1]
        } else {
            shorter
        };
        if shorter.len() == value.len() {
            return value;
        }
        value = shorter;
    }
}

/// `^[A-Za-z_][A-Za-z0-9_]*$`, minus anything that reads as a hash.
fn is_plain_identifier(value: &str) -> bool {
    value
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        && !(value.len() >= HASH_LIKE_MIN_LEN && value.chars().all(|ch| ch.is_ascii_hexdigit()))
}

/// A kept key's value, when it is a number: `key=12.5` and `key=0.1.331` end in
/// what looks like an extension, but a number is not a name. The exemption
/// holds only when the key pass has read the token the same way: it splits at
/// an `=` (not an earlier `:`) and the key is kept. Any other token that
/// carries a number, such as `name:cover.jpg=1.2`, still goes to the filename
/// check.
pub(super) fn is_kept_number(body: &str) -> bool {
    let Some(assignment) = split_assignment(body) else {
        return false;
    };
    if assignment.separator_char != '=' || !is_safe_structured_field(&assignment.key) {
        return false;
    }
    is_number_or_version(
        trim_value(&body[assignment.separator + 1..]),
        &assignment.key,
    )
}

/// A number as `Debug` writes it: an integer, a float with one dot, a float
/// with an exponent, a duration (`12.5ms`), optionally negative and optionally
/// inside `Some(..)`. Only the key `version` may carry several dots.
fn is_number_or_version(value: &str, key: &str) -> bool {
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
