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

use super::field_sweep_cfg::{without_test_items, TestModule, TestSplit};
use super::field_sweep_lex::{matching_close, tokenize, Spanned, Token};

/// What to do about a shape the scanner cannot split: a top-level comma inside
/// a turbofish (`collect::<HashMap<_, _>>()`) or a closure's parameter list
/// looks like the end of the field, and `format_args!(…)` is no field at all.
const UNREADABLE_HINT: &str = "wrap the value in parentheses or bind it to a local first, and \
     pass a message as a string literal";

const MACROS: [&str; 6] = ["trace", "debug", "info", "warn", "error", "event"];

/// One field key at one place in the sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FieldSite {
    pub(super) key: String,
    pub(super) file: String,
    pub(super) line: usize,
}

/// What reading one source file yields.
#[derive(Debug, Default)]
pub(super) struct FileSweep {
    pub(super) sites: Vec<FieldSite>,
    /// Out-of-line modules declared behind a test-only `cfg`.
    pub(super) test_modules: Vec<TestModule>,
    /// The whole file is test-only: it opens with `#![cfg(test)]`.
    pub(super) test_only: bool,
}

/// What reading the whole workspace yields.
#[derive(Debug)]
pub(super) struct WorkspaceSweep {
    /// The production files whose sites were read.
    pub(super) files: usize,
    pub(super) sites: Vec<FieldSite>,
}

/// Every field site in production code under `crates/*/src`.
pub(super) fn workspace_sites(crates_dir: &Path) -> Result<WorkspaceSweep, String> {
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
        excluded.extend(test_only_paths(path, &sweep));
        swept.push((path.clone(), sweep.sites));
    }
    let production: Vec<_> = swept
        .into_iter()
        .filter(|(path, _)| !excluded.iter().any(|skip| path.starts_with(skip)))
        .collect();
    Ok(WorkspaceSweep {
        files: production.len(),
        sites: production
            .into_iter()
            .flat_map(|(_, sites)| sites)
            .collect(),
    })
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

/// The files and directories a test-only `mod name;` pulls in, and, for a
/// file that is test-only as a whole, the file and the modules it owns.
///
/// A directory entry is matched with `starts_with`, which compares whole path
/// components. A module declared inside inline modules resolves in the
/// directory those modules name; an inline module's own `#[path]` is not
/// followed.
pub(super) fn test_only_paths(file: &Path, sweep: &FileSweep) -> Vec<PathBuf> {
    let Some(dir) = file.parent() else {
        return Vec::new();
    };
    let stem = file
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let base = if is_module_root(dir, &stem) {
        dir.to_path_buf()
    } else {
        dir.join(&stem)
    };
    let mut paths = Vec::new();
    if sweep.test_only {
        paths.push(file.to_path_buf());
        if base != dir {
            paths.push(base.clone());
        }
    }
    for module in &sweep.test_modules {
        // Inside an inline module, both the module's file and a `#[path]`
        // are relative to a directory per enclosing inline module.
        let anchor = module
            .inline
            .iter()
            .fold(base.clone(), |anchor, segment| anchor.join(segment));
        if let Some(path) = &module.path {
            paths.push(if module.inline.is_empty() {
                dir.join(path)
            } else {
                anchor.join(path)
            });
            continue;
        }
        paths.push(anchor.join(format!("{}.rs", module.name)));
        paths.push(anchor.join(&module.name));
    }
    paths
}

/// Whether `mod x;` in this file resolves beside it rather than in a
/// directory named after it: `mod.rs`, `lib.rs`, `main.rs` and the crate
/// roots of `src/bin/*.rs`.
fn is_module_root(dir: &Path, stem: &str) -> bool {
    let is_bin_root = dir.file_name().is_some_and(|name| name == "bin")
        && dir
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|name| name == "src");
    matches!(stem, "mod" | "lib" | "main") || is_bin_root
}

/// Reads one source file. `name` only labels the sites.
pub(super) fn sweep_source(name: &str, source: &str) -> Result<FileSweep, String> {
    let tokens = tokenize(source);
    let TestSplit {
        production,
        test_modules,
        test_only,
    } = without_test_items(&tokens);
    reject_aliased_imports(name, &production)?;
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
        for key in field_keys(arguments, ident == "event").map_err(|part| {
            format!("{name}:{line}: cannot read the field `{part}`; {UNREADABLE_HINT}")
        })? {
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
        test_only,
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
///
/// `event!` takes its level as the first argument after the options. It is
/// skipped by position, because a level can be a path, a constant or a
/// call, and none of those is told apart from a shorthand field by its
/// tokens.
fn field_keys(arguments: &[Spanned], is_event: bool) -> Result<Vec<String>, String> {
    let mut keys = Vec::new();
    let mut level_pending = is_event;
    for part in top_level_parts(arguments) {
        if is_option(part) {
            continue;
        }
        if level_pending {
            level_pending = false;
            if !assigns(part) {
                continue;
            }
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

/// Whether the part has a `=` outside any bracket that is not part of `==`,
/// `!=`, `<=`, `>=` or `=>`: the mark of `key = value`.
fn assigns(part: &[Spanned]) -> bool {
    let punct = |at: Option<usize>| match at.and_then(|at| part.get(at)) {
        Some(Spanned {
            token: Token::Punct(ch),
            ..
        }) => Some(*ch),
        _ => None,
    };
    let mut depth = 0usize;
    for (at, spanned) in part.iter().enumerate() {
        match spanned.token {
            Token::Punct('(' | '[' | '{') => depth += 1,
            Token::Punct(')' | ']' | '}') => depth = depth.saturating_sub(1),
            Token::Punct('=') if depth == 0 => {
                let before = punct(at.checked_sub(1));
                let after = punct(Some(at + 1));
                if !matches!(before, Some('=' | '!' | '<' | '>'))
                    && !matches!(after, Some('=' | '>'))
                {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
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

/// The scanner finds a macro by its name, so `use tracing::info as i;` would
/// hide every `i!(…)` call from it without a trace. An alias is an error that
/// names the line.
fn reject_aliased_imports(name: &str, tokens: &[Spanned]) -> Result<(), String> {
    let is_word = |at: usize, word: &str| matches!(tokens.get(at).map(|s| &s.token), Some(Token::Ident(text)) if text == word);
    for (at, spanned) in tokens.iter().enumerate() {
        if !is_word(at, "use") {
            continue;
        }
        let end = tokens[at..]
            .iter()
            .position(|s| matches!(s.token, Token::Punct(';')))
            .map_or(tokens.len(), |offset| at + offset);
        let mut path = tokens[at + 1..end].iter().map(|s| &s.token).peekable();
        while matches!(path.peek(), Some(Token::Punct(':'))) {
            path.next();
        }
        if !matches!(path.next(), Some(Token::Ident(root)) if root == "tracing") {
            continue;
        }
        for pair in (at + 1..end).filter(|index| is_word(*index, "as")) {
            let hides = match tokens.get(pair - 1).map(|s| &s.token) {
                Some(Token::Ident(word)) => {
                    MACROS.contains(&word.as_str()) || matches!(word.as_str(), "tracing" | "self")
                }
                _ => false,
            };
            if hides {
                return Err(format!(
                    "{name}:{}: an alias of a `tracing` import hides its macro calls from the \
                     field scan; import the macro by its own name",
                    spanned.line
                ));
            }
        }
    }
    Ok(())
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
