//! The standard library's panic phrases, as templates.
//!
//! A panic whose message has run-time arguments reaches the hook as a
//! `String`. Its text is the std phrase with the values filled in, and the
//! values may be anything: the index, but also the `Err` a library returned or
//! the text of a string being sliced. A phrase is static text, so a message
//! that matches one is shown as the phrase with every hole replaced by `…`
//! and the values never leave the machine.
//!
//! The wording is the toolchain's. A toolchain that rewords a phrase makes the
//! tests in `panic_phrases_tests` fail, each of which triggers the real panic.

use std::sync::OnceLock;

use super::log_template::LogTemplate;

/// The phrases in format-string form: `{}` is where a value goes.
const PHRASES: &[&str] = &[
    "called `Option::unwrap()` on a `None` value",
    "called `Result::unwrap()` on an `Err` value: {}",
    "index out of bounds: the len is {} but the index is {}",
    "range start index {} out of range for slice of length {}",
    "range end index {} out of range for slice of length {}",
    "slice index starts at {} but ends at {}",
    "start byte index {} is out of bounds for string of length {}",
    "end byte index {} is out of bounds for string of length {}",
    "start byte index {} is not a char boundary; it is inside {} (bytes {} of string)",
    "end byte index {} is not a char boundary; it is inside {} (bytes {} of string)",
    "byte range starts at {} but ends at {}",
    "attempt to add with overflow",
    "attempt to subtract with overflow",
    "attempt to multiply with overflow",
    "attempt to divide by zero",
    "attempt to divide with overflow",
    "attempt to calculate the remainder with a divisor of zero",
    "attempt to calculate the remainder with overflow",
    "attempt to negate with overflow",
    "attempt to shift left with overflow",
    "attempt to shift right with overflow",
    "RefCell already borrowed",
    "RefCell already mutably borrowed",
    "assertion `left == right` failed\n  left: {}\n right: {}",
    "assertion `left == right` failed: {}\n  left: {}\n right: {}",
    "assertion `left != right` failed\n  left: {}\n right: {}",
    "assertion `left != right` failed: {}\n  left: {}\n right: {}",
    "internal error: entered unreachable code: {}",
    "not yet implemented: {}",
    "not implemented: {}",
    "removal index (is {}) should be < len (is {})",
    "insertion index (is {}) should be <= len (is {})",
    "swap_remove index (is {}) should be < len (is {})",
    "`at` split index (is {}) should be <= len (is {})",
    "copy_from_slice: source slice length ({}) does not match destination slice length ({})",
    "overflow when adding durations",
    "overflow when subtracting durations",
    "window size must be non-zero",
];

/// The parsed phrase table. A phrase that does not parse is skipped here and
/// fails the table's test there.
pub(super) fn phrases() -> &'static [LogTemplate] {
    static TABLE: OnceLock<Vec<LogTemplate>> = OnceLock::new();
    TABLE.get_or_init(|| {
        PHRASES
            .iter()
            .filter_map(|phrase| LogTemplate::parse(phrase).ok())
            .collect()
    })
}

#[cfg(test)]
pub(super) fn phrase_sources() -> &'static [&'static str] {
    PHRASES
}
