//! Structural styling for the Artists master/detail view (Task 10).
//!
//! Kept out of `library_view_css.rs` (which owns the Albums grid) so each
//! feature CSS section stays focused. All colors come from the theme's
//! `@accent_color` / `@accent_bg_color` / `@window_fg_color` vars (never a
//! hard-coded palette) and the shared [`tokens`]. Artist avatar gradients are
//! a finite, centrally registered palette; the cover-derived hero glow is
//! drawn by its widget because it is genuinely dynamic data.

use crate::ui::style::tokens;

/// Selected-row tint alpha (over `@accent_bg_color`). Single-use, so it stays
/// local rather than living in the shared token table.
const ROW_SELECTED_BG_ALPHA: &str = "0.14";
/// Album-card hover tint alpha (over `@accent_bg_color`), matching the Albums
/// grid card hover in `library_view_css.rs`.
const CARD_HOVER_BG_ALPHA: &str = "0.12";
/// Hero-meta text alpha — one notch brighter than the master-list muted text.
const HERO_META_ALPHA: &str = "0.55";
/// Top-track rank text alpha — the quietest label in the view.
const RANK_ALPHA: &str = "0.35";

pub(in crate::ui) fn css() -> String {
    use tokens::{
        AVATAR_INITIALS_COLOR, HOVER_BG_ALPHA, MUTED_TEXT_ALPHA, RADIUS_SURFACE, SUBTLE_FILL_ALPHA,
        SUBTLE_FILL_HOVER_ALPHA, SURFACE_BORDER_ALPHA, TRANSITION,
    };
    let mut output = format!(
        ".artist-master {{ \
           border-right: 1px solid alpha(@window_fg_color, {SURFACE_BORDER_ALPHA}); }}\n\
         .artist-master-header {{ padding: 16px 14px 8px 16px; }}\n\
         .artist-master-title {{ font-weight: 800; font-size: 17px; }}\n\
         .artist-master-count {{ \
           margin-left: 2px; font-size: 12px; \
           color: alpha(@window_fg_color, {MUTED_TEXT_ALPHA}); }}\n\
         .artist-master-sort > button {{ \
           min-height: 0; padding: 3px 6px; font-size: 12px; \
           background-color: transparent; box-shadow: none; border: none; \
           transition: background-color {TRANSITION}; }}\n\
         .artist-master-sort > button:hover {{ \
           background-color: alpha(@window_fg_color, {SUBTLE_FILL_ALPHA}); }}\n\
         .artist-list {{ background-color: transparent; padding: 4px 8px; }}\n\
         .artist-list row {{ padding: 0; }}\n\
         .artist-list row:selected {{ background-color: transparent; }}\n\
         .artist-list-row {{ \
           min-height: 56px; padding: 0 12px; border-radius: {RADIUS_SURFACE}; \
           transition: background-color {TRANSITION}; }}\n\
         .artist-list-row:hover {{ \
           background-color: alpha(@accent_bg_color, {HOVER_BG_ALPHA}); }}\n\
         .artist-list row:selected .artist-list-row {{ \
           background-color: alpha(@accent_bg_color, {ROW_SELECTED_BG_ALPHA}); }}\n\
         .artist-list row:selected .artist-list-name {{ font-weight: 700; }}\n\
         .artist-list-avatar {{ border-radius: 999px; }}\n\
         .artist-list-avatar label {{ \
           color: {AVATAR_INITIALS_COLOR}; font-weight: 700; font-size: 13px; }}\n\
         .artist-list-name {{ font-size: 13.5px; }}\n\
         .artist-list-meta {{ \
           font-size: 11.5px; color: alpha(@window_fg_color, {MUTED_TEXT_ALPHA}); }}\n\
         .artist-list-section {{ \
           font-size: 11px; font-weight: 700; letter-spacing: 1px; \
           text-transform: uppercase; padding: 12px 12px 4px 12px; \
           color: alpha(@window_fg_color, {MUTED_TEXT_ALPHA}); }}\n\
         .artist-detail {{ padding: 24px 28px 32px 28px; }}\n\
         .artist-hero {{ padding: 24px 8px 8px 8px; }}\n\
         .artist-hero-glow {{ \
           min-height: 320px; border-radius: 28px; opacity: 0.45; }}\n\
         .artist-eyebrow {{ \
           font-size: 11px; font-weight: 700; letter-spacing: 1.2px; \
           text-transform: uppercase; \
           color: alpha(@window_fg_color, {MUTED_TEXT_ALPHA}); }}\n\
         .artist-hero-name {{ font-size: 34px; font-weight: 800; }}\n\
         .artist-hero-meta {{ \
           font-size: 13.5px; color: alpha(@window_fg_color, {HERO_META_ALPHA}); }}\n\
         .artist-hero-avatar {{ border-radius: 999px; }}\n\
         .artist-hero-initials {{ \
           color: {AVATAR_INITIALS_COLOR}; font-weight: 800; font-size: 46px; }}\n\
         .artist-hero-actions {{ margin-top: 4px; }}\n\
         .artist-hero-play {{ padding: 8px 22px; min-height: 40px; }}\n\
         .artist-hero-shuffle {{ \
           padding: 8px 20px; min-height: 40px; color: @window_fg_color; \
           background-color: alpha(@window_fg_color, {SUBTLE_FILL_ALPHA}); \
           box-shadow: none; transition: background-color {TRANSITION}; }}\n\
         .artist-hero-shuffle:hover {{ \
           background-color: alpha(@window_fg_color, {SUBTLE_FILL_HOVER_ALPHA}); }}\n\
         .artist-hero-menu {{ \
           min-width: 36px; min-height: 36px; border-radius: 999px; \
           background-color: alpha(@window_fg_color, {SUBTLE_FILL_ALPHA}); \
           transition: background-color {TRANSITION}; }}\n\
         .artist-hero-menu:hover {{ \
           background-color: alpha(@window_fg_color, {SUBTLE_FILL_HOVER_ALPHA}); }}\n\
         .artist-section-title {{ \
           font-size: 13px; font-weight: 700; letter-spacing: 0.6px; \
           text-transform: uppercase; \
           color: alpha(@window_fg_color, {HERO_META_ALPHA}); }}\n\
         .artist-albums-section {{ margin-top: 12px; }}\n\
         .artist-albums {{ background-color: transparent; }}\n\
         .artist-album-card {{ \
           padding: 6px; border-radius: {RADIUS_SURFACE}; \
           background-color: transparent; box-shadow: none; \
           transition: background-color {TRANSITION}; }}\n\
         .artist-album-card:hover {{ \
           background-color: alpha(@accent_bg_color, {CARD_HOVER_BG_ALPHA}); }}\n\
         .artist-album-cover {{ \
           border-radius: 10px; \
           box-shadow: inset 0 0 0 1px alpha(@window_fg_color, {SURFACE_BORDER_ALPHA}); }}\n\
         .artist-album-title {{ font-weight: 700; font-size: 12.5px; }}\n\
         .artist-album-meta {{ \
           font-size: 11.5px; color: alpha(@window_fg_color, {MUTED_TEXT_ALPHA}); }}\n\
         .artist-albums-hint {{ \
           padding: 8px 2px; color: alpha(@window_fg_color, {MUTED_TEXT_ALPHA}); }}\n\
         .artist-albums-show-all, .artist-top-show-all {{ color: @accent_color; }}\n\
         .artist-top-section {{ margin-top: 4px; }}\n\
         .artist-top-track {{ \
           min-height: 44px; padding: 0 6px; \
           border-bottom: 1px solid alpha(@window_fg_color, {SURFACE_BORDER_ALPHA}); \
           transition: background-color {TRANSITION}; }}\n\
         .artist-top-track:hover {{ \
           background-color: alpha(@accent_bg_color, {HOVER_BG_ALPHA}); }}\n\
         .artist-top-track-rank {{ \
           font-feature-settings: \"tnum\"; \
           color: alpha(@window_fg_color, {RANK_ALPHA}); }}\n\
         .artist-top-track-cover {{ \
           border-radius: 6px; \
           box-shadow: inset 0 0 0 1px alpha(@window_fg_color, {SURFACE_BORDER_ALPHA}); }}\n\
         .artist-top-track-title {{ font-size: 13px; }}\n\
         .artist-top-track-album {{ \
           font-size: 11.5px; color: alpha(@window_fg_color, {MUTED_TEXT_ALPHA}); }}\n\
         .artist-top-track-plays {{ \
           font-size: 12px; color: alpha(@window_fg_color, {MUTED_TEXT_ALPHA}); }}\n\
         .artist-top-track-duration {{ \
           font-feature-settings: \"tnum\"; font-size: 12px; \
           color: alpha(@window_fg_color, {MUTED_TEXT_ALPHA}); }}"
    );
    for index in 0..crate::ui::artist_avatar::GRADIENT_COUNT {
        let gradient = crate::ui::artist_avatar::gradient_css_for_index(index);
        output.push_str(&format!(
            ".artist-avatar-gradient-{index} {{ background-image: {gradient}; }}\n"
        ));
    }
    output
}

