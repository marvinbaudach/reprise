//! Pure helpers for the deterministic, network-free artist avatar: initials
//! plus a hue derived from the artist name. The hue maps onto a small centrally
//! registered CSS palette so recycled rows never install per-widget providers.

use std::hash::{Hash, Hasher};

/// Number of centrally registered avatar gradients.
pub(in crate::ui) const GRADIENT_COUNT: usize = 18;

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

/// Deterministic hue in `0..360` from the (case-folded) name.
pub(in crate::ui) fn hue_for(name: &str) -> u16 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    name.to_lowercase().hash(&mut hasher);
    (hasher.finish() % 360) as u16
}

/// Stable CSS class for the palette entry associated with `name`.
pub(in crate::ui) fn gradient_class(name: &str) -> String {
    let index = usize::from(hue_for(name)) * GRADIENT_COUNT / 360;
    format!("artist-avatar-gradient-{index}")
}

/// A two-stop diagonal gradient CSS value for one palette entry.
pub(in crate::ui) fn gradient_css_for_index(index: usize) -> String {
    let hue = (index % GRADIENT_COUNT) * 360 / GRADIENT_COUNT;
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

    #[test]
    fn gradient_class_is_deterministic_and_bounded() {
        let class = gradient_class("A Day to Remember");
        assert_eq!(class, gradient_class("A Day to Remember"));
        let index = class
            .strip_prefix("artist-avatar-gradient-")
            .unwrap()
            .parse::<usize>()
            .unwrap();
        assert!(index < GRADIENT_COUNT);
    }

    #[test]
    fn initials_never_exceed_two_chars_even_with_expanding_uppercase() {
        // 'ß'.to_uppercase() == "SS" — a single word whose first char expands
        assert_eq!(initials("ßigband").chars().count(), 2);
        // ligature first char in a single word
        assert!(initials("ﬁre").chars().count() <= 2);
    }
}
