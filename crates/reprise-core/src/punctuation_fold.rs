//! The one table that maps typographic punctuation onto its ASCII form.
//!
//! Taggers and MusicBrainz disagree on `-` against `–`, `'` against `’` and
//! `"` against `“`. Every comparison that must not care shares this table, so
//! the cover matcher and the group key can never disagree about which
//! characters are the same one.

/// Whether `character` is a hyphen or any dash that stands in for one.
pub(crate) fn is_dash(character: char) -> bool {
    matches!(
        character,
        '-' | '‐' | '‑' | '‒' | '–' | '—' | '―' | '−' | '﹘' | '﹣' | '－'
    )
}

/// Dashes become `-`, apostrophes and primes `'`, double quotes `"` and the
/// ellipsis `...`. Nothing else changes: not case, not whitespace, not
/// diacritics.
///
/// Callers that also run a Unicode compatibility decomposition must fold
/// first: NFKD turns `´` into a space plus a combining acute and `″` into two
/// primes, and the fold would then never see the character it maps.
pub(crate) fn fold_typographic_punctuation(text: &str) -> String {
    let mut folded = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            character if is_dash(character) => folded.push('-'),
            '’' | '‘' | '‚' | '‛' | '′' | 'ʼ' | '`' | '´' => folded.push('\''),
            '“' | '”' | '„' | '‟' | '″' => folded.push('"'),
            '…' => folded.push_str("..."),
            character => folded.push(character),
        }
    }
    folded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_family_lands_on_its_ascii_form() {
        assert_eq!(fold_typographic_punctuation("a‐b–c—d−e"), "a-b-c-d-e");
        assert_eq!(fold_typographic_punctuation("N’ ‘n´ `x"), "N' 'n' 'x");
        assert_eq!(fold_typographic_punctuation("“a” „b″"), "\"a\" \"b\"");
        assert_eq!(fold_typographic_punctuation("wait…"), "wait...");
    }

    #[test]
    fn ascii_and_other_characters_pass_through_unchanged() {
        let text = "Björk - It's \"Fine\" (feat. A/B) 85+92";
        assert_eq!(fold_typographic_punctuation(text), text);
    }
}