#[cfg(test)]
mod tests {
    #[test]
    fn css_styles_every_artist_view_class() {
        let css = super::css();
        for marker in [
            // Master pane
            ".artist-master",
            ".artist-master-header",
            ".artist-master-title",
            ".artist-master-count",
            ".artist-master-sort",
            ".artist-list",
            ".artist-list-row",
            ".artist-list-row:hover",
            ".artist-list-avatar",
            ".artist-list-name",
            ".artist-list-meta",
            ".artist-list-section",
            // Detail hero
            ".artist-detail",
            ".artist-hero-glow",
            ".artist-eyebrow",
            ".artist-hero-name",
            ".artist-hero-meta",
            ".artist-hero-avatar",
            ".artist-hero-initials",
            ".artist-hero-play",
            ".artist-hero-shuffle",
            ".artist-hero-menu",
            // Albums + top tracks
            ".artist-section-title",
            ".artist-albums-section",
            ".artist-album-card",
            ".artist-album-cover",
            ".artist-albums-hint",
            ".artist-top-track",
            ".artist-top-track-rank",
            ".artist-top-track-cover",
            ".artist-top-track-plays",
            ".artist-top-track-duration",
        ] {
            assert!(css.contains(marker), "missing artist-view rule: {marker}");
        }
    }

