//! FIL-5: accent-bold highlighting of the live-search needle inside cell
//! text. Matching is ASCII-case-insensitive on purpose — it mirrors the
//! SQLite LIKE semantics of the search query, so a highlighted row and a
//! matching row are the same set.

use std::cell::RefCell;

use gtk4::glib;
use gtk4::prelude::*;

pub(in crate::ui) fn is_searchable_column(sort_id: &str) -> bool {
    matches!(sort_id, "artist" | "album" | "genre")
}

pub(in crate::ui) fn highlight_markup(
    text: &str,
    needle: &str,
    foreground: Option<&str>,
) -> Option<String> {
    let needle = needle.trim();
    if needle.is_empty() {
        return None;
    }
    let hay = text.to_ascii_lowercase();
    let ndl = needle.to_ascii_lowercase();
    let (open, close) = match foreground {
        Some(hex) => (
            format!("<span foreground=\"{hex}\" weight=\"bold\">"),
            "</span>",
        ),
        None => ("<b>".to_string(), "</b>"),
    };
    let mut out = String::new();
    let mut cursor = 0usize;
    let mut found = false;
    while let Some(pos) = hay[cursor..].find(&ndl) {
        let start = cursor + pos;
        let end = start + ndl.len();
        out.push_str(&glib::markup_escape_text(&text[cursor..start]));
        out.push_str(&open);
        out.push_str(&glib::markup_escape_text(&text[start..end]));
        out.push_str(close);
        cursor = end;
        found = true;
    }
    if !found {
        return None;
    }
    out.push_str(&glib::markup_escape_text(&text[cursor..]));
    Some(out)
}

/// Highlights against the live filter without cloning it. Empty searches
/// return before `resolve_foreground`, avoiding a style-manager lookup and
/// allocation on the normal, unfiltered bind path. The filter borrow is
/// deliberately dropped before invoking the GTK-facing resolver.
pub(in crate::ui) fn highlight_from_filter(
    text: &str,
    filter: &RefCell<String>,
    resolve_foreground: impl FnOnce() -> Option<String>,
) -> Option<String> {
    if filter.borrow().trim().is_empty() {
        return None;
    }
    let foreground = resolve_foreground();
    let needle = filter.borrow();
    highlight_markup(text, &needle, foreground.as_deref())
}

/// Resolves libadwaita's current accent into the literal color Pango markup
/// requires. The widget parameter keeps the binding-facing interface stable.
pub(in crate::ui) fn accent_foreground(_widget: &impl IsA<gtk4::Widget>) -> Option<String> {
    let rgba = crate::ui::style::accent::accent_rgba();
    Some(format!(
        "#{:02x}{:02x}{:02x}",
        (rgba.red() * 255.0) as u8,
        (rgba.green() * 255.0) as u8,
        (rgba.blue() * 255.0) as u8
    ))
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::*;

    // UX FIL-5: matching mirrors SQLite LIKE — ASCII-case-insensitive substring.
    #[test]
    fn fil_5_highlight_matches_are_ascii_case_insensitive() {
        assert_eq!(
            highlight_markup("Falling Apart", "falling", None),
            Some("<b>Falling</b> Apart".to_string())
        );
    }

    // UX FIL-5: every occurrence is highlighted, not only the first.
    #[test]
    fn fil_5_all_occurrences_are_highlighted() {
        assert_eq!(
            highlight_markup("la la", "la", None),
            Some("<b>la</b> <b>la</b>".to_string())
        );
    }

    // UX FIL-5: cell text is Pango-escaped — markup metacharacters stay literal.
    #[test]
    fn fil_5_highlight_escapes_pango_markup() {
        assert_eq!(
            highlight_markup("Rock & <Roll>", "rock", None),
            Some("<b>Rock</b> &amp; &lt;Roll&gt;".to_string())
        );
    }

    // UX FIL-5: no needle or no match → no markup (caller uses set_text).
    #[test]
    fn fil_5_no_markup_when_needle_empty_or_absent() {
        assert_eq!(highlight_markup("Falling", "  ", None), None);
        assert_eq!(highlight_markup("Falling", "xyz", None), None);
    }

    #[test]
    fn fil_5_inactive_search_skips_accent_resolution() {
        let filter = RefCell::new(String::new());
        let accent_requested = Cell::new(false);

        let markup = highlight_from_filter("Falling", &filter, || {
            accent_requested.set(true);
            Some("#2ec8a6".to_string())
        });

        assert_eq!(markup, None);
        assert!(!accent_requested.get());
    }

    // UX FIL-5: with a resolved accent, matches are accent bold.
    #[test]
    fn fil_5_accent_color_wraps_the_match() {
        assert_eq!(
            highlight_markup("Falling", "fall", Some("#2ec8a6")),
            Some("<span foreground=\"#2ec8a6\" weight=\"bold\">Fall</span>ing".to_string())
        );
    }

    // UX FIL-5: only the text fields used by the SQL search request markup;
    // numeric metadata columns must not imply that they contributed a match.
    #[test]
    fn fil_5_only_searched_columns_request_highlighting() {
        assert!(is_searchable_column("artist"));
        assert!(is_searchable_column("album"));
        assert!(is_searchable_column("genre"));
        assert!(!is_searchable_column("year"));
        assert!(!is_searchable_column("track_number"));
        assert!(!is_searchable_column("duration"));
        assert!(!is_searchable_column("play_count"));
    }

    #[test]
    fn nr_2_release_placeholders_and_search_highlights_use_the_central_accent() {
        let highlight = include_str!("match_highlight.rs");
        let release_cover = include_str!("../updates/release_cover.rs");
        let central = ["style::accent", "::accent_rgba()"].concat();
        assert!(highlight.contains(&central));
        assert!(release_cover.contains(&central));
        for retired in [
            ["StyleManager", "::default"].concat(),
            ["DEFAULT", "_ACCENT"].concat(),
            ["cover_", "palette"].concat(),
        ] {
            assert!(
                !highlight.contains(&retired),
                "highlight retained {retired}"
            );
            assert!(
                !release_cover.contains(&retired),
                "release cover retained {retired}"
            );
        }
    }
}
