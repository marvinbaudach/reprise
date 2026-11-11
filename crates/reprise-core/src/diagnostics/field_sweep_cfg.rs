//! Test support: which items of a source file are test-only.
//!
//! `field_sweep` must not count the fields of items behind `#[cfg(test)]`, so
//! this module cuts them out of the token stream and reports the out-of-line
//! modules they pull in.

use super::field_sweep_lex::{matching_close, Spanned, Token};

/// An out-of-line module declared behind a test-only `cfg`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TestModule {
    pub(super) name: String,
    /// The `#[path = "…"]` it carries, if any.
    pub(super) path: Option<String>,
    /// The inline modules (`mod a { … }`) the declaration sits in, outermost
    /// first. A file-backed module inside them lives in a directory named
    /// after each. An inline module's own `#[path]` is not followed.
    pub(super) inline: Vec<String>,
}

/// A source file with its test-only items cut out.
pub(super) struct TestSplit {
    pub(super) production: Vec<Spanned>,
    /// The out-of-line modules among the cut items.
    pub(super) test_modules: Vec<TestModule>,
    /// The whole file is test-only.
    pub(super) test_only: bool,
}

/// Drops every item behind a test-only `cfg` and reports the out-of-line
/// modules among them.
///
/// The attributes in front of an item are read as one run, so a `#[path]`
/// before the `#[cfg(test)]` names the module as well as one after it. A
/// test-only inner attribute at the top of the file, `#![cfg(test)]`, makes
/// the whole file test-only.
pub(super) fn without_test_items(tokens: &[Spanned]) -> TestSplit {
    let mut kept = Vec::with_capacity(tokens.len());
    let mut modules = Vec::new();
    let mut at = 0;
    while at < tokens.len() {
        let Some(first_close) = attribute_end(tokens, at) else {
            kept.push(tokens[at].clone());
            at += 1;
            continue;
        };
        if is_inner(tokens, at) {
            if is_test_only_cfg(&tokens[at..first_close]) && brace_depth(&tokens[..at]) == 0 {
                return TestSplit {
                    production: Vec::new(),
                    test_modules: modules,
                    test_only: true,
                };
            }
            kept.extend_from_slice(&tokens[at..first_close]);
            at = first_close;
            continue;
        }
        let mut attributes = vec![(at, first_close)];
        while let Some(next) = attribute_end(tokens, attributes[attributes.len() - 1].1) {
            if is_inner(tokens, attributes[attributes.len() - 1].1) {
                break;
            }
            attributes.push((attributes[attributes.len() - 1].1, next));
        }
        let item = attributes[attributes.len() - 1].1;
        if !attributes
            .iter()
            .any(|(start, end)| is_test_only_cfg(&tokens[*start..*end]))
        {
            kept.extend_from_slice(&tokens[at..item]);
            at = item;
            continue;
        }
        let path_attribute = attributes
            .iter()
            .find_map(|(start, end)| path_value(&tokens[*start..*end]));
        let end = item_end(tokens, item);
        if let Some(name) = out_of_line_module(&tokens[item..end]) {
            modules.push(TestModule {
                name,
                path: path_attribute,
                inline: inline_modules(&tokens[..at]),
            });
        }
        at = end;
    }
    TestSplit {
        production: kept,
        test_modules: modules,
        test_only: false,
    }
}

