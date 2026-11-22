//! The checked-in table of the app's own log message templates, read at run
//! time.
//!
//! `log_templates.txt` holds one line per distinct `(file, template)`:
//! `<workspace-relative file>\t<template>`, the template in canonical form
//! (`LogTemplate::canonical`) with `\\`, `\t`, `\n` and `\r` escaped. It is
//! generated from the sources by the test-only field sweep and kept fresh by a
//! test (`log_templates_tests`), which also blesses it. The file column is the
//! path `file!()` yields in a build from the workspace root, which is the
//! `Metadata::file()` of a call site; a test per crate pins that form.
//!
//! The table is parsed once into an index by file. A malformed line is skipped
//! here and fails the freshness test there, so a damaged table degrades to
//! shown placeholders and never to shown text.

use std::collections::HashMap;
use std::sync::OnceLock;

use super::log_template::{LogTemplate, HOLE};

const TABLE: &str = include_str!("log_templates.txt");

type Index = HashMap<String, Vec<LogTemplate>>;

/// The templates of `file` in table order; empty when the file has none.
pub fn templates_for_file(file: &str) -> &'static [LogTemplate] {
    index().get(file).map_or(&[], Vec::as_slice)
}

/// Whether `file` is one of the app's own sources that log with a template, as
/// opposed to a dependency's file or a file the table does not know.
pub fn is_own_file(file: &str) -> bool {
    index().contains_key(file)
}

/// The static text that stands in for `rendered`, a message logged from
/// `file`: the reduced form of the template that matches it, or [`HOLE`] when
/// none does or the file is unknown. The rendered text is never part of the
/// result.
pub fn reduced_message(file: &str, rendered: &str) -> String {
    reduced_by(templates_for_file(file), rendered)
}

/// Of the templates that match `rendered`, the one with the most literal
/// characters, the first in table order on a tie.
pub(super) fn reduced_by(templates: &[LogTemplate], rendered: &str) -> String {
    let mut best: Option<&LogTemplate> = None;
    for template in templates
        .iter()
        .filter(|template| template.matches(rendered))
    {
        if best.is_none_or(|kept| template.literal_chars() > kept.literal_chars()) {
            best = Some(template);
        }
    }
    best.map_or_else(|| HOLE.to_owned(), LogTemplate::reduced)
}

fn index() -> &'static Index {
    static INDEX: OnceLock<Index> = OnceLock::new();
    INDEX.get_or_init(|| parse_table(TABLE))
}

/// Groups the table's well-formed lines by file; a malformed line is skipped.
pub(super) fn parse_table(text: &str) -> Index {
    let mut index = Index::new();
    for (file, template) in text.split('\n').filter_map(parse_line) {
        index.entry(file.to_owned()).or_default().push(template);
    }
    index
}

/// One table line; `None` for a blank or malformed one.
pub(super) fn parse_line(line: &str) -> Option<(&str, LogTemplate)> {
    let (file, escaped) = line.split_once('\t')?;
    if file.is_empty() {
        return None;
    }
    let template = LogTemplate::parse(&unescape(escaped)?).ok()?;
    Some((file, template))
}

/// Reads back what the table writer escaped; `None` for any other backslash
/// sequence or a trailing backslash.
fn unescape(text: &str) -> Option<String> {
    let mut plain = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            plain.push(ch);
            continue;
        }
        plain.push(match chars.next()? {
            '\\' => '\\',
            't' => '\t',
            'n' => '\n',
            'r' => '\r',
            _ => return None,
        });
    }
    Some(plain)
}
