//! The `key=value` pass of the free-text redaction.
//!
//! A kept key (see `field_policy`) keeps its value only when the value is a
//! number, a plain identifier, or a composite of plain values (`redact_composite`).
//! Any other value is redacted together with the words after it, because a
//! value with spaces would otherwise leak every word after the first. How far
//! that reaches is [`Reach`]: the one rule is that text a redaction swallowed
//! never ends at a token that merely looks like a known field.

use super::field_policy::{KEEP_FIELDS, REDACT_FIELDS};
use super::field_values::{is_number_or_version, is_plain_identifier, MAX_VALUE_BYTES};
use super::redact::is_safe_structured_field;
use super::redact_composite::{is_plain_wrapped, plain_list_span};

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

/// What the user name is replaced with. A kept key may carry it as its value,
/// so that a line the report redacts a second time reads as it did the first.
const USER_MARKER: &str = "$USER";

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
///
/// What ends a swallow depends on what started it. Foreign text can contain
/// anything, including a forged `count=3`, so a swallow that began at a value
/// nobody vetted must not end at a token an attacker can write. A token whose
/// value is already exactly `$REDACTED` can only be a field the structured path
/// has redacted (or a copy of that text): there is nothing left in it to leak,
/// so it is the one thing that may end such a swallow.
#[derive(Clone, Copy, PartialEq)]
enum Reach {
    /// Only the token itself.
    Token,
    /// After a token whose value is already `$REDACTED`: up to the next token
    /// that opens a known `key=` assignment, or the end of the line. The
    /// structured path writes `error=$REDACTED count=3`, one field after the
    /// other, and the typed fields after a redacted one must stay readable.
    /// The reach outlives the kept tokens that end it: after `error=$REDACTED
    /// count=3` a stray word is still not part of any field.
    UntilKnownField,
    /// After a value that was rejected, or that belongs to a key the policy
    /// does not keep (raw foreign text): up to the next token whose value is
    /// already exactly `$REDACTED`, or the end of the line. A token that
    /// merely opens a known key (`count=3`) does not end it.
    UntilRedactedField,
    /// To the end of the line: a secret has no reliable end.
    EndOfLine,
}

