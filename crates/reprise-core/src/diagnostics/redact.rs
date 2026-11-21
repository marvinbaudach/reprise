use super::field_policy::KEEP_FIELDS;
use super::model::RedactionContext;
use super::redact_assignments::{is_kept_number, is_known_field, redact_sensitive_assignments};
use super::redact_network::redact_network_identifiers;
use super::redact_paths::{
    redact_absolute_paths, redact_path_roots, redact_prefixed_path, redact_uris,
};

const ELLIPSIS: &str = "…";

/// Removes values that must not leave the user's machine in a copied report.
pub fn redact_log_message(message: &str, context: &RedactionContext) -> String {
    let mut redacted = redact_uris(message);
    if let Some(music_dir) = context
        .music_dir
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        redacted = redact_prefixed_path(&redacted, music_dir, "$XDG_MUSIC_DIR/…");
    }
    if let Some(home_dir) = context
        .home_dir
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        redacted = redact_prefixed_path(&redacted, home_dir, "$HOME/…");
    }
    redacted = redact_path_roots(&redacted);
    redacted = redact_absolute_paths(&redacted);
    redacted = redact_network_identifiers(&redacted);
    // The key pass reads the line before the name is replaced, so a user whose
    // name is a key (`password`, `id`) cannot change what that key means. The
    // report redacts the file's lines again; `id=$USER` is a value that pass
    // accepts as it is (`is_plain_value`), so the second run changes nothing.
    redacted = redact_sensitive_assignments(&redacted);
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
        redacted = replace_name(&redacted, hostname, "$HOST");
    }
    redact_filenames(&redacted)
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

/// Replaces `name` (the user name or the machine's host name) wherever it
/// stands as a word, whatever its case (`Marvin-PC` for the user `marvin`),
/// except as a known field's key (`id=`): replacing that would rename the
/// field, and the line would read differently the second time the report
/// redacts it. A name that is not a known key is replaced in key position too
/// (`marvin=3`). The marker itself (`$USER`) is never replaced again.
fn replace_name(input: &str, name: &str, marker: &str) -> String {
    let spare_key = name.is_ascii() && is_known_field(&name.to_ascii_lowercase());
    // ASCII case folding keeps every byte offset, so a hit in the folded copy
    // is a hit in the original.
    let folded = input.to_ascii_lowercase();
    let needle = name.to_ascii_lowercase();
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    while let Some(relative) = folded[cursor..].find(&needle) {
        let start = cursor + relative;
        let end = start + needle.len();
        let left = input[..start].chars().next_back();
        let right = input[end..].chars().next();
        let bounded = left.is_none_or(|ch| !is_word(ch)) && right.is_none_or(|ch| !is_word(ch));
        // Never inside one of our own markers (`$HOME`, `$HOST`, `$REDACTED`),
        // whatever name a user or a machine has.
        let in_marker = left == Some('$');
        output.push_str(&input[cursor..start]);
        if bounded && !in_marker && !(spare_key && right == Some('=')) {
            output.push_str(marker);
        } else {
            output.push_str(&input[start..end]);
        }
        cursor = end;
    }
    output.push_str(&input[cursor..]);
    output
}

fn is_word(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
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
