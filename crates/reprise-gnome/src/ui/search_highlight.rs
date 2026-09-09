//! FIL-5a: one accent-bold, accent-tinted search highlight for every visible
//! field that participates in a section's query.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;

const HIT_BACKGROUND_ALPHA: &str = "18%";
const TEXT_COLOR_MIX: f32 = 0.20;

pub(in crate::ui) type QuerySource = Rc<dyn Fn() -> String>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui) struct HighlightPalette {
    foreground: String,
    background: String,
}

impl HighlightPalette {
    #[cfg(test)]
    pub(in crate::ui) fn new(foreground: &str, background: &str) -> Self {
        Self {
            foreground: foreground.to_owned(),
            background: background.to_owned(),
        }
    }
}

pub(in crate::ui) fn highlight_markup(
    text: &str,
    needle: &str,
    palette: Option<&HighlightPalette>,
) -> Option<String> {
    let needle = needle.trim();
    if needle.is_empty() {
        return None;
    }
    let hay = text.to_ascii_lowercase();
    let ndl = needle.to_ascii_lowercase();
    let (open, close) = match palette {
        Some(palette) => (
            format!(
                "<span foreground=\"{}\" background=\"{}\" bgalpha=\"{}\" weight=\"bold\">",
                palette.foreground, palette.background, HIT_BACKGROUND_ALPHA
            ),
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
/// return before `resolve_palette`, avoiding GTK color reads and allocation
/// on the normal, unfiltered bind path. The filter borrow is deliberately
/// dropped before invoking the GTK-facing resolver.
pub(in crate::ui) fn highlight_from_filter(
    text: &str,
    filter: &RefCell<String>,
    resolve_palette: impl FnOnce() -> HighlightPalette,
) -> Option<String> {
    if filter.borrow().trim().is_empty() {
        return None;
    }
    let palette = resolve_palette();
    let needle = filter.borrow();
    highlight_markup(text, &needle, Some(&palette))
}

fn accent_palette_for_appearance(
    theme: crate::ui::style::theme::Theme,
    is_dark: bool,
    accent: [u8; 3],
    dark_foreground: String,
) -> HighlightPalette {
    let foreground = if is_dark {
        dark_foreground
    } else {
        let palette = theme.light_palette();
        crate::ui::style::accent::accent_text_color(
            accent,
            palette.critical_accent_surface(false, accent),
            false,
        )
    };
    HighlightPalette {
        foreground,
        background: rgba_hex(
            f32::from(accent[0]) / 255.0,
            f32::from(accent[1]) / 255.0,
            f32::from(accent[2]) / 255.0,
        ),
    }
}

fn dark_highlight_foreground(accent: gtk4::gdk::RGBA, text: gtk4::gdk::RGBA) -> String {
    let mix = |accent: f32, text: f32| accent + (text - accent) * TEXT_COLOR_MIX;
    rgba_hex(
        mix(accent.red(), text.red()),
        mix(accent.green(), text.green()),
        mix(accent.blue(), text.blue()),
    )
}

/// Resolves the selected accent once for both Pango roles. Light appearance
/// uses the same contrast-derived foreground as theme CSS against the same
/// critical surface. Dark keeps the established label-color mix unchanged.
/// The background stays the exact selected accent at the fixed 18% alpha
/// declared in the markup.
pub(in crate::ui) fn accent_palette(widget: &impl IsA<gtk4::Widget>) -> HighlightPalette {
    let accent_rgba = crate::ui::style::accent::accent_rgba();
    let dark_foreground = dark_highlight_foreground(accent_rgba, widget.color());
    let accent = [
        (accent_rgba.red() * 255.0).round() as u8,
        (accent_rgba.green() * 255.0).round() as u8,
        (accent_rgba.blue() * 255.0).round() as u8,
    ];
    accent_palette_for_appearance(
        crate::ui::style::current_theme(),
        crate::ui::style::accent::is_dark(),
        accent,
        dark_foreground,
    )
}

pub(in crate::ui) fn apply(label: &gtk4::Label, text: &str, needle: &str) {
    if needle.trim().is_empty() {
        label.set_text(text);
        return;
    }
    let palette = accent_palette(label);
    match highlight_markup(text, needle, Some(&palette)) {
        Some(markup) => label.set_markup(&markup),
        None => label.set_text(text),
    }
}

fn rgba_hex(red: f32, green: f32, blue: f32) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        (red.clamp(0.0, 1.0) * 255.0).round() as u8,
        (green.clamp(0.0, 1.0) * 255.0).round() as u8,
        (blue.clamp(0.0, 1.0) * 255.0).round() as u8
    )
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::*;

    #[test]
    fn fil_5a_light_highlight_foreground_clears_aa_on_its_own_tint() {
        use crate::ui::style::accent::APP_ACCENT;
        use crate::ui::style::color_math::{composite, contrast_ratio, parse_hex_rgb};
        use crate::ui::style::theme::Theme;

        for theme in Theme::all() {
            let accent = parse_hex_rgb(APP_ACCENT).expect("app accent is valid hex");
            let palette =
                accent_palette_for_appearance(theme, false, accent, "#unused".to_string());
            let foreground =
                parse_hex_rgb(&palette.foreground).expect("highlight foreground is hex");
            let background =
                parse_hex_rgb(&palette.background).expect("highlight background is hex");
            let view =
                parse_hex_rgb(theme.light_palette().view_bg).expect("view background is hex");
            let tinted = composite(background, view, 0.18);
            let ratio = contrast_ratio(foreground, tinted);

            assert!(
                ratio >= 4.5,
                "{theme:?}: light search highlight reaches only {ratio:.2}:1 on its own tint"
            );
        }
    }

    #[test]
    fn fil_5a_dark_highlight_keeps_the_existing_label_colour_mix() {
        let accent = gtk4::gdk::RGBA::new(79.0 / 255.0, 219.0 / 255.0, 212.0 / 255.0, 1.0);
        let text = gtk4::gdk::RGBA::new(231.0 / 255.0, 233.0 / 255.0, 236.0 / 255.0, 1.0);
        let dark_foreground = dark_highlight_foreground(accent, text);
        let palette = accent_palette_for_appearance(
            crate::ui::style::theme::Theme::DEFAULT,
            true,
            [79, 219, 212],
            dark_foreground,
        );

        assert_eq!(palette.foreground, "#6dded9");
        assert_eq!(palette.background, "#4fdbd4");
    }

    #[test]
    fn fil_5a_highlight_matches_are_ascii_case_insensitive() {
        assert_eq!(
            highlight_markup("Falling Apart", "falling", None),
            Some("<b>Falling</b> Apart".to_string())
        );
    }

    #[test]
    fn fil_5a_all_occurrences_are_highlighted() {
        assert_eq!(
            highlight_markup("la la", "la", None),
            Some("<b>la</b> <b>la</b>".to_string())
        );
    }

    #[test]
    fn fil_5a_highlight_escapes_pango_markup() {
        assert_eq!(
            highlight_markup("Rock & <Roll>", "rock", None),
            Some("<b>Rock</b> &amp; &lt;Roll&gt;".to_string())
        );
    }

    #[test]
    fn fil_5a_empty_or_absent_needle_has_no_markup() {
        assert_eq!(highlight_markup("Falling", "  ", None), None);
        assert_eq!(highlight_markup("Falling", "xyz", None), None);
    }

    #[test]
    fn fil_5a_inactive_search_skips_accent_resolution() {
        let filter = RefCell::new(String::new());
        let accent_requested = Cell::new(false);

        let markup = highlight_from_filter("Falling", &filter, || {
            accent_requested.set(true);
            HighlightPalette::new("#45d0b2", "#2ec8a6")
        });

        assert_eq!(markup, None);
        assert!(!accent_requested.get());
    }

    #[test]
    fn fil_5a_accent_color_and_tint_wrap_the_match() {
        let palette = HighlightPalette::new("#45d0b2", "#2ec8a6");
        assert_eq!(
            highlight_markup("Falling", "fall", Some(&palette)),
            Some(
                "<span foreground=\"#45d0b2\" background=\"#2ec8a6\" \
                 bgalpha=\"18%\" weight=\"bold\">Fall</span>ing"
                    .to_string()
            )
        );
    }

    #[test]
    fn nr_2a_search_highlights_use_the_central_accent() {
        let highlight = include_str!("search_highlight.rs");
        let release_cover = [
            include_str!("updates/release_cover.rs"),
            include_str!("updates/release_cover_tile.rs"),
        ]
        .concat();
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