    #[test]
    fn css_uses_theme_vars_not_a_hard_coded_palette() {
        let css = super::css();
        assert!(css.contains("@accent_color"));
        assert!(css.contains("@accent_bg_color"));
        assert!(css.contains("@window_fg_color"));
        assert!(!css.contains("@define-color"));
    }

    #[test]
    fn css_registers_the_complete_artist_avatar_palette() {
        let css = super::css();
        for index in 0..crate::ui::artist_avatar::GRADIENT_COUNT {
            assert!(css.contains(&format!(".artist-avatar-gradient-{index}")));
        }
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn css_parses_in_gtk_without_dropping_declarations() {
        use std::cell::RefCell;
        use std::rc::Rc;

        if gtk4::init().is_err() {
            return;
        }
        // The theme provider defines `@accent_color` etc.; load it first so the
        // color references in our section resolve during parsing.
        let errors: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let provider = gtk4::CssProvider::new();
        {
            let errors = errors.clone();
            provider.connect_parsing_error(move |_, section, error| {
                errors.borrow_mut().push(format!("{section:?}: {error}"));
            });
        }
        let combined = format!(
            "{}\n{}",
            crate::ui::style::theme::theme_css(crate::ui::style::theme::Theme::DEFAULT, true),
            super::css()
        );
        provider.load_from_string(&combined);
        assert!(
            errors.borrow().is_empty(),
            "GTK reported CSS parsing errors: {:?}",
            errors.borrow()
        );
    }

    #[test]
    fn selected_row_reads_the_listview_selection_state() {
        // GtkListView marks selection on the item's `row` node, not our inner
        // `.artist-list-row` box, so the selected treatment must reach through
        // `row:selected` (regression guard against styling only the box).
        let css = super::css();
        assert!(css.contains("row:selected"));
    }
}
