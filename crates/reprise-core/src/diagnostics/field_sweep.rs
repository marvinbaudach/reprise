//! Test support: finds every structured field key a `tracing` macro emits.
//!
//! The classification check in `field_policy_sweep_tests` needs the set of
//! keys the workspace's production code can put in a log line. This module
//! reads the sources with a small token scanner instead of a regular
//! expression, because a field list nests parentheses, strings and closures.
//!
//! What counts as a field: the `key = expr`, `key = %expr`, `key = ?expr`
//! forms and the shorthand `key`, `%key` and `?key`, in an invocation of
//! `trace!`, `debug!`, `info!`, `warn!`, `error!` or `event!` (bare, or as
//! `tracing::…`). A dotted key such as `restored.browse` is one key. Scanning
//! stops at the first string literal, because everything after the message is
//! a format argument, not a field.
//!
//! What does not count: items behind `#[cfg(test)]` or `#[cfg(all(test, …))]`,
//! files those attributes pull in through `mod name;`, `*_tests.rs` files,
//! `tests.rs`, `tests/` directories and `build.rs`.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const MACROS: [&str; 6] = ["trace", "debug", "info", "warn", "error", "event"];

/// One field key at one place in the sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FieldSite {
    pub(super) key: String,
    pub(super) file: String,
    pub(super) line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Ident(String),
    Str(String),
    Punct(char),
    Other,
}

#[derive(Debug, Clone)]
struct Spanned {
    token: Token,
    line: usize,
}

/// What reading one source file yields.
#[derive(Debug, Default)]
pub(super) struct FileSweep {
    pub(super) sites: Vec<FieldSite>,
    /// Out-of-line modules declared behind a test-only `cfg`: the module name
    /// and the `#[path = "…"]` it carries, if any.
    pub(super) test_modules: Vec<(String, Option<String>)>,
}

/// Every field site in production code under `crates/*/src`.
pub(super) fn workspace_sites(crates_dir: &Path) -> Result<Vec<FieldSite>, String> {
    let mut sources = Vec::new();
    for entry in read_dir(crates_dir)? {
        let src = entry.join("src");
        if src.is_dir() {
            collect_sources(&src, &mut sources)?;
        }
    }
    sources.sort();
    let mut swept = Vec::new();
    let mut excluded = Vec::new();
    for path in &sources {
        let text =
            fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let name = relative(path, crates_dir);
        let sweep = sweep_source(&name, &text)?;
        excluded.extend(test_only_paths(path, &sweep.test_modules));
        swept.push((path.clone(), sweep.sites));
    }
    Ok(swept
        .into_iter()
        .filter(|(path, _)| !excluded.iter().any(|skip| path.starts_with(skip)))
        .flat_map(|(_, sites)| sites)
        .collect())
}

/// The distinct keys among `sites`.
pub(super) fn distinct_keys(sites: &[FieldSite]) -> BTreeSet<&str> {
    sites.iter().map(|site| site.key.as_str()).collect()
}

fn relative(path: &Path, base: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn read_dir(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("{}: {error}", dir.display()))?;
        paths.push(entry.path());
    }
    paths.sort();
    Ok(paths)
}

fn collect_sources(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for path in read_dir(dir)? {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if path.is_dir() {
            if name != "tests" {
                collect_sources(&path, out)?;
            }
        } else if name.ends_with(".rs")
            && !name.ends_with("_tests.rs")
            && name != "tests.rs"
            && name != "build.rs"
        {
            out.push(path);
        }
    }
    Ok(())
}

/// The files and directories a test-only `mod name;` pulls in. A directory
/// entry ends in a separator so `starts_with` matches whole path components.
fn test_only_paths(file: &Path, modules: &[(String, Option<String>)]) -> Vec<PathBuf> {
    let Some(dir) = file.parent() else {
        return Vec::new();
    };
    let stem = file
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let base = if matches!(stem.as_str(), "mod" | "lib" | "main") {
        dir.to_path_buf()
    } else {
        dir.join(&stem)
    };
    let mut paths = Vec::new();
    for (name, path_attribute) in modules {
        if let Some(path) = path_attribute {
            paths.push(dir.join(path));
            continue;
        }
        paths.push(base.join(format!("{name}.rs")));
        paths.push(base.join(name));
    }
    paths
}

