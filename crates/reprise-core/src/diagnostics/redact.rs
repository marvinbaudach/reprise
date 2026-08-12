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
            .rfind(|ch: char| !is_scheme_char(ch))
            .map_or(0, |index| {
                index + input[index..].chars().next().unwrap().len_utf8()
            });
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
        cursor = span_end(input, start + prefix.len(), true);
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
        cursor = span_end(input, start + 1, true);
    }
    output
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

fn redact_sensitive_token(token: &str) -> String {
    let Some(separator) = token.find(['=', ':']) else {
        return token.to_string();
    };
    let key = token[..separator]
        .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .to_ascii_lowercase();
    let separator_char = token[separator..].chars().next().unwrap();
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

pub fn is_safe_structured_field(key: &str) -> bool {
    matches!(
        key,
        "track_id"
            | "episode_id"
            | "playlist_id"
            | "run_id"
            | "job_id"
            | "position"
            | "count"
            | "attempt"
            | "retry_count"
    )
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
            if looks_like_filename(trimmed) {
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
