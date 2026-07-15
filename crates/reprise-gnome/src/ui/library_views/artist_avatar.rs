//! Pure helpers for the deterministic, network-free artist avatar: initials
//! plus a hue derived from the artist name (used for a two-stop gradient in the
//! master list, where per-row cover extraction would stall scrolling).

use std::hash::{Hash, Hasher};

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
        out
    }
}

/// Deterministic hue in `0..360` from the (case-folded) name.
pub(in crate::ui) fn hue_for(name: &str) -> u16 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    name.to_lowercase().hash(&mut hasher);
    (hasher.finish() % 360) as u16
}

/// A two-stop diagonal gradient CSS value for the avatar background.
pub(in crate::ui) fn gradient_css(name: &str) -> String {
    let hue = hue_for(name);
    let hue2 = (hue + 40) % 360;
    format!("linear-gradient(135deg, hsl({hue}, 42%, 32%), hsl({hue2}, 46%, 22%))")
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
    fn hue_is_deterministic_and_bounded() {
        assert_eq!(hue_for("Solo"), hue_for("Solo"));
        assert!(hue_for("A Day to Remember") < 360);
    }
}
