//! CSS for the My Stats view, registered in the shared application stylesheet.
//!
//! Uses the existing design-system tokens (`.reprise-surface` for cards,
//! `@accent_color` for the chart, `@window_fg_color` for text) so the view
//! recolors with the active theme.

use crate::ui::style::tokens;

pub(in crate::ui) fn css() -> String {
    format!(
        // Cairo charts pick up `color` from these selectors.
        ".stats-chart, .stats-ribbon {{ color: @accent_color; }}\n\
         \
         .stats-headline-hours {{ \
           font-size: 52px; \
           font-weight: 800; \
           letter-spacing: -0.02em; }}\n\
         \
         .stats-header-title {{ \
           font-size: 19px; \
           font-weight: 700; }}\n\
         .stats-hero-number {{ \
           font-size: 84px; \
           font-weight: 500; \
           letter-spacing: -0.03em; }}\n\
         .stats-kpi-label {{ \
           font-size: 10px; \
           font-weight: 700; \
           letter-spacing: 0.06em; \
           color: alpha(@window_fg_color, 0.55); }}\n\
         .stats-kpi-value {{ \
           font-size: 17px; \
           font-weight: 700; }}\n\
         .stats-kpi-reference {{ \
           font-size: 11px; \
           color: alpha(@window_fg_color, 0.55); }}\n\
         .stats-kpi-trend-icon {{ color: @accent_color; }}\n\
         \
         .stats-headline-subtitle {{ \
           font-size: 14px; \
           color: alpha(@window_fg_color, 0.6); }}\n\
         \
         .stats-pill {{ \
           font-size: 12px; \
           font-weight: 700; \
           padding: 4px 8px; \
           border-radius: 999px; \
           background-color: alpha(@accent_bg_color, 0.20); \
           color: @accent_color; }}\n\
         \
         .stats-section-title {{ \
           font-size: 13px; \
           font-weight: 700; \
           letter-spacing: 0.04em; \
           color: alpha(@window_fg_color, 0.55); }}\n\
         \
         .stats-rank {{ \
           font-size: 13px; \
           font-weight: 700; \
           min-width: 22px; \
           color: alpha(@window_fg_color, 0.58); }}\n\
         \
         .stats-item-title {{ \
           font-size: 14px; \
           font-weight: 600; }}\n\
         \
         .stats-item-subtitle {{ \
           font-size: 12px; \
           color: alpha(@window_fg_color, 0.55); }}\n\
         \
         .stats-play-count {{ \
           font-size: 12px; \
           font-weight: 600; \
           color: alpha(@window_fg_color, 0.58); }}\n\
         \
         .stats-card {{ \
           border-radius: {radius}; \
           border: 1px solid alpha(@window_fg_color, {border_alpha}); \
           padding: 16px; }}\n\
         .stats-thin-history {{ \
           padding: 12px; \
           color: alpha(@window_fg_color, 0.68); }}\n\
         \
         .stats-badge {{ \
           font-size: 9px; \
           font-weight: 700; \
           letter-spacing: 0.06em; \
           padding: 1px 6px; \
           border-radius: 4px; \
           background-color: alpha(@accent_bg_color, 0.28); \
           color: @accent_color; }}\n\
         \
         .stats-cover-thumb {{ \
           border-radius: 4px; }}\n\
         \
         .stats-period-dropdown {{ min-width: 140px; }}\n\
         .stats-band-card {{ \
           border-radius: {radius}; \
           background-color: @card_bg_color; }}\n\
         .stats-band-fade {{ \
           background-image: linear-gradient(to top, @card_bg_color 8%, \
             alpha(@card_bg_color, 0) 55%); }}\n\
         .stats-band-content {{ padding: 20px; }}\n\
         .stats-band-initials {{ \
           font-size: 64px; \
           font-weight: 700; \
           color: @accent_color; \
           background-color: alpha(@accent_bg_color, 0.18); }}\n\
         .stats-band-name {{ font-size: 28px; font-weight: 800; }}\n\
         .stats-band-rank {{ padding: 2px 0; }}\n\
         .stats-band-rank-bar block.filled {{ \
           background-color: @accent_bg_color; \
           min-height: 4px; }}\n\
         .stats-band-rank-bar block.empty {{ \
           background-color: alpha(@window_fg_color, 0.06); \
           min-height: 4px; }}\n\
         .stats-songs-card {{ padding: 8px; }}\n\
         .stats-song-row {{ padding: 5px; }}\n\
         .stats-song-row:hover {{ background-color: alpha(@window_fg_color, 0.05); }}\n\
         .stats-song-row:focus-visible {{ outline: 2px solid @accent_color; }}\n\
         .stats-song-play {{ background-color: alpha(@card_bg_color, 0.88); }}\n\
         .stats-song-bar block.filled {{ \
           background-image: linear-gradient(to right, \
             shade(@accent_bg_color, 0.7), shade(@accent_bg_color, 1.15)); \
           min-height: 5px; }}\n\
         .stats-song-bar block.empty {{ \
           background-color: alpha(@window_fg_color, 0.06); \
           min-height: 5px; }}\n\
         .stats-songs-reveal {{ color: @accent_color; }}\n\
         .stats-eyebrow {{ \
           font-size: 11px; \
           font-weight: 700; \
           letter-spacing: 0.06em; \
           color: alpha(@window_fg_color, 0.58); }}\n\
         .stats-rank-badge {{ \
           font-weight: 800; \
           color: @accent_color; }}\n\
         .stats-track-chip {{ \
           padding: 4px 8px; \
           border-radius: 999px; \
           background-color: alpha(@window_fg_color, 0.08); }}\n\
         .stats-ghost-rank, .stats-unify-hint {{ \
           color: alpha(@window_fg_color, 0.58); }}\n\
         .stats-genre-card {{ padding: 8px; }}\n\
         .stats-genre-bar {{ border-radius: 999px; }}\n\
         .stats-genre-rank-0 {{ background-color: shade(@accent_bg_color, 1.15); }}\n\
         .stats-genre-rank-1 {{ background-color: shade(@accent_bg_color, 1.0); }}\n\
         .stats-genre-rank-2 {{ background-color: shade(@accent_bg_color, 0.85); }}\n\
         .stats-genre-rank-3 {{ background-color: shade(@accent_bg_color, 0.70); }}\n\
         .stats-genre-rank-4 {{ background-color: shade(@accent_bg_color, 0.55); }}\n\
         .stats-genre-segment-last {{ background-color: alpha(@window_fg_color, 0.25); }}\n\
         .stats-genre-tile {{ padding: 4px; }}\n\
         .stats-genre-cover:focus-visible {{ outline: 2px solid @accent_color; }}\n\
         .stats-highlight-tile {{ \
           padding: 12px; \
           border-radius: {radius}; \
           background-color: alpha(@window_fg_color, 0.05); }}\n\
         .stats-highlight-value {{ font-size: 18px; font-weight: 700; }}\n\
         .stats-top-track-row {{ padding: 5px; }}",
        radius = tokens::RADIUS_SURFACE,
        border_alpha = tokens::SURFACE_BORDER_ALPHA,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn stats_css_omits_unsupported_overflow_property() {
        assert!(!super::css().contains("overflow:"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stats_css_parses_without_errors() {
        gtk4::init().unwrap();
        let errors = crate::ui::style::css_parse_errors(&super::css());
        assert!(errors.is_empty(), "CSS parse errors: {errors:?}");
    }

    #[test]
    fn stats_css_defines_the_ribbon_pill_and_band_classes() {
        let css = super::css();
        assert!(css.contains(".stats-ribbon"));
        assert!(css.contains(".stats-pill"));
        assert!(css.contains(".stats-band-card"));
        assert!(css.contains("@card_bg_color"));
    }

    #[test]
    fn css_defines_chart_and_headline_classes() {
        let css = super::css();
        assert!(css.contains(".stats-chart"));
        assert!(css.contains(".stats-headline-hours"));
        assert!(css.contains(".stats-hero-number"));
        assert!(css.contains(".stats-kpi-label"));
        assert!(css.contains(".stats-section-title"));
        assert!(css.contains(".stats-card"));
        assert!(css.contains(".stats-thin-history"));
        assert!(css.contains(".stats-badge"));
        assert!(css.contains(".stats-cover-thumb"));
        assert!(css.contains(".stats-period-dropdown"));
        assert!(css.contains("@accent_color"));
    }
}
