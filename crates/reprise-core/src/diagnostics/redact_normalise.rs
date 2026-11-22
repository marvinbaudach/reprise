//! One reading of the text before any pass looks at it.
//!
//! A look-alike of a structural character is a bypass to a parser and the
//! same character to a reader, so it is written as that character first:
//! fullwidth forms (`／`, `＝`, `：`, `ｐａｓｓｗｏｒｄ`), the ideographic full
//! stop between digits, the slash look-alikes, a percent-encoded `/`, `\` or
//! `:`, and the characters that have no width at all.

/// Characters that take no space and only break a word apart.
const INVISIBLE: [char; 8] = [
    '\u{200b}', '\u{200c}', '\u{200d}', '\u{2060}', '\u{feff}', '\u{ad}', '\u{200e}', '\u{200f}',
];

/// Look-alikes of the forward and the backward slash.
const SLASH_LOOKALIKES: [char; 4] = ['\u{2044}', '\u{2215}', '\u{29f8}', '\u{2571}'];
const BACKSLASH_LOOKALIKES: [char; 2] = ['\u{fe68}', '\u{29f5}'];

const IDEOGRAPHIC_STOPS: [char; 2] = ['\u{3002}', '\u{ff61}'];

const FULLWIDTH_FIRST: u32 = 0xff01;
const FULLWIDTH_LAST: u32 = 0xff5e;
const FULLWIDTH_OFFSET: u32 = 0xfee0;

pub(super) fn normalised(text: &str) -> String {
    let characters: Vec<char> = text.chars().filter(|ch| !INVISIBLE.contains(ch)).collect();
    let mut mapped = String::with_capacity(text.len());
    for (index, &ch) in characters.iter().enumerate() {
        let code = ch as u32;
        mapped.push(if (FULLWIDTH_FIRST..=FULLWIDTH_LAST).contains(&code) {
            char::from_u32(code - FULLWIDTH_OFFSET).unwrap_or(ch)
        } else if SLASH_LOOKALIKES.contains(&ch) {
            '/'
        } else if BACKSLASH_LOOKALIKES.contains(&ch) {
            '\\'
        } else if IDEOGRAPHIC_STOPS.contains(&ch) && is_between_digits(&characters, index) {
            '.'
        } else {
            ch
        });
    }
    decode_structural_percents(&mapped)
}

fn is_between_digits(characters: &[char], index: usize) -> bool {
    let digit = |at: Option<&char>| at.is_some_and(char::is_ascii_digit);
    digit(
        index
            .checked_sub(1)
            .and_then(|before| characters.get(before)),
    ) && digit(characters.get(index + 1))
}

/// `%2F` is `/`, `%5C` is `\`, `%3A` is `:` and `%25` is `%`, whatever the case
/// of the hex. A twice-encoded name is read once per round.
fn decode_structural_percents(text: &str) -> String {
    if !text.contains('%') {
        return text.to_owned();
    }
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('%') {
        output.push_str(&rest[..at]);
        let decoded =
            rest.get(at + 1..at + 3)
                .and_then(|hex| match hex.to_ascii_uppercase().as_str() {
                    "2F" => Some('/'),
                    "5C" => Some('\\'),
                    "3A" => Some(':'),
                    "25" => Some('%'),
                    _ => None,
                });
        match decoded {
            Some(ch) => {
                output.push(ch);
                rest = &rest[at + 3..];
            }
            None => {
                output.push('%');
                rest = &rest[at + 1..];
            }
        }
    }
    output.push_str(rest);
    output
}
