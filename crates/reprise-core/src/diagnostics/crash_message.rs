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
        if is_source_text(condition) && redact_names(condition, context) == condition {
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
/// FnOnce<A>>::call_once{{vtable.shim}}`, `  10: __libc_start_call_main`. A
/// number, a colon and a symbol of plain characters, each word of which is part
/// of a path or a type (or the only word there is). Its names are replaced, as
/// in a source location, and nothing else is.
pub(super) fn is_symbol_frame(line: &str) -> bool {
    let Some((number, symbol)) = line.trim_start().split_once(':') else {
        return false;
    };
    let symbol = symbol.trim();
    if number.is_empty() || !number.chars().all(|ch| ch.is_ascii_digit()) || symbol.is_empty() {
        return false;
    }
    let words: Vec<&str> = symbol.split_whitespace().collect();
    words.iter().all(|word| {
        word.chars().all(is_symbol_char)
            && (words.len() == 1
                || SYMBOL_KEYWORDS.contains(word)
                || word.contains(['<', '>', '(', ')'])
                || word.contains("::"))
    })
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
                | '='
        )
}
