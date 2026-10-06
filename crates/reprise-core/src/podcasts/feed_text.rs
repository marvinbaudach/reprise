//! Text normalization for provider-authored feed fields.

use std::borrow::Cow;

const MAX_REFERENCE_LENGTH: usize = 32;

/// Decodes one layer of XML and HTML4 Latin-1 character references.
#[must_use]
pub fn decode_html_entities(value: &str) -> Cow<'_, str> {
    let mut scan = 0;
    let mut copied = 0;
    let mut decoded = None::<String>;

    while let Some(relative_amp) = value[scan..].find('&') {
        let amp = scan + relative_amp;
        let tail = &value[amp + 1..];
        let search_length = tail.len().min(MAX_REFERENCE_LENGTH + 1);
        let Some(relative_end) = tail.as_bytes()[..search_length]
            .iter()
            .position(|byte| *byte == b';')
        else {
            scan = amp + 1;
            continue;
        };
        let end = amp + 1 + relative_end;
        if let Some(nested_amp) = tail.as_bytes()[..relative_end]
            .iter()
            .position(|byte| *byte == b'&')
        {
            scan = amp + 1 + nested_amp;
            continue;
        }
        let reference = &value[amp + 1..end];
        if !reference
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '#')
        {
            scan = end + 1;
            continue;
        }
        if let Some(character) = decode_reference(reference) {
            let output = decoded.get_or_insert_with(|| String::with_capacity(value.len()));
            output.push_str(&value[copied..amp]);
            output.push(character);
            copied = end + 1;
        }
        scan = end + 1;
    }

    match decoded {
        Some(mut output) => {
            output.push_str(&value[copied..]);
            Cow::Owned(output)
        }
        None => Cow::Borrowed(value),
    }
}

pub(super) fn decode_reference(reference: &str) -> Option<char> {
    if let Some(decimal) = reference.strip_prefix('#') {
        let (digits, radix) = decimal
            .strip_prefix('x')
            .or_else(|| decimal.strip_prefix('X'))
            .map_or((decimal, 10), |hex| (hex, 16));
        let digits_are_valid = !digits.is_empty()
            && digits.chars().all(|digit| match radix {
                10 => digit.is_ascii_digit(),
                16 => digit.is_ascii_hexdigit(),
                _ => false,
            });
        if !digits_are_valid {
            return None;
        }
        return u32::from_str_radix(digits, radix)
            .ok()
            .filter(|codepoint| is_xml_display_codepoint(*codepoint))
            .and_then(char::from_u32);
    }

    Some(match reference {
        "amp" => '&',
        "apos" => '\'',
        "gt" => '>',
        "lt" => '<',
        "quot" => '"',
        "nbsp" => '\u{00a0}',
        "iexcl" => '¡',
        "cent" => '¢',
        "pound" => '£',
        "curren" => '¤',
        "yen" => '¥',
        "brvbar" => '¦',
        "sect" => '§',
        "uml" => '¨',
        "copy" => '©',
        "ordf" => 'ª',
        "laquo" => '«',
        "not" => '¬',
        "shy" => '\u{00ad}',
        "reg" => '®',
        "macr" => '¯',
        "deg" => '°',
        "plusmn" => '±',
        "sup2" => '²',
        "sup3" => '³',
        "acute" => '´',
        "micro" => 'µ',
        "para" => '¶',
        "middot" => '·',
        "cedil" => '¸',
        "sup1" => '¹',
        "ordm" => 'º',
        "raquo" => '»',
        "frac14" => '¼',
        "frac12" => '½',
        "frac34" => '¾',
        "iquest" => '¿',
        "Agrave" => 'À',
        "Aacute" => 'Á',
        "Acirc" => 'Â',
        "Atilde" => 'Ã',
        "Auml" => 'Ä',
        "Aring" => 'Å',
        "AElig" => 'Æ',
        "Ccedil" => 'Ç',
        "Egrave" => 'È',
        "Eacute" => 'É',
        "Ecirc" => 'Ê',
        "Euml" => 'Ë',
        "Igrave" => 'Ì',
        "Iacute" => 'Í',
        "Icirc" => 'Î',
        "Iuml" => 'Ï',
        "ETH" => 'Ð',
        "Ntilde" => 'Ñ',
        "Ograve" => 'Ò',
        "Oacute" => 'Ó',
        "Ocirc" => 'Ô',
        "Otilde" => 'Õ',
        "Ouml" => 'Ö',
        "times" => '×',
        "Oslash" => 'Ø',
        "Ugrave" => 'Ù',
        "Uacute" => 'Ú',
        "Ucirc" => 'Û',
        "Uuml" => 'Ü',
        "Yacute" => 'Ý',
        "THORN" => 'Þ',
        "szlig" => 'ß',
        "agrave" => 'à',
        "aacute" => 'á',
        "acirc" => 'â',
        "atilde" => 'ã',
        "auml" => 'ä',
        "aring" => 'å',
        "aelig" => 'æ',
        "ccedil" => 'ç',
        "egrave" => 'è',
        "eacute" => 'é',
        "ecirc" => 'ê',
        "euml" => 'ë',
        "igrave" => 'ì',
        "iacute" => 'í',
        "icirc" => 'î',
        "iuml" => 'ï',
        "eth" => 'ð',
        "ntilde" => 'ñ',
        "ograve" => 'ò',
        "oacute" => 'ó',
        "ocirc" => 'ô',
        "otilde" => 'õ',
        "ouml" => 'ö',
        "divide" => '÷',
        "oslash" => 'ø',
        "ugrave" => 'ù',
        "uacute" => 'ú',
        "ucirc" => 'û',
        "uuml" => 'ü',
        "yacute" => 'ý',
        "thorn" => 'þ',
        "yuml" => 'ÿ',
        "mdash" => '—',
        "ndash" => '–',
        "hellip" => '…',
        "lsquo" => '‘',
        "rsquo" => '’',
        "ldquo" => '“',
        "rdquo" => '”',
        "bull" => '•',
        "trade" => '™',
        _ => return None,
    })
}

fn is_xml_display_codepoint(codepoint: u32) -> bool {
    matches!(codepoint, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
        && !matches!(codepoint, 0x7F..=0x9F)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn decoded_text_is_borrowed_when_there_is_nothing_to_change() {
        assert!(matches!(
            decode_html_entities("Gülsha & Maja"),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn decoding_is_one_pass_and_keeps_unknown_references() {
        assert_eq!(
            decode_html_entities("&amp;amp; &unknown;"),
            "&amp; &unknown;"
        );
        assert_eq!(decode_html_entities("A & B &amp; C"), "A & B & C");
    }

    #[test]
    fn hostile_invalid_reference_input_is_bounded() {
        let hostile = format!("{};", "&".repeat(100_000));
        let started = Instant::now();

        assert_eq!(decode_html_entities(&hostile), hostile);
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "hostile input took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn xml_illegal_numeric_references_stay_literal() {
        for reference in ["#0", "#x0", "#1", "#x1f", "#127", "#x80", "#x9f"] {
            assert_eq!(decode_reference(reference), None, "{reference}");
            let encoded = format!("&{reference};");
            assert_eq!(decode_html_entities(&encoded), encoded, "{reference}");
        }

        for (reference, expected) in [("#9", '\t'), ("#10", '\n'), ("#13", '\r')] {
            assert_eq!(decode_reference(reference), Some(expected), "{reference}");
        }
    }
}