/// The names of the inline modules, `mod a { … }`, that are still open at the
/// end of `tokens`, outermost first.
fn inline_modules(tokens: &[Spanned]) -> Vec<String> {
    let mut open: Vec<(String, usize)> = Vec::new();
    let mut depth = 0usize;
    for (at, spanned) in tokens.iter().enumerate() {
        match spanned.token {
            Token::Punct('{') => {
                depth += 1;
                let declared = at.checked_sub(2).and_then(|keyword| {
                    match (&tokens[keyword].token, &tokens[at - 1].token) {
                        (Token::Ident(word), Token::Ident(name)) if word == "mod" => {
                            Some(name.clone())
                        }
                        _ => None,
                    }
                });
                if let Some(name) = declared {
                    open.push((name, depth));
                }
            }
            Token::Punct('}') => {
                if open.last().is_some_and(|(_, opened)| *opened == depth) {
                    open.pop();
                }
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }
    open.into_iter().map(|(name, _)| name).collect()
}

/// `#![…]` rather than `#[…]` at `at`.
fn is_inner(tokens: &[Spanned], at: usize) -> bool {
    matches!(
        tokens.get(at + 1).map(|spanned| &spanned.token),
        Some(Token::Punct('!'))
    )
}

/// How many braces are open at the end of `tokens`.
fn brace_depth(tokens: &[Spanned]) -> usize {
    tokens
        .iter()
        .fold(0usize, |depth, spanned| match spanned.token {
            Token::Punct('{') => depth + 1,
            Token::Punct('}') => depth.saturating_sub(1),
            _ => depth,
        })
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

/// The index of the first token after `visibility` at `at`: past `pub` and,
/// if it has one, its `(crate)`, `(super)` or `(in path)` group.
fn past_visibility(tokens: &[Spanned], at: usize) -> usize {
    if !matches!(tokens.get(at).map(|s| &s.token), Some(Token::Ident(word)) if word == "pub") {
        return at;
    }
    match tokens.get(at + 1).map(|s| &s.token) {
        Some(Token::Punct('(')) => matching_close(tokens, at + 1).map_or(at + 1, |close| close + 1),
        _ => at + 1,
    }
}

/// The first word of the item at `start`, after any visibility.
fn first_word(tokens: &[Spanned], start: usize) -> Option<&str> {
    let mut at = start;
    while at < tokens.len() {
        let after = past_visibility(tokens, at);
        if after != at {
            at = after;
            continue;
        }
        if let Token::Ident(word) = &tokens[at].token {
            return Some(word.as_str());
        }
        at += 1;
    }
    None
}

/// Whether the item at `start` declares generics in its header: an item
/// keyword, after the visibility and any `const`, `async`, `unsafe`,
/// `extern "ABI"` or `default`, that takes `<…>`.
fn has_generic_header(tokens: &[Spanned], start: usize) -> bool {
    let mut at = past_visibility(tokens, start);
    while let Some(spanned) = tokens.get(at) {
        match &spanned.token {
            Token::Ident(word)
                if matches!(
                    word.as_str(),
                    "const" | "async" | "unsafe" | "extern" | "default"
                ) => {}
            Token::Str(_) => {}
            Token::Ident(word) => {
                return matches!(
                    word.as_str(),
                    "fn" | "struct" | "enum" | "impl" | "trait" | "type" | "union"
                )
            }
            _ => return false,
        }
        at += 1;
    }
    false
}

/// The index just past the thing that starts at `start`.
///
/// An item (`fn`, `mod`, `impl`, …) ends at its `;` or its first brace group,
/// except that a `use` ends only at its `;`, and that a brace group inside
/// the `<…>` of a generic header (`Foo<{ 1 }>`) is a const argument, not a
/// body. Anything else a `cfg` can sit on (a struct field, an enum variant, an
/// argument, a statement) ends at the next `,` or `;`, or just before the
/// bracket that closes its scope.
fn item_end(tokens: &[Spanned], start: usize) -> usize {
    let first_word = first_word(tokens, start);
    let is_item = first_word.is_some_and(|word| ITEM_KEYWORDS.contains(&word));
    let is_use = first_word == Some("use");
    let generic_header = has_generic_header(tokens, start);
    let mut depth = 0usize;
    let mut angle = 0usize;
    let mut at = start;
    while at < tokens.len() {
        match tokens[at].token {
            Token::Punct('(' | '[') => depth += 1,
            Token::Punct(')' | ']') if depth > 0 => depth -= 1,
            Token::Punct('<') if generic_header && depth == 0 => angle += 1,
            // The `>` of `->` closes nothing.
            Token::Punct('>') if angle > 0 && !follows_minus(tokens, at) => angle -= 1,
            Token::Punct('{') if is_use => depth += 1,
            Token::Punct('}') if is_use && depth > 0 => depth -= 1,
            Token::Punct('{') if depth == 0 && angle > 0 => {
                let Some(close) = matching_close(tokens, at) else {
                    return tokens.len();
                };
                at = close;
            }
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
            Token::Punct(',') if depth == 0 && angle == 0 && !is_item => return at + 1,
            _ => {}
        }
        at += 1;
    }
    tokens.len()
}

fn follows_minus(tokens: &[Spanned], at: usize) -> bool {
    at.checked_sub(1)
        .and_then(|before| tokens.get(before))
        .is_some_and(|spanned| matches!(spanned.token, Token::Punct('-')))
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