/// Redacts every `key=value` whose key or value may not leave the machine.
///
/// A redacted `key=` and a kept key with a value that is not plain both own
/// the words after them, as far as their [`Reach`] goes.
pub(super) fn redact_sensitive_assignments(input: &str) -> String {
    let tokens: Vec<&str> = input.split_inclusive(char::is_whitespace).collect();
    let mut output = String::with_capacity(input.len());
    // The whitespace after the last token that was folded into a redaction:
    // it is written only once something follows on the same line.
    let mut pending_whitespace = "";
    let mut reach = Reach::Token;
    let mut index = 0;
    while let Some(token) = tokens.get(index) {
        let body = token.trim_end_matches(char::is_whitespace);
        let whitespace = &token[body.len()..];
        let swallowed = match reach {
            Reach::Token => false,
            Reach::UntilKnownField => !starts_known_assignment(body),
            Reach::UntilRedactedField => !is_redacted_field(body),
            Reach::EndOfLine => true,
        };
        let mut last_whitespace = whitespace;
        let start = index;
        index += 1;
        if swallowed {
            pending_whitespace = whitespace;
        } else {
            output.push_str(pending_whitespace);
            pending_whitespace = "";
            match redact_assignment(&tokens[start..]) {
                Outcome::Kept(count) => {
                    // A list spans several tokens; the gaps between them are
                    // single spaces, so only the last one can hold a newline.
                    for kept in &tokens[start..start + count] {
                        output.push_str(kept);
                    }
                    index += count - 1;
                    last_whitespace = tokens.get(index - 1).map_or("", |last| {
                        &last[last.trim_end_matches(char::is_whitespace).len()..]
                    });
                    if reach != Reach::UntilKnownField {
                        reach = Reach::Token;
                    }
                }
                Outcome::Redacted(redaction) => {
                    output.push_str(&redaction.text);
                    pending_whitespace = whitespace;
                    reach = redaction.reach;
                }
            }
        }
        if last_whitespace.contains(['\n', '\r']) {
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

enum Outcome {
    /// The token, or the run of this many tokens, is written as it is.
    Kept(usize),
    Redacted(Redaction),
}

fn redact_assignment(tokens: &[&str]) -> Outcome {
    let body = tokens
        .first()
        .map_or("", |token| token.trim_end_matches(char::is_whitespace));
    let Some(Assignment {
        separator,
        separator_char,
        key,
    }) = split_assignment(body)
    else {
        return Outcome::Kept(1);
    };
    let reach = if separator_char == '=' {
        if is_redacted_field(body) {
            return Outcome::Redacted(Redaction {
                text: body.to_owned(),
                reach: Reach::UntilKnownField,
            });
        }
        if let Some(count) = plain_list_span(tokens) {
            return Outcome::Kept(count);
        }
        if is_safe_structured_field(&key)
            && is_plain_value(&body[separator + 1..], &key)
            && !opens_struct_body(tokens)
        {
            return Outcome::Kept(1);
        }
        Reach::UntilRedactedField
    } else if SECRET_KEY_NEEDLES.iter().any(|needle| key.contains(needle)) {
        Reach::EndOfLine
    } else {
        return Outcome::Kept(1);
    };
    Outcome::Redacted(Redaction {
        text: format!("{}{REDACTED}", &body[..=separator]),
        reach,
    })
}

/// `QueueRow { index: 1 }`: a plain name followed by a struct body is not a
/// plain value, the body holds fields that were never checked.
fn opens_struct_body(tokens: &[&str]) -> bool {
    tokens.get(1).is_some_and(|next| next.starts_with('{'))
}

/// Whether a token opens a known assignment, which ends a swallow that began
/// at an already redacted field. Only a well-formed key that the policy knows
/// does: `abc==` (base64 padding) or `a=b` inside swallowed text is part of the
/// value, not a new assignment.
fn starts_known_assignment(body: &str) -> bool {
    let Some(assignment) = split_assignment(body) else {
        return false;
    };
    let key = assignment.key.as_str();
    assignment.separator_char == '='
        && body.len() > assignment.separator + 1
        && is_well_formed_key(key)
        && (is_known_field(key) || is_already_redacted(body, &assignment))
}

/// A key either list of the policy names, as the key pass reads keys
/// (lowercase).
pub(super) fn is_known_field(key: &str) -> bool {
    KEEP_FIELDS.binary_search(&key).is_ok() || REDACT_FIELDS.binary_search(&key).is_ok()
}

fn is_well_formed_key(key: &str) -> bool {
    key.starts_with(|ch: char| ch.is_ascii_lowercase() || ch == '_')
        && key
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '_' | '.'))
}

/// `user_name=$REDACTED`, as `EventFields` writes an unlisted structured
/// field: there is no value left to protect, so the token may end a swallow.
/// The value must be exactly the marker: `$REDACTED more` is two tokens and
/// `x=$REDACTED)` is a value nobody wrote.
fn is_redacted_field(body: &str) -> bool {
    split_assignment(body).is_some_and(|assignment| {
        assignment.separator_char == '='
            && is_well_formed_key(&assignment.key)
            && is_already_redacted(body, &assignment)
    })
}

fn is_already_redacted(body: &str, assignment: &Assignment) -> bool {
    body[assignment.separator + 1..] == *REDACTED
}

/// A value a kept key may carry in free text: a number, a plain identifier, or
/// a wrapper around one (`Some(Track(5))`).
fn is_plain_value(value: &str, key: &str) -> bool {
    if value.len() > MAX_VALUE_BYTES {
        return false;
    }
    let value = trim_value(value);
    if value == USER_MARKER {
        return true;
    }
    is_number_or_version(value, key) || is_plain_identifier(value) || is_plain_wrapped(value)
}

/// The value without the punctuation that wraps it in running text. A closing
/// parenthesis is dropped only when no opening one in the value matches it, so
/// `Some(1.5)` stays whole and `(elapsed_ms=1.5)` loses only the last `)`.
///
/// One pass from the back with a running count of the unmatched closers: the
/// parentheses are counted once, so a value that is nothing but `)` costs as
/// much as its length and no more.
pub(super) fn trim_value(value: &str) -> &str {
    let opens = value.matches('(').count();
    let mut closes = value.matches(')').count();
    let mut end = value.len();
    loop {
        end = value[..end].trim_end_matches(TRAILING_PUNCTUATION).len();
        if value[..end].ends_with(')') && closes > opens {
            end -= 1;
            closes -= 1;
        } else {
            return &value[..end];
        }
    }
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
    if assignment.separator_char != '='
        || !is_safe_structured_field(&assignment.key)
        || body.len() - assignment.separator > MAX_VALUE_BYTES
    {
        return false;
    }
    is_number_or_version(
        trim_value(&body[assignment.separator + 1..]),
        &assignment.key,
    )
}
