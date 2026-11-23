//! Test support: the message templates of the checked-in template table.
//!
//! `field_sweep` already finds every tracing call in production code and stops
//! at the message. This module takes the message literal it stopped at, the
//! first literal of every `invariant_broken!` call, and renders the table
//! `log_templates.txt` the runtime reads (`log_templates`).
//!
//! The literal arrives unescaped: the lexer reads `\n`, `\u{…}`, a line
//! continuation and the other escapes the way Rust does, and a raw string
//! as written. A format string is split by `LogTemplate::parse`, and one it
//! refuses (an unclosed `{`, a stray `}`) fails the sweep naming the site.
//!
//! What is not read: a message that is not a single string literal in an
//! event macro (the sweep's field reader refuses those shapes itself), and an
//! event without a message, which renders as the empty text and so is shown as
//! `…`.

use std::collections::BTreeSet;

use super::field_sweep::{macro_arguments, top_level_parts};
use super::field_sweep_lex::{Spanned, Token};
use super::log_template::LogTemplate;

/// What `invariant_broken!` puts in front of the caller's text.
pub(super) const INVARIANT_PREFIX: &str = "invariant broken: ";

/// The crates whose events can reach the desktop layers, and so the crates
/// the table covers. `reprise-android-ffi` has its own capture layer, and the
/// CLI, the MCP server and the stem backend run in other processes.
pub(super) const TABLE_CRATES: [&str; 4] = [
    "reprise-core",
    "reprise-view",
    "reprise-platform-linux",
    "reprise-gnome",
];

/// One message template at one place in the sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TemplateSite {
    pub(super) template: LogTemplate,
    /// Relative to the `crates` directory, like `FieldSite::file`.
    pub(super) file: String,
    pub(super) line: usize,
}

/// The template of an event whose message part is `message`.
pub(super) fn event_template(
    name: &str,
    line: usize,
    message: &[Spanned],
) -> Result<TemplateSite, String> {
    let [Spanned {
        token: Token::Str(text),
        ..
    }] = message
    else {
        return Err(format!(
            "{name}:{line}: the message must be one string literal"
        ));
    };
    template_site(name, line, text)
}

/// The template of every `invariant_broken!` call among `tokens`: its first
/// argument, which must be a string literal, behind [`INVARIANT_PREFIX`].
pub(super) fn invariant_templates(
    name: &str,
    tokens: &[Spanned],
) -> Result<Vec<TemplateSite>, String> {
    let mut sites = Vec::new();
    for (at, spanned) in tokens.iter().enumerate() {
        if !matches!(&spanned.token, Token::Ident(word) if word == "invariant_broken") {
            continue;
        }
        let is_call = matches!(
            (tokens.get(at + 1), tokens.get(at + 2)),
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
        if !is_call {
            continue;
        }
        let line = spanned.line;
        let first = macro_arguments(tokens, at + 2)
            .and_then(|arguments| top_level_parts(arguments).into_iter().next());
        let Some(
            [Spanned {
                token: Token::Str(text),
                ..
            }],
        ) = first
        else {
            return Err(format!(
                "{name}:{line}: `invariant_broken!` must start with one string literal, so its \
                 template can be read"
            ));
        };
        sites.push(template_site(
            name,
            line,
            &format!("{INVARIANT_PREFIX}{text}"),
        )?);
    }
    Ok(sites)
}

fn template_site(name: &str, line: usize, format: &str) -> Result<TemplateSite, String> {
    let template = LogTemplate::parse(format)
        .map_err(|error| format!("{name}:{line}: the format string {format:?} has {error}"))?;
    Ok(TemplateSite {
        template,
        file: name.to_owned(),
        line,
    })
}

/// The table file's lines for `sites`: `<path>\t<template>`, one per distinct
/// pair, sorted, each ending in a line feed. The path is workspace-relative,
/// the form `file!()` has, and only the crates in [`TABLE_CRATES`] appear.
pub(super) fn table_text(sites: &[TemplateSite]) -> String {
    let lines: BTreeSet<String> = sites
        .iter()
        .filter(|site| in_table(&site.file))
        .map(|site| {
            format!(
                "crates/{}\t{}",
                site.file,
                escape(&site.template.canonical())
            )
        })
        .collect();
    lines.into_iter().map(|line| line + "\n").collect()
}

fn in_table(file: &str) -> bool {
    TABLE_CRATES.iter().any(|krate| {
        file.strip_prefix(krate)
            .is_some_and(|rest| rest.starts_with('/'))
    })
}

/// Writes the characters that would break a table line as `\\`, `\t`, `\n`
/// and `\r`; the loader reads exactly these back.
fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '\t' => escaped.push_str("\\t"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            other => escaped.push(other),
        }
    }
    escaped
}
