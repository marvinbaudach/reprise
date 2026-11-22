use super::field_policy::KEEP_FIELDS;
use super::model::RedactionContext;
use super::redact_assignments::{is_kept_number, redact_sensitive_assignments};
use super::redact_markers::ELLIPSIS;
use super::redact_names::{host_names, replace_name};
use super::redact_network::redact_network_identifiers;
use super::redact_normalise::normalised;
use super::redact_paths::redact_paths;

/// The longest message that is redacted. No line of ours comes near it, and
/// the passes read each message more than once: what lies beyond is cut off
/// (and so never leaves the machine) instead of being scanned. The cap holds
/// for what a round writes as well as for what it reads.
const MAX_MESSAGE_BYTES: usize = 8 * 1024;

/// The message up to [`MAX_MESSAGE_BYTES`], cut at a character boundary and
/// marked with an ellipsis.
fn capped(message: String) -> String {
    if message.len() <= MAX_MESSAGE_BYTES {
        return message;
    }
    let mut end = MAX_MESSAGE_BYTES - ELLIPSIS.len();
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{ELLIPSIS}", &message[..end])
}

/// How often the passes are applied to their own output at most. A message
/// that still changes after this many rounds is not a message to explain: see
/// [`redact_log_message`].
const MAX_ROUNDS: usize = 8;

/// What a message that does not settle is replaced with. It is a fixed point
/// of every round, whatever the names of the user and the machine are.
const UNSETTLED: &str = "$REDACTED";

/// Removes values that must not leave the user's machine in a copied report.
///
/// The passes run on their own output until a round changes nothing: the
/// report redacts the lines the file mirror has already redacted, and a pass
/// that cuts a word short can leave a prefix an earlier pass reads differently
/// the second time. What comes back is therefore a fixed point by
/// construction, never by hope: either a text one more round leaves exactly
/// as it is (so a second redaction is a no-op), or, when eight rounds have not
/// settled the text, `$REDACTED` in its place. Nothing that did not settle
/// leaves the function.
pub fn redact_log_message(message: &str, context: &RedactionContext) -> String {
    let mut current = capped(message.to_owned());
    for _ in 0..MAX_ROUNDS {
        let next = redact_round(&current, context);
        if next == current {
            return current;
        }
        current = next;
    }
    UNSETTLED.to_owned()
}

pub(super) fn redact_round(message: &str, context: &RedactionContext) -> String {
    let normalised = normalised(message);
    let mut redacted = redact_paths(&normalised, context);
    redacted = redact_network_identifiers(&redacted);
    // The key pass reads the line before the name is replaced, so a user whose
    // name is a key (`password`, `id`) cannot change what that key means. The
    // report redacts the file's lines again; `id=$USER` is a value that pass
    // accepts as it is (`is_plain_value`), so the second run changes nothing.
    redacted = redact_sensitive_assignments(&redacted);
    capped(redact_filenames(&redact_names(&redacted, context)))
}

/// The user's name and the machine's name, replaced by their markers.
pub(super) fn redact_names(text: &str, context: &RedactionContext) -> String {
    let mut redacted = text.to_owned();
    if let Some(username) = context
        .username
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        redacted = replace_name(&redacted, username, "$USER");
    }
    if let Some(hostname) = context
        .hostname
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        for name in host_names(hostname) {
            redacted = replace_name(&redacted, name, "$HOST");
        }
    }
    redacted
}

/// Whether a structured field's value may be kept in a diagnostics line.
///
/// A lookup in `KEEP_FIELDS`: both redaction passes ask this one question,
/// and every key not listed there is redacted. The lookup is an exact match
/// against lowercase names. `EventFields` passes a field's name as written,
/// so `Count` is unknown there; the free-text pass lowercases the key first
/// (`split_assignment`), so `Count=3` reads as `count=3`. That is harmless:
/// it can only keep a value whose lowercase key is kept anyway. An unknown key
/// is never safe. `field_policy` explains how a key earns its place.
pub fn is_safe_structured_field(key: &str) -> bool {
    KEEP_FIELDS.binary_search(&key).is_ok()
}

fn redact_filenames(input: &str) -> String {
    input
        .split_inclusive(char::is_whitespace)
        .map(|token| {
            let body = token.trim_end_matches(char::is_whitespace);
            let whitespace = &token[body.len()..];
            let trimmed = body.trim_matches(|ch: char| {
                matches!(
                    ch,
                    ',' | ';' | ':' | '(' | ')' | '[' | ']' | '{' | '}' | '"' | '\''
                )
            });
            if looks_like_filename(trimmed)
                && !is_rfc3339_timestamp(trimmed)
                && !is_kept_number(body)
            {
                format!("{ELLIPSIS}{whitespace}")
            } else {
                token.to_string()
            }
        })
        .collect()
}

fn looks_like_filename(token: &str) -> bool {
    let Some((stem, extension)) = token.rsplit_once('.') else {
        return false;
    };
    !stem.is_empty()
        && !stem.chars().all(|ch| ch.is_ascii_digit() || ch == '.')
        && (1..=12).contains(&extension.len())
        && extension.chars().all(|ch| ch.is_ascii_alphanumeric())
}

/// `2026-11-08T12:00:00.123Z`, with an optional fraction and either `Z` or a
/// numeric offset. Every file-mirror line starts with one, and `.123Z` reads
/// as a file extension, so this exact shape is exempt from the filename pass.
/// Nothing looser is: a name that merely resembles a time is still a name.
fn is_rfc3339_timestamp(token: &str) -> bool {
    let bytes = token.as_bytes();
    let digits = |range: std::ops::Range<usize>| {
        bytes
            .get(range)
            .is_some_and(|part| part.iter().all(u8::is_ascii_digit))
    };
    let separators = [(4, b'-'), (7, b'-'), (10, b'T'), (13, b':'), (16, b':')];
    if !(digits(0..4)
        && digits(5..7)
        && digits(8..10)
        && digits(11..13)
        && digits(14..16)
        && digits(17..19))
        || !separators
            .iter()
            .all(|(index, expected)| bytes.get(*index) == Some(expected))
    {
        return false;
    }
    let mut end = 19;
    if bytes.get(end) == Some(&b'.') {
        let fraction = bytes[end + 1..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        if fraction == 0 {
            return false;
        }
        end += 1 + fraction;
    }
    match &bytes[end..] {
        b"Z" => true,
        [b'+' | b'-', ..] => {
            digits(end + 1..end + 3)
                && bytes.get(end + 3) == Some(&b':')
                && digits(end + 4..end + 6)
                && bytes.len() == end + 6
        }
        _ => false,
    }
}
