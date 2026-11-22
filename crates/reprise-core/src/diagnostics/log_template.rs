//! A log message's static template: literal pieces and holes.
//!
//! A tracing call site's message is a format string compiled into the binary.
//! Its literal pieces are harmless text; its holes (`{}`, `{name}`, `{:?}`)
//! are where run-time values land. A template matches a rendered message when
//! its literal pieces occur in order and cover the message from start to end,
//! holes matching any text, the empty text included. The output of a match is
//! [`LogTemplate::reduced`], the template's own text with every hole replaced
//! by [`HOLE`]: static text only, never a byte of the rendered message.

use std::fmt;

/// What stands in for a hole, and for a whole message no template matches.
pub const HOLE: &str = "…";

/// A format string split into literal pieces and holes.
///
/// `pieces` always holds one more entry than there are holes: the text before
/// the first hole, the text between each pair of holes (empty when two holes
/// touch) and the text after the last one. A template without holes has a
/// single piece.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogTemplate {
    pieces: Vec<String>,
}

/// Why a format string is not a template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateError {
    /// A `{` that never closes, or one that opens inside a hole.
    UnclosedHole,
    /// A `}` that closes nothing and is not written `}}`.
    StrayClose,
}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnclosedHole => f.write_str("a `{` that is never closed by a `}`"),
            Self::StrayClose => f.write_str("a `}` that closes nothing (write `}}` for a brace)"),
        }
    }
}

impl LogTemplate {
    /// Splits `format` the way `format!` reads it: `{{` and `}}` are literal
    /// braces and every other `{…}` is a hole.
    pub fn parse(format: &str) -> Result<Self, TemplateError> {
        let mut pieces = vec![String::new()];
        let mut chars = format.chars().peekable();
        while let Some(ch) = chars.next() {
            match ch {
                '{' if chars.peek() == Some(&'{') => {
                    chars.next();
                    push_char(&mut pieces, '{');
                }
                '{' => {
                    loop {
                        match chars.next() {
                            Some('}') => break,
                            Some('{') | None => return Err(TemplateError::UnclosedHole),
                            Some(_) => {}
                        }
                    }
                    pieces.push(String::new());
                }
                '}' if chars.peek() == Some(&'}') => {
                    chars.next();
                    push_char(&mut pieces, '}');
                }
                '}' => return Err(TemplateError::StrayClose),
                other => push_char(&mut pieces, other),
            }
        }
        Ok(Self { pieces })
    }

    /// The number of holes.
    pub fn hole_count(&self) -> usize {
        self.pieces.len() - 1
    }

    /// The characters that are not holes: how specific the template is.
    pub fn literal_chars(&self) -> usize {
        self.pieces.iter().map(|piece| piece.chars().count()).sum()
    }

    /// The template's text with every hole written as [`HOLE`].
    pub fn reduced(&self) -> String {
        self.pieces.join(HOLE)
    }

    /// The canonical format string: literal braces doubled, every hole `{}`.
    /// Parsing it gives back an equal template.
    pub fn canonical(&self) -> String {
        let mut text = String::new();
        for (at, piece) in self.pieces.iter().enumerate() {
            if at > 0 {
                text.push_str("{}");
            }
            for ch in piece.chars() {
                match ch {
                    '{' => text.push_str("{{"),
                    '}' => text.push_str("}}"),
                    other => text.push(other),
                }
            }
        }
        text
    }

    /// Whether `rendered` is what this template renders to for some values.
    ///
    /// Anchored at both ends: the first piece is a prefix, the last a suffix,
    /// and the pieces between occur in order in what is left. Taking each
    /// middle piece at its leftmost occurrence leaves the most room for the
    /// rest, so this greedy pass finds a match whenever one exists, without
    /// backtracking.
    pub fn matches(&self, rendered: &str) -> bool {
        let [first, middle @ .., last] = self.pieces.as_slice() else {
            return self.pieces.first().is_some_and(|only| only == rendered);
        };
        let Some(rest) = rendered.strip_prefix(first.as_str()) else {
            return false;
        };
        let Some(mut rest) = rest.strip_suffix(last.as_str()) else {
            return false;
        };
        for piece in middle {
            let Some(found) = rest.find(piece.as_str()) else {
                return false;
            };
            rest = &rest[found + piece.len()..];
        }
        true
    }
}

fn push_char(pieces: &mut [String], ch: char) {
    if let Some(last) = pieces.last_mut() {
        last.push(ch);
    }
}
