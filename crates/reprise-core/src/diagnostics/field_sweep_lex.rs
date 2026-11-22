//! Test support: the token reader behind `field_sweep`.
//!
//! A small lexer instead of a regular expression, because a field list nests
//! parentheses, strings and closures. It drops comments and keeps string
//! contents out of the identifier stream.

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Token {
    Ident(String),
    Str(String),
    Punct(char),
    Other,
}

#[derive(Debug, Clone)]
pub(super) struct Spanned {
    pub(super) token: Token,
    pub(super) line: usize,
}

pub(super) fn tokenize(source: &str) -> Vec<Spanned> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut at = 0;
    let mut line = 1;
    while at < chars.len() {
        let ch = chars[at];
        let start_line = line;
        if ch == '\n' {
            line += 1;
            at += 1;
        } else if ch.is_whitespace() {
            at += 1;
        } else if ch == '/' && chars.get(at + 1) == Some(&'/') {
            while at < chars.len() && chars[at] != '\n' {
                at += 1;
            }
        } else if ch == '/' && chars.get(at + 1) == Some(&'*') {
            let (next, lines) = skip_block_comment(&chars, at);
            at = next;
            line += lines;
        } else if ch == '"' {
            let (text, next) = read_string(&chars, at);
            line += chars[at..next].iter().filter(|c| **c == '\n').count();
            tokens.push(Spanned {
                token: Token::Str(text),
                line: start_line,
            });
            at = next;
        } else if ch == '\'' {
            let (token, next) = read_quote(&chars, at);
            tokens.push(Spanned { token, line });
            at = next;
        } else if ch.is_alphabetic() || ch == '_' {
            let mut end = at;
            while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
                end += 1;
            }
            let word: String = chars[at..end].iter().collect();
            if let Some((text, next)) = prefixed_string(&chars, &word, end) {
                line += chars[at..next].iter().filter(|c| **c == '\n').count();
                tokens.push(Spanned {
                    token: Token::Str(text),
                    line: start_line,
                });
                at = next;
            } else {
                tokens.push(Spanned {
                    token: Token::Ident(word),
                    line,
                });
                at = end;
            }
        } else if ch.is_ascii_digit() {
            while at < chars.len() && (chars[at].is_alphanumeric() || chars[at] == '_') {
                at += 1;
            }
            tokens.push(Spanned {
                token: Token::Other,
                line,
            });
        } else {
            tokens.push(Spanned {
                token: Token::Punct(ch),
                line,
            });
            at += 1;
        }
    }
    tokens
}

fn skip_block_comment(chars: &[char], start: usize) -> (usize, usize) {
    let mut depth = 0usize;
    let mut at = start;
    let mut lines = 0;
    while at < chars.len() {
        if chars[at] == '/' && chars.get(at + 1) == Some(&'*') {
            depth += 1;
            at += 2;
        } else if chars[at] == '*' && chars.get(at + 1) == Some(&'/') {
            depth -= 1;
            at += 2;
            if depth == 0 {
                break;
            }
        } else {
            lines += usize::from(chars[at] == '\n');
            at += 1;
        }
    }
    (at, lines)
}

/// A `"…"` string starting at `start`; returns its text, with every escape
/// read the way Rust reads it, and the index past it. An unterminated string
/// runs to the end of the input, so the index never exceeds `chars.len()`.
///
/// Escapes: `\n`, `\r`, `\t`, `\0`, `\\`, `\"`, `\'`, `\x..`, `\u{…}`, and a
/// backslash before a line break, which drops the break and the spaces, tabs
/// and line breaks that follow (other white space stays, as in rustc). A
/// backslash before anything else keeps that character, and a `\x` or `\u`
/// that names no character is dropped; the source compiled, so neither occurs
/// in practice and the reader never has to fail.
fn read_string(chars: &[char], start: usize) -> (String, usize) {
    let mut text = String::new();
    let mut at = start + 1;
    while at < chars.len() && chars[at] != '"' {
        if chars[at] != '\\' {
            text.push(chars[at]);
            at += 1;
            continue;
        }
        at += 1;
        let Some(&escaped) = chars.get(at) else {
            break;
        };
        at += 1;
        match escaped {
            'n' => text.push('\n'),
            'r' => text.push('\r'),
            't' => text.push('\t'),
            '0' => text.push('\0'),
            'x' => {
                let digits: String = chars[at..].iter().take(2).collect();
                at += digits.chars().count();
                text.extend(u8::from_str_radix(&digits, 16).ok().map(char::from));
            }
            'u' => {
                let (decoded, next) = read_unicode_escape(chars, at);
                text.extend(decoded);
                at = next;
            }
            '\n' | '\r' => {
                while chars
                    .get(at)
                    .is_some_and(|ch| matches!(ch, ' ' | '\t' | '\n' | '\r'))
                {
                    at += 1;
                }
            }
            other => text.push(other),
        }
    }
    (text, (at + 1).min(chars.len()))
}

/// The `{…}` after `\u` at `at`: the character it names and the index past
/// the closing brace. `None` when it names no character.
fn read_unicode_escape(chars: &[char], at: usize) -> (Option<char>, usize) {
    if chars.get(at) != Some(&'{') {
        return (None, at);
    }
    let close = chars[at..]
        .iter()
        .position(|ch| *ch == '}')
        .map_or(chars.len(), |offset| at + offset);
    let digits: String = chars[at + 1..close]
        .iter()
        .filter(|ch| **ch != '_')
        .collect();
    let decoded = u32::from_str_radix(&digits, 16)
        .ok()
        .and_then(char::from_u32);
    (decoded, (close + 1).min(chars.len()))
}

/// `r"…"`, `r#"…"#`, `b"…"`, `br"…"`, `c"…"` after the prefix word `word`.
fn prefixed_string(chars: &[char], word: &str, end: usize) -> Option<(String, usize)> {
    if !matches!(word, "r" | "b" | "br" | "c" | "cr") {
        return None;
    }
    if word.ends_with('r') {
        let hashes = chars[end..].iter().take_while(|ch| **ch == '#').count();
        if chars.get(end + hashes) != Some(&'"') {
            return None;
        }
        let close: Vec<char> = std::iter::once('"')
            .chain(std::iter::repeat_n('#', hashes))
            .collect();
        let body = end + hashes + 1;
        let mut at = body;
        while at < chars.len() && chars[at..].get(..close.len()) != Some(&close[..]) {
            at += 1;
        }
        return Some((
            chars[body..at.min(chars.len())].iter().collect(),
            (at + close.len()).min(chars.len()),
        ));
    }
    (chars.get(end) == Some(&'"')).then(|| read_string(chars, end))
}

/// A char literal or a lifetime starting at the quote at `start`.
pub(super) fn read_quote(chars: &[char], start: usize) -> (Token, usize) {
    let is_char = match chars.get(start + 1) {
        Some('\\') => true,
        Some(_) => chars.get(start + 2) == Some(&'\''),
        None => false,
    };
    if !is_char {
        return (Token::Punct('\''), start + 1);
    }
    let mut at = start + 1;
    while at < chars.len() && chars[at] != '\'' {
        at += if chars[at] == '\\' { 2 } else { 1 };
    }
    (Token::Other, (at + 1).min(chars.len()))
}

/// The index of the bracket that closes the one at `open`.
pub(super) fn matching_close(tokens: &[Spanned], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (at, spanned) in tokens.iter().enumerate().skip(open) {
        match spanned.token {
            Token::Punct('(' | '[' | '{') => depth += 1,
            Token::Punct(')' | ']' | '}') => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
    }
    None
}
