//! The panic message of a crash report, and the symbol lines of its backtrace.
//!
//! A panic message is free text, so it is redacted like any other message, and
//! that makes a Rust panic unreadable: `Result::unwrap()` reads as a key and
//! `==` as an assignment. The messages the standard library writes itself, and
//! the source text of a failed `assert!`, hold nothing a user could have typed,
//! so they stay as written; whatever follows them (the `Debug` of an error, a
//! path, a name) is redacted as a message of its own. Only an exact phrase at
//! the start of the message counts: a phrase behind other words is text.
//!
//! The file and the report both apply this, so the message the report reads
//! back is the message the file holds.

use super::model::RedactionContext;
use super::redact::{redact_log_message, redact_names};
use super::redact_assignments::redact_sensitive_assignments;
use super::redact_filenames::has_media_extension;
use super::redact_network::redact_network_identifiers;
use super::redact_secrets::secret_cut;

/// What the standard library and `core` write in front of the values they
/// report. Each is the beginning of the whole message.
const STANDARD_PHRASES: [&str; 7] = [
    "called `Result::unwrap()` on an `Err` value",
    "called `Option::unwrap()` on a `None` value",
    "assertion `left == right` failed",
    "assertion `left != right` failed",
    "internal error: entered unreachable code",
    "not yet implemented",
    "not implemented",
];

/// What `assert!` writes in front of the source text of its condition.
const ASSERTION_PREFIX: &str = "assertion failed: ";

/// The longest condition read as source text.
const MAX_CONDITION_BYTES: usize = 256;

/// Words that join the parts of a type in a symbol (`<F as Trait>`).
const SYMBOL_KEYWORDS: [&str; 9] = [
    "as", "for", "dyn", "impl", "mut", "const", "unsafe", "fn", "extern",
];

/// The panic message as the crash report may carry it.
pub(crate) fn redact_panic_message(message: &str, context: &RedactionContext) -> String {
    for phrase in STANDARD_PHRASES {
        if let Some(rest) = message.strip_prefix(phrase) {
            if rest.is_empty() || rest.starts_with(':') || rest.starts_with(char::is_whitespace) {
                return format!("{phrase}{}", redact_log_message(rest, context));
            }
        }
    }
    if let Some(condition) = message.strip_prefix(ASSERTION_PREFIX) {
        if is_source_text(condition)
            && redact_names(condition, context) == condition
            && carries_no_value(condition)
        {
            return message.to_owned();
        }
        return format!("{ASSERTION_PREFIX}$REDACTED");
    }
    redact_log_message(message, context)
}

/// `index < self.len()`, `!self.queue.is_empty()`, `a == b`: operands and
/// operators, one after the other. A run of plain words (`Pink Floyd`) is text,
/// and so is a lone word: an operand with no operator, call, path or field
/// around it could be anything.
fn is_source_text(condition: &str) -> bool {
    if condition.is_empty()
        || condition.len() > MAX_CONDITION_BYTES
        || !condition.chars().all(is_source_char)
    {
        return false;
    }
    let mut depth: i32 = 0;
    let mut after_operand = false;
    let mut structured = false;
    for token in condition.split_whitespace() {
        let operator = token.chars().all(is_operator_char);
        if operator {
            after_operand = false;
            structured = true;
        } else {
            if after_operand && depth <= 0 {
                return false;
            }
            after_operand = true;
            structured |= token.contains(['.', '(', '[', ':']);
        }
        depth += token.matches(['(', '[']).count() as i32;
        depth -= token.matches([')', ']']).count() as i32;
    }
    depth == 0 && structured
}

/// Source text holds names of the program, never a value: no address, no
/// secret word with something after it, no track or cover file name. The same
/// passes a message goes through decide, so `addr == 192.168.1.5` is not
/// readable because it looks like code.
fn carries_no_value(condition: &str) -> bool {
    // An `=` or a `:` that is no comparison and no path separator makes an
    // assignment, whatever its key (`password=x`, `token: x`).
    let assigns = condition.split_whitespace().any(|token| {
        let without_comparisons = ["==", "!=", "<=", ">=", "=>"]
            .iter()
            .fold(token.to_owned(), |text, operator| {
                text.replace(operator, "")
            });
        without_comparisons.contains('=') || token.replace("::", "").contains(':')
    });
    let operands = condition
        .split_whitespace()
        .filter(|token| !token.chars().all(is_operator_char))
        .collect::<Vec<_>>()
        .join(" ");
    !assigns
        && redact_sensitive_assignments(&operands) == operands
        && carries_no_address_or_secret(condition)
        && !condition.split_whitespace().any(has_media_extension)
}

/// Neither an address nor a secret word with something after it.
fn carries_no_address_or_secret(text: &str) -> bool {
    let tokens: Vec<&str> = text.split_inclusive(char::is_whitespace).collect();
    redact_network_identifiers(text) == text
        && (0..tokens.len()).all(|from| secret_cut(&tokens[from..]).is_none())
}

fn is_operator_char(ch: char) -> bool {
    matches!(
        ch,
        '=' | '!' | '<' | '>' | '&' | '|' | '+' | '-' | '*' | '%' | '^'
    )
}

fn is_source_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric()
        || ch.is_ascii_whitespace()
        || is_operator_char(ch)
        || matches!(ch, '_' | '.' | ':' | '(' | ')' | '[' | ']' | ',')
}

/// Whether `line` is a backtrace symbol: `   7: <F as core::ops::function::
/// FnOnce<A>>::call_once{{vtable.shim}}`. A number, a colon and a symbol of a
/// path (`::`) or a type (`<…>`) in the characters of one: no `=`, no dot
/// outside a `{{…}}` marker, so an address, a `key=value` and a file name are
/// none. The address and secret passes must leave it alone as well. A frame of a plain word (`main`, `clone3`) has no structure to check
/// and goes through the redactor like any line. Each word of a symbol with
/// spaces is part of a path or a type, or a keyword that joins them. Its names
/// are replaced, as in a source location, and nothing else is.
pub(super) fn is_symbol_frame(line: &str) -> bool {
    let Some((number, symbol)) = line.trim_start().split_once(':') else {
        return false;
    };
    let symbol = symbol.trim();
    if number.is_empty() || !number.chars().all(|ch| ch.is_ascii_digit()) {
        return false;
    }
    let structured = symbol.contains("::") || (symbol.starts_with('<') && symbol.contains('>'));
    structured
        && has_only_symbol_chars(symbol)
        && carries_no_address_or_secret(symbol)
        && symbol.split_whitespace().all(|word| {
            SYMBOL_KEYWORDS.contains(&word)
                || word.contains(['<', '>', '(', ')'])
                || word.contains("::")
        })
}

/// Symbol characters, with a dot only inside `{{…}}` (`{{vtable.shim}}`).
fn has_only_symbol_chars(symbol: &str) -> bool {
    let mut braces = 0usize;
    for ch in symbol.chars() {
        match ch {
            '{' => braces += 1,
            '}' => braces = braces.saturating_sub(1),
            '.' if braces == 0 => return false,
            ch if ch.is_whitespace() || is_symbol_char(ch) => {}
            _ => return false,
        }
    }
    true
}

fn is_symbol_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric()
        || matches!(
            ch,
            '_' | ':'
                | '<'
                | '>'
                | ','
                | '&'
                | '*'
                | '('
                | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | '.'
                | '$'
                | '#'
                | '\''
                | '+'
                | '-'
                | ';'
                | '!'
        )
}