/// Reads one source file. `name` only labels the sites.
pub(super) fn sweep_source(name: &str, source: &str) -> Result<FileSweep, String> {
    let tokens = tokenize(source);
    let (production, test_modules) = without_test_items(&tokens);
    let mut sites = Vec::new();
    for index in 0..production.len() {
        let Token::Ident(ident) = &production[index].token else {
            continue;
        };
        if !MACROS.contains(&ident.as_str()) || !is_tracing_call(&production, index) {
            continue;
        }
        let line = production[index].line;
        let Some(arguments) = macro_arguments(&production, index + 2) else {
            continue;
        };
        for key in field_keys(arguments, ident == "event")
            .map_err(|part| format!("{name}:{line}: cannot read the field `{part}`"))?
        {
            sites.push(FieldSite {
                key,
                file: name.to_owned(),
                line,
            });
        }
    }
    Ok(FileSweep {
        sites,
        test_modules,
    })
}

fn is_tracing_call(tokens: &[Spanned], index: usize) -> bool {
    let call = matches!(
        (tokens.get(index + 1), tokens.get(index + 2)),
        (
            Some(Spanned {
                token: Token::Punct('!'),
                ..
            }),
            Some(Spanned {
                token: Token::Punct('(' | '[' | '{'),
                ..
            })
        )
    );
    if !call {
        return false;
    }
    let before = |back: usize| index.checked_sub(back).map(|at| &tokens[at].token);
    match (before(1), before(2)) {
        (Some(Token::Punct(':')), Some(Token::Punct(':'))) => {
            matches!(before(3), Some(Token::Ident(path)) if path == "tracing")
        }
        _ => true,
    }
}

/// The tokens between the brackets that open at `open`.
fn macro_arguments(tokens: &[Spanned], open: usize) -> Option<&[Spanned]> {
    let end = matching_close(tokens, open)?;
    tokens.get(open + 1..end)
}

