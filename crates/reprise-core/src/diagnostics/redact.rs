use super::field_policy::KEEP_FIELDS;
use super::model::RedactionContext;

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
    redacted = redact_absolute_paths(&redacted);
    redacted = redact_sensitive_assignments(&redacted);
    if let Some(username) = context
        .username
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        redacted = replace_bounded(&redacted, username, "$USER");
    }
    redact_filenames(&redacted)
}

fn redact_uris(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    while cursor < input.len() {
        let Some(relative) = input[cursor..].find("://") else {
            output.push_str(&input[cursor..]);
            break;
        };
        let separator = cursor + relative;
        let scheme_start = input[..separator]
            .char_indices()
            .rev()
            .find(|(_, ch)| !is_scheme_char(*ch))
            .map_or(0, |(index, ch)| index + ch.len_utf8());
        let scheme = &input[scheme_start..separator];
        if !valid_scheme(scheme) {
            output.push_str(&input[cursor..separator + 3]);
            cursor = separator + 3;
            continue;
        }
        output.push_str(&input[cursor..scheme_start]);
        output.push_str(scheme);
        output.push_str("://…");
        cursor = span_end(input, separator + 3, true);
    }
    output
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

fn redact_prefixed_path(input: &str, prefix: &str, replacement: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    while let Some(relative) = input[cursor..].find(prefix) {
        let start = cursor + relative;
        output.push_str(&input[cursor..start]);
        output.push_str(replacement);
        cursor = path_continuation_end(input, span_end(input, start + prefix.len(), true));
    }
    output.push_str(&input[cursor..]);
    output
}

fn redact_absolute_paths(input: &str) -> String {
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
        let boundary = start == 0
            || input[..start]
                .chars()
                .next_back()
                .is_some_and(|ch| !is_word(ch));
        if !boundary {
            output.push_str(&input[cursor..start + 1]);
            cursor = start + 1;
            continue;
        }
        output.push_str(&input[cursor..start]);
        output.push_str(ELLIPSIS);
        cursor = path_continuation_end(input, span_end(input, start + 1, true));
    }
    output
}

/// A path with spaces in a folder name ends at the first space. The words after
/// it that still contain a `/` are further segments of the same path, so they
/// go too: `/media/disk/My Band/Live 2019` loses `Band/Live`. A last segment
/// with no `/` of its own (`2019`) cannot be told from the next word and stays.
fn path_continuation_end(input: &str, mut end: usize) -> usize {
    loop {
        let rest = &input[end..];
        let gap = rest.len() - rest.trim_start_matches([' ', '\t']).len();
        if gap == 0 {
            return end;
        }
        let word_start = end + gap;
        let word_end = span_end(input, word_start, true);
        if !input[word_start..word_end].contains('/') {
            return end;
        }
        end = word_end;
    }
}

fn span_end(input: &str, start: usize, stop_at_whitespace: bool) -> usize {
    input[start..]
        .char_indices()
        .find_map(|(offset, ch)| {
            (matches!(ch, ',' | ';' | '\n' | '\r' | ')' | ']' | '}')
                || (stop_at_whitespace && ch.is_whitespace()))
            .then_some(start + offset)
        })
        .unwrap_or(input.len())
}

fn redact_sensitive_assignments(input: &str) -> String {
    input
        .split_inclusive(char::is_whitespace)
        .map(redact_sensitive_token)
        .collect()
}

/// Where a token splits into a key and a value, as both passes read it.
struct Assignment {
    /// Byte index of the first `=` or `:`.
    separator: usize,
    separator_char: char,
    /// The text before it, reduced to a lowercase identifier.
    key: String,
}

/// The one reading of "key then value" that the key pass and the filename
/// pass share, so a token cannot be a kept `key=value` to one and something
/// else to the other.
fn split_assignment(token: &str) -> Option<Assignment> {
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

fn redact_sensitive_token(token: &str) -> String {
    let Some(Assignment {
        separator,
        separator_char,
        key,
    }) = split_assignment(token)
    else {
        return token.to_string();
    };
    let sensitive = if separator_char == '=' {
        !is_safe_structured_field(&key)
    } else {
        [
            "password",
            "token",
            "secret",
            "credential",
            "username",
            "user_name",
            "api_key",
        ]
        .iter()
        .any(|needle| key.contains(needle))
    };
    if !sensitive {
        return token.to_string();
    }
    let body = token.trim_end_matches(char::is_whitespace);
    let whitespace = &token[body.len()..];
    format!(
        "{}{}$REDACTED{whitespace}",
        &token[..separator],
        &token[separator..separator + 1]
    )
}

/// Whether a structured field's value may be kept in a diagnostics line.
///
/// A lookup in [`KEEP_FIELDS`]: both redaction passes ask this one question,
/// and every key not listed there is redacted. Only an exact, lowercase match
/// counts; an unknown key is never safe. `field_policy` explains how a key
/// earns its place.
pub fn is_safe_structured_field(key: &str) -> bool {
    KEEP_FIELDS.binary_search(&key).is_ok()
}

fn replace_bounded(input: &str, needle: &str, replacement: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    while let Some(relative) = input[cursor..].find(needle) {
        let start = cursor + relative;
        let end = start + needle.len();
        let left = input[..start].chars().next_back();
        let right = input[end..].chars().next();
        let bounded = left.is_none_or(|ch| !is_word(ch)) && right.is_none_or(|ch| !is_word(ch));
        output.push_str(&input[cursor..start]);
        if bounded {
            output.push_str(replacement);
        } else {
            output.push_str(needle);
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

/// `key=12.5` and `key=0.1.331` end in what looks like an extension, but a
/// plain number or a dotted version is not a name. The exemption holds only
/// when the key pass has read the token the same way: it splits at an `=`
/// (not an earlier `:`) and the key is kept. Any other token that carries a
/// number, such as `name:cover.jpg=1.2`, still goes to the filename check.
fn is_kept_number(body: &str) -> bool {
    let Some(assignment) = split_assignment(body) else {
        return false;
    };
    if assignment.separator_char != '=' || !is_safe_structured_field(&assignment.key) {
        return false;
    }
    let value = body[assignment.separator + 1..].trim_matches(|ch: char| {
        matches!(
            ch,
            ',' | ';' | ':' | '(' | ')' | '[' | ']' | '{' | '}' | '"' | '\''
        )
    });
    is_number_or_version(value)
}

/// An integer, a float or a dotted version: digits and single dots, with an
/// optional leading `-`, starting and ending in a digit.
fn is_number_or_version(value: &str) -> bool {
    let digits = value.strip_prefix('-').unwrap_or(value);
    digits.starts_with(|ch: char| ch.is_ascii_digit())
        && digits.ends_with(|ch: char| ch.is_ascii_digit())
        && !digits.contains("..")
        && digits.chars().all(|ch| ch.is_ascii_digit() || ch == '.')
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
