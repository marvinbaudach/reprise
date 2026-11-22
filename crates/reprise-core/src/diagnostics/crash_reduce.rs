//! The crash section's three parts, reduced to what is provably harmless.
//!
//! A panic hook runs with whatever the process holds, so nothing it received
//! is copied into the report as it came. The message is static text or it is
//! `…` (see [`reduce_message`]); a source location is one of three path shapes
//! that name the app's own or a dependency's source, never a directory of the
//! user's ([`reduce_location`]); a backtrace line is a frame whose symbol uses
//! only the characters of a Rust symbol, a location, or `…`
//! ([`reduce_backtrace`]). Only `std` is used: this crate does not link a
//! panic or backtrace library.

use std::any::Any;

use super::log_template::LogTemplate;
use super::log_templates::{best_match, templates_for_file};
use super::panic_phrases::phrases;
use super::HOLE;

/// What the panic message may be shown as when its payload is not text.
const NON_STRING_PAYLOAD: &str = "non-string panic payload";
/// The most bytes of a static payload the report keeps.
const MESSAGE_MAX_BYTES: usize = 1024;
/// The most digits of a line, column or frame number.
const NUMBER_MAX_DIGITS: usize = 10;
/// The longest symbol a frame keeps; a longer one is not a symbol we know.
const SYMBOL_MAX_BYTES: usize = 512;
/// The shortest and longest commit hash a `/rustc/<hash>/` path carries.
const COMMIT_HASH_DIGITS: std::ops::RangeInclusive<usize> = 7..=64;
/// What stands in for a line the grammar does not know.
const UNKNOWN: &str = HOLE;

/// A panic payload, by how the standard library created it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanicPayload {
    /// A `&'static str`: the message of a `panic!` without run-time
    /// arguments. It was compiled into the binary.
    Static(&'static str),
    /// A `String`: a formatted message, whose text may hold any run-time
    /// value.
    Formatted(String),
    /// Any other type.
    Other,
}

impl PanicPayload {
    /// Classifies the payload a panic hook is given.
    #[must_use]
    pub fn of(payload: &(dyn Any + Send)) -> Self {
        if let Some(text) = payload.downcast_ref::<&'static str>() {
            Self::Static(text)
        } else if let Some(text) = payload.downcast_ref::<String>() {
            Self::Formatted(text.clone())
        } else {
            Self::Other
        }
    }
}

/// The message as the crash section shows it, on one line.
///
/// A static payload is kept. A formatted one is matched against the standard
/// library's panic phrases and the log templates of `file`, the panic's source
/// file (`invariant_broken!`'s `debug_assert!` panics with the template of its
/// log line), and shown as the best match with every hole replaced by `…`;
/// when nothing matches it is `…`. Any other payload type has a fixed text.
#[must_use]
pub fn reduce_message(payload: &PanicPayload, file: Option<&str>) -> String {
    let message = match payload {
        PanicPayload::Static(text) => cut(text, MESSAGE_MAX_BYTES).to_owned(),
        PanicPayload::Formatted(text) => {
            let file_templates = file.map_or(&[][..], templates_for_file);
            best_match(phrases().iter().chain(file_templates), text)
                .map_or_else(|| UNKNOWN.to_owned(), LogTemplate::reduced)
        }
        PanicPayload::Other => NON_STRING_PAYLOAD.to_owned(),
    };
    one_line(&message)
}

/// `text` with every line break written as the two characters `\n` or `\r`.
fn one_line(text: &str) -> String {
    text.replace('\r', "\\r").replace('\n', "\\n")
}

/// `text` cut to at most `max` bytes, on a character boundary.
fn cut(text: &str, max: usize) -> &str {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.get(..end).unwrap_or_default()
}

/// A source location, `<path>:<line>:<column>`, in the location grammar: the
/// path is kept when it is one of the shapes of [`reduce_path`] and `…`
/// otherwise; the line and column are numbers or the whole text is `…`.
#[must_use]
pub fn reduce_location(location: &str) -> String {
    let Some((path, numbers)) = split_numbers(location) else {
        return UNKNOWN.to_owned();
    };
    format!(
        "{}:{numbers}",
        reduce_path(path).unwrap_or_else(|| UNKNOWN.to_owned())
    )
}

/// A path and its one or two trailing `:<number>` parts.
fn split_numbers(location: &str) -> Option<(&str, String)> {
    let mut rest = location;
    let mut numbers = Vec::new();
    while numbers.len() < 2 {
        let Some((head, tail)) = rest.rsplit_once(':') else {
            break;
        };
        if !is_number(tail) {
            break;
        }
        numbers.push(tail);
        rest = head;
    }
    if numbers.is_empty() {
        return None;
    }
    numbers.reverse();
    Some((rest, numbers.join(":")))
}