fn matching_close(tokens: &[Spanned], open: usize) -> Option<usize> {
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

/// Splits on commas outside any bracket.
fn top_level_parts(tokens: &[Spanned]) -> Vec<&[Spanned]> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (at, spanned) in tokens.iter().enumerate() {
        match spanned.token {
            Token::Punct('(' | '[' | '{') => depth += 1,
            Token::Punct(')' | ']' | '}') => depth = depth.saturating_sub(1),
            Token::Punct(',') if depth == 0 => {
                parts.push(&tokens[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    parts.push(&tokens[start..]);
    parts.retain(|part| !part.is_empty());
    parts
}

/// The field keys in a macro's argument list; `Err` carries the text of a
/// part that is neither a field nor a recognised option, so a shape the
/// scanner cannot read fails loudly instead of hiding a key.
fn field_keys(arguments: &[Spanned], is_event: bool) -> Result<Vec<String>, String> {
    let mut keys = Vec::new();
    for part in top_level_parts(arguments) {
        if is_option(part) || (is_event && mentions_level(part)) {
            continue;
        }
        if matches!(part.first().map(|first| &first.token), Some(Token::Str(_)))
            && !is_quoted_key(part)
        {
            break;
        }
        keys.push(field_key(part).ok_or_else(|| describe(part))?);
    }
    Ok(keys)
}

/// `target: …`, `parent: …` and `name: …`.
fn is_option(part: &[Spanned]) -> bool {
    matches!(
        (part.first(), part.get(1), part.get(2)),
        (
            Some(Spanned { token: Token::Ident(option), .. }),
            Some(Spanned { token: Token::Punct(':'), .. }),
            third,
        ) if matches!(option.as_str(), "target" | "parent" | "name")
            && !matches!(third, Some(Spanned { token: Token::Punct(':'), .. }))
    )
}

fn mentions_level(part: &[Spanned]) -> bool {
    part.iter()
        .any(|spanned| matches!(&spanned.token, Token::Ident(ident) if ident == "Level"))
}

fn is_quoted_key(part: &[Spanned]) -> bool {
    matches!(
        (part.get(1), part.get(2)),
        (Some(Spanned { token: Token::Punct('='), .. }), next)
            if !matches!(next, Some(Spanned { token: Token::Punct('='), .. }))
    )
}

fn field_key(part: &[Spanned]) -> Option<String> {
    if let [Spanned {
        token: Token::Str(key),
        ..
    }, Spanned {
        token: Token::Punct('='),
        ..
    }, ..] = part
    {
        return Some(key.clone());
    }
    let sigil = matches!(
        part.first().map(|first| &first.token),
        Some(Token::Punct('%' | '?'))
    );
    let (key, rest) = dotted_path(&part[usize::from(sigil)..])?;
    match rest.first().map(|first| &first.token) {
        None => Some(key),
        Some(Token::Punct('=')) if !sigil => {
            let equality = matches!(rest.get(1).map(|next| &next.token), Some(Token::Punct('=')));
            (!equality).then_some(key)
        }
        _ => None,
    }
}

/// `a`, `a.b`, `a.b.c` — returns the joined path and the tokens after it.
fn dotted_path(tokens: &[Spanned]) -> Option<(String, &[Spanned])> {
    let Some(Spanned {
        token: Token::Ident(first),
        ..
    }) = tokens.first()
    else {
        return None;
    };
    let mut key = first.clone();
    let mut at = 1;
    while let (
        Some(Spanned {
            token: Token::Punct('.'),
            ..
        }),
        Some(Spanned {
            token: Token::Ident(segment),
            ..
        }),
    ) = (tokens.get(at), tokens.get(at + 1))
    {
        key.push('.');
        key.push_str(segment);
        at += 2;
    }
    Some((key, &tokens[at..]))
}

fn describe(part: &[Spanned]) -> String {
    part.iter()
        .map(|spanned| match &spanned.token {
            Token::Ident(text) | Token::Str(text) => text.clone(),
            Token::Punct(ch) => ch.to_string(),
            Token::Other => "…".to_owned(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Drops every item behind a test-only `cfg` and reports the out-of-line
/// modules among them.
fn without_test_items(tokens: &[Spanned]) -> (Vec<Spanned>, Vec<(String, Option<String>)>) {
    let mut kept = Vec::with_capacity(tokens.len());
    let mut modules = Vec::new();
    let mut at = 0;
    while at < tokens.len() {
        let Some(attribute_close) = attribute_end(tokens, at) else {
            kept.push(tokens[at].clone());
            at += 1;
            continue;
        };
        if !is_test_only_cfg(&tokens[at..attribute_close]) {
            kept.extend_from_slice(&tokens[at..attribute_close]);
            at = attribute_close;
            continue;
        }
        let mut item = attribute_close;
        let mut path_attribute = None;
        while let Some(next) = attribute_end(tokens, item) {
            path_attribute = path_attribute.or_else(|| path_value(&tokens[item..next]));
            item = next;
        }
        let end = item_end(tokens, item);
        if let Some(name) = out_of_line_module(&tokens[item..end]) {
            modules.push((name, path_attribute));
        }
        at = end;
    }
    (kept, modules)
}

/// `#[…]` or `#![…]` starting at `at`: the index just past the closing `]`.
fn attribute_end(tokens: &[Spanned], at: usize) -> Option<usize> {
    if !matches!(tokens.get(at)?.token, Token::Punct('#')) {
        return None;
    }
    let open = if matches!(tokens.get(at + 1)?.token, Token::Punct('!')) {
        at + 2
    } else {
        at + 1
    };
    if !matches!(tokens.get(open)?.token, Token::Punct('[')) {
        return None;
    }
    matching_close(tokens, open).map(|close| close + 1)
}

/// `#[cfg(test)]` and `#[cfg(all(test, …))]`.
fn is_test_only_cfg(attribute: &[Spanned]) -> bool {
    let words: Vec<&Token> = attribute.iter().map(|spanned| &spanned.token).collect();
    let is = |token: &Token, ident: &str| matches!(token, Token::Ident(text) if text == ident);
    let Some(cfg) = words.iter().position(|token| is(token, "cfg")) else {
        return false;
    };
    match words.get(cfg + 2) {
        Some(token) if is(token, "test") => true,
        Some(token) if is(token, "all") => {
            let mut depth = 0usize;
            for token in &words[cfg + 3..] {
                match token {
                    Token::Punct('(') => depth += 1,
                    Token::Punct(')') => depth = depth.saturating_sub(1),
                    token if depth == 1 && is(token, "test") => return true,
                    _ => {}
                }
            }
            false
        }
        _ => false,
    }
}

/// The string in `#[path = "…"]`.
fn path_value(attribute: &[Spanned]) -> Option<String> {
    let words: Vec<&Token> = attribute.iter().map(|spanned| &spanned.token).collect();
    match words.as_slice() {
        [Token::Punct('#'), Token::Punct('['), Token::Ident(path), Token::Punct('='), Token::Str(value), ..]
            if path == "path" =>
        {
            Some(value.clone())
        }
        _ => None,
    }
}

/// Words that open an item as opposed to a field, variant, argument or
/// statement.
const ITEM_KEYWORDS: [&str; 15] = [
    "fn",
    "struct",
    "enum",
    "impl",
    "mod",
    "trait",
    "use",
    "const",
    "static",
    "type",
    "extern",
    "unsafe",
    "async",
    "macro_rules",
    "union",
];

/// The index just past the thing that starts at `start`.
///
/// An item (`fn`, `mod`, `impl`, …) ends at its `;` or its first brace group,
/// except that a `use` ends only at its `;`. Anything else a `cfg` can sit on
/// (a struct field, an enum variant, an argument, a statement) ends at the
/// next `,` or `;`, or just before the bracket that closes its scope.
fn item_end(tokens: &[Spanned], start: usize) -> usize {
    let first_word = tokens[start..]
        .iter()
        .find_map(|spanned| match &spanned.token {
            Token::Ident(word) if word != "pub" => Some(word.as_str()),
            _ => None,
        });
    let is_item = first_word.is_some_and(|word| ITEM_KEYWORDS.contains(&word));
    let is_use = first_word == Some("use");
    let mut depth = 0usize;
    let mut at = start;
    while at < tokens.len() {
        match tokens[at].token {
            Token::Punct('(' | '[') => depth += 1,
            Token::Punct(')' | ']') if depth > 0 => depth -= 1,
            Token::Punct('{') if is_use => depth += 1,
            Token::Punct('}') if is_use && depth > 0 => depth -= 1,
            Token::Punct('{') if depth == 0 => {
                let Some(close) = matching_close(tokens, at) else {
                    return tokens.len();
                };
                // A brace group ends an item, and also a macro call such as
                // `glib::wrapper! { … }`; an expression or field carries on.
                let continues = matches!(
                    tokens.get(close + 1).map(|next| &next.token),
                    Some(Token::Punct(',' | ';' | '.' | '?'))
                );
                if is_item || !continues {
                    return close + 1;
                }
                at = close;
            }
            Token::Punct(')' | ']' | '}') => return at,
            Token::Punct(';') if depth == 0 => return at + 1,
            Token::Punct(',') if depth == 0 && !is_item => return at + 1,
            _ => {}
        }
        at += 1;
    }
    tokens.len()
}

/// `mod name ;` (with any visibility) as the name, else `None`.
fn out_of_line_module(item: &[Spanned]) -> Option<String> {
    let words: Vec<&Token> = item.iter().map(|spanned| &spanned.token).collect();
    let module = words
        .iter()
        .position(|token| matches!(token, Token::Ident(text) if text == "mod"))?;
    match (words.get(module + 1), words.get(module + 2)) {
        (Some(Token::Ident(name)), Some(Token::Punct(';'))) => Some(name.clone()),
        _ => None,
    }
}

fn tokenize(source: &str) -> Vec<Spanned> {
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
