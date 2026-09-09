pub(super) fn strip_release_decoration(album: &str) -> Option<String> {
    let album = album.trim();
    let spaced_dash = album.char_indices().rev().find(|(index, dash)| {
        matches!(dash, '-' | '–' | '—')
            && album[..*index]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace)
            && album[*index + dash.len_utf8()..]
                .chars()
                .next()
                .is_some_and(char::is_whitespace)
    });
    if let Some((dash_index, dash)) = spaced_dash {
        let title = &album[..dash_index];
        let suffix = &album[dash_index + dash.len_utf8()..];
        if matches!(suffix.trim().to_ascii_lowercase().as_str(), "single" | "ep") {
            let title = title.trim_end();
            if !title.is_empty() {
                return Some(title.to_owned());
            }
            return None;
        }
    }

    let (opening, closing) = match album.chars().last() {
        Some(')') => ('(', ')'),
        Some(']') => ('[', ']'),
        _ => return None,
    };
    let mut depth = 0;
    let mut opening_index = None;
    for (index, character) in album.char_indices().rev() {
        if character == closing {
            depth += 1;
        } else if character == opening {
            depth -= 1;
            if depth == 0 {
                opening_index = Some(index);
                break;
            }
        }
    }
    let opening_index = opening_index?;
    let title = album[..opening_index].trim_end();
    (!title.is_empty()).then(|| title.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_decoration_stripping_removes_exactly_one_trailing_decoration() {
        for (album, expected) in [
            ("Leave (Get Out) - Single", Some("Leave (Get Out)")),
            ("Number[s] (Deluxe Version)", Some("Number[s]")),
            ("Self Inflicted (Deluxe Edition)", Some("Self Inflicted")),
            ("Evolve [Explicit]", Some("Evolve")),
            ("My Forever Drug - Single", Some("My Forever Drug")),
            ("Album – Single", Some("Album")),
            ("Album — EP", Some("Album")),
            ("X-Single", None),
            ("The Black Crown (2011)", Some("The Black Crown")),
            ("Mixtape (Vol. 1) Part 2)", None),
            ("(What's the Story) Morning Glory?", None),
            ("Genesi[s]", Some("Genesi")),
            ("The Wall", None),
        ] {
            assert_eq!(
                strip_release_decoration(album).as_deref(),
                expected,
                "unexpected decoration stripping for {album:?}"
            );
        }
    }
}
