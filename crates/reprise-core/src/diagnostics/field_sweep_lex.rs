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

/// A `"…"` string starting at `start`; returns its text and the index past it.
fn read_string(chars: &[char], start: usize) -> (String, usize) {
    let mut text = String::new();
    let mut at = start + 1;
    while at < chars.len() && chars[at] != '"' {
        if chars[at] == '\\' {
            at += 1;
        }
        if let Some(ch) = chars.get(at) {
            text.push(*ch);
        }
        at += 1;
    }
    (text, at + 1)
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
            at + close.len(),
        ));
    }
    (chars.get(end) == Some(&'"')).then(|| read_string(chars, end))
}

/// A char literal or a lifetime starting at the quote at `start`.
fn read_quote(chars: &[char], start: usize) -> (Token, usize) {
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
    (Token::Other, at + 1)
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
