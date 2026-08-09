//! Which tracks the release lookup treats as one album.
//!
//! Grouping is by album artist and album title, and a multi-disc set whose
//! discs carry different titles — "Album (Disc 1)" against "Album [CD2]" —
//! would otherwise be one group, one search and one chosen release per disc.
//! No disc holds the full tracklist, so each scores worse than the set does as
//! a whole, and the discs can end up on different releases.
//!
//! The key drops a trailing disc marker so those discs meet again. It is a
//! runtime key only: nothing here is written back to a tag.

use crate::library::group_key::normalize_group_key;

/// The words a disc marker is spelled with. Anything else — "Volume", "Part",
/// "Teil" — names a different release often enough that folding it would be a
/// guess, and a wrong fold is worse than the split it repairs.
const DISC_WORDS: [&str; 3] = ["disc", "disk", "cd"];

/// What may stand between the title and its marker. The trailing run of these
/// goes with the marker: "Album," and "Album -" are not titles.
const DISC_SEPARATORS: [char; 5] = ['(', '[', ',', '-', '\u{2013}'];

/// Joins the two normalized halves with a control character no tag carries, so
/// an artist ending in a space cannot run into an album starting with one.
pub(super) fn album_group_key(album_artist: &str, album: &str) -> String {
    format!(
        "{}\u{1}{}",
        normalize_group_key(album_artist),
        normalize_group_key(album_title_without_disc(album))
    )
}

/// The album title without a trailing disc marker, or the title unchanged.
///
/// Deliberately narrow: only at the end, only with a number, and never down to
/// nothing. A record legitimately called "Songs for the Deaf CD" keeps its
/// name because no number follows, and a track whose album tag reads only
/// "CD 1" keeps that too rather than joining every other title-less album.
pub(super) fn album_title_without_disc(album: &str) -> &str {
    match strip_trailing_disc_marker(album) {
        Some(stripped) if !stripped.trim().is_empty() => stripped,
        _ => album,
    }
}

fn strip_trailing_disc_marker(album: &str) -> Option<&str> {
    let trimmed = album.trim_end();
    let (rest, opener) = match trimmed.chars().next_back()? {
        ')' => (&trimmed[..trimmed.len() - 1], Some('(')),
        ']' => (&trimmed[..trimmed.len() - 1], Some('[')),
        _ => (trimmed, None),
    };

    // The number is what separates a marker from a title. Without it, "CD" is
    // just a word the album is called.
    let rest = rest.trim_end();
    let without_number = rest.trim_end_matches(|character: char| character.is_ascii_digit());
    if without_number.len() == rest.len() {
        return None;
    }

    let rest = without_number.trim_end();
    let word = DISC_WORDS.iter().find(|word| {
        rest.len()
            .checked_sub(word.len())
            .and_then(|start| rest.get(start..))
            .is_some_and(|tail| tail.eq_ignore_ascii_case(word))
    })?;
    let rest = &rest[..rest.len() - word.len()];

    // "Megadisc 2" is not "Mega" with a disc marker: the word has to start
    // where the title stops.
    if rest.chars().next_back()?.is_alphanumeric() {
        return None;
    }
    let rest = match opener {
        Some(opener) => rest.trim_end().strip_suffix(opener)?,
        None => rest,
    };
    Some(rest.trim_end_matches(|character: char| {
        character.is_whitespace() || DISC_SEPARATORS.contains(&character)
    }))
}

#[cfg(test)]
mod tests {
    use super::{album_group_key, album_title_without_disc};

    #[test]
    fn doc_1g_a_trailing_disc_marker_is_dropped_in_every_common_spelling() {
        for title in [
            "Album (Disc 1)",
            "Album (disc 1)",
            "Album [Disc 1]",
            "Album [CD2]",
            "Album, Disc 3",
            "Album - Disc 2",
            "Album \u{2013} CD 2",
            "Album Disc 1",
            "Album (Disk 2)",
            "Album (CD 1)  ",
        ] {
            assert_eq!(album_title_without_disc(title), "Album", "on {title:?}");
        }
    }

    #[test]
    fn doc_1g_a_title_that_only_looks_like_a_disc_marker_is_left_alone() {
        for title in [
            // No number: this is what the record is called.
            "Songs for the Deaf CD",
            "The Blue Disc",
            // Nothing but the marker — stripping it would merge every album
            // whose tag says no more than this.
            "CD 1",
            "Disc 2",
            // Not at the end.
            "Disc 1 Rarities",
            // Not a marker at all.
            "Ocean's 11",
            "Megadisc 2",
            "Album (CD 1 of 2)",
            "",
        ] {
            assert_eq!(album_title_without_disc(title), title, "on {title:?}");
        }
    }

    #[test]
    fn doc_1g_the_group_key_folds_the_discs_of_one_set_and_nothing_else() {
        assert_eq!(
            album_group_key("Artist", "Album (Disc 1)"),
            album_group_key("Artist", "Album [CD2]")
        );
        assert_ne!(
            album_group_key("Artist", "Album (Disc 1)"),
            album_group_key("Other Artist", "Album (Disc 1)")
        );
        assert_ne!(
            album_group_key("Artist", "First Album"),
            album_group_key("Artist", "Second Album")
        );
    }
}