fn is_number(text: &str) -> bool {
    (1..=NUMBER_MAX_DIGITS).contains(&text.len()) && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// A source path in one of the three shapes that carry no user text, or
/// `None`.
///
/// - workspace-relative: `crates/<crate>/src/<dirs>/<file>.rs`, the crate
///   directory in `[a-z0-9_-]+`, every other segment in `[a-z0-9_]+`;
/// - the standard library: `/rustc/<hex>/library/<segments>.rs`, where a
///   `..` segment is allowed because the backtrace prints one;
/// - a dependency's source: any prefix followed by `<name>-<semver>/src/…`,
///   kept as `…/<name>-<semver>/src/<dirs>/<file>.rs`; the prefix is what
///   holds a home directory or a build directory, and it is dropped.
fn reduce_path(path: &str) -> Option<String> {
    workspace_path(path)
        .or_else(|| rustc_path(path))
        .or_else(|| crate_source_path(path))
}

fn workspace_path(path: &str) -> Option<String> {
    let mut segments = path.split('/');
    let (Some("crates"), Some(krate), Some("src")) =
        (segments.next(), segments.next(), segments.next())
    else {
        return None;
    };
    let tail: Vec<&str> = segments.collect();
    (is_crate_dir(krate) && is_source_tail(&tail, false)).then(|| path.to_owned())
}

fn rustc_path(path: &str) -> Option<String> {
    let rest = path.strip_prefix("/rustc/")?;
    let mut segments = rest.split('/');
    let hash = segments.next()?;
    if !COMMIT_HASH_DIGITS.contains(&hash.len()) || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    if segments.next() != Some("library") {
        return None;
    }
    let tail: Vec<&str> = segments.collect();
    is_source_tail(&tail, true).then(|| path.to_owned())
}

fn crate_source_path(path: &str) -> Option<String> {
    let segments: Vec<&str> = path.split('/').collect();
    let at = segments
        .windows(2)
        .position(|pair| matches!(pair, [name, "src"] if is_name_and_version(name)))?;
    let name = segments.get(at)?;
    let tail = segments.get(at + 2..)?;
    is_source_tail(tail, false).then(|| format!("…/{name}/src/{}", tail.join("/")))
}

/// Directory segments and a final `<stem>.rs` file name.
fn is_source_tail(tail: &[&str], allow_parent: bool) -> bool {
    let Some((file, directories)) = tail.split_last() else {
        return false;
    };
    let is_stem = |text: &str| {
        !text.is_empty()
            && text
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    };
    file.strip_suffix(".rs").is_some_and(is_stem)
        && directories
            .iter()
            .all(|dir| is_stem(dir) || (allow_parent && *dir == ".."))
}

fn is_crate_dir(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
        })
}

/// `<name>-<semver>`, split at the first `-` before a digit whose rest is a
/// version, so `md-5-0.10.6` is the crate `md-5` at `0.10.6`.
fn is_name_and_version(segment: &str) -> bool {
    segment.match_indices('-').any(|(at, _)| {
        let (name, version) = (&segment[..at], &segment[at + 1..]);
        !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            && is_semver(version)
    })
}

/// `MAJOR.MINOR.PATCH` with an optional `-pre` and `+build` suffix.
fn is_semver(version: &str) -> bool {
    let (core, suffix) = version.split_at(version.find(['-', '+']).unwrap_or(version.len()));
    let mut parts = core.split('.');
    let numeric = (0..3).all(|_| parts.next().is_some_and(is_number)) && parts.next().is_none();
    let suffix_ok = suffix.is_empty()
        || (suffix.len() > 1
            && suffix
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+')));
    numeric && suffix_ok
}

/// A backtrace, `Backtrace::to_string()`, one reduced line per line.
///
/// A frame `<n>: <symbol>` is kept when the symbol uses only Rust-symbol
/// characters; an `at <location>` line goes through [`reduce_location`]; any
/// other line becomes `…`. Blank lines are dropped.
#[must_use]
pub fn reduce_backtrace(backtrace: &str) -> String {
    backtrace
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(reduce_backtrace_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn reduce_backtrace_line(line: &str) -> String {
    let line = line.trim();
    if let Some(location) = line.strip_prefix("at ") {
        return format!("             at {}", reduce_location(location));
    }
    match line.split_once(": ") {
        Some((number, symbol)) if is_number(number) && is_symbol(symbol) => {
            format!("{number:>4}: {symbol}")
        }
        _ => UNKNOWN.to_owned(),
    }
}

/// Whether `symbol` is made of the characters a demangled Rust symbol uses.
fn is_symbol(symbol: &str) -> bool {
    !symbol.is_empty()
        && symbol.len() <= SYMBOL_MAX_BYTES
        && symbol.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'_' | b':'
                        | b'<'
                        | b'>'
                        | b'{'
                        | b'}'
                        | b'('
                        | b')'
                        | b'['
                        | b']'
                        | b'&'
                        | b'*'
                        | b','
                        | b';'
                        | b'\''
                        | b'!'
                        | b'-'
                        | b' '
                        | b'#'
                        | b'='
                        | b'+'
                )
        })
}
