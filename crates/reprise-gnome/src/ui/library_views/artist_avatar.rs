//! Pure helper for compact artist initials used outside the retired Artists
//! grid (for example New Releases release covers).

/// Up to two uppercase initials from the first two words; `"?"` when blank.
pub(in crate::ui) fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    let mut out = String::new();
    match words.as_slice() {
        [] => return "?".to_string(),
        [single] => {
            for ch in single.chars().take(2) {
                out.extend(ch.to_uppercase());
            }
        }
        [first, second, ..] => {
            if let Some(c) = first.chars().next() {
                out.extend(c.to_uppercase());
            }
            if let Some(c) = second.chars().next() {
                out.extend(c.to_uppercase());
            }
        }
    }
    if out.is_empty() {
        "?".to_string()
    } else {
        out.chars().take(2).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_take_up_to_two_words_uppercased() {
        assert_eq!(initials("A Day to Remember"), "AD");
        assert_eq!(initials("Various Artists"), "VA");
    }

    #[test]
    fn initials_single_word_takes_first_char() {
        assert_eq!(initials("Solo"), "SO");
        assert_eq!(initials("x"), "X");
        assert_eq!(initials("   "), "?");
    }

    #[test]
    fn initials_never_exceed_two_chars_even_with_expanding_uppercase() {
        // 'ß'.to_uppercase() == "SS" — a single word whose first char expands
        assert_eq!(initials("ßigband").chars().count(), 2);
        // ligature first char in a single word
        assert!(initials("ﬁre").chars().count() <= 2);
    }
}
