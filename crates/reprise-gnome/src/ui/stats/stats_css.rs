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
           font-size: 42px; \
           font-weight: 800; \
           letter-spacing: -0.02em; }}\n\
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
         .stats-spotlight {{ padding: 8px; }}\n\
         .stats-spotlight-cover {{ \
           border-radius: {radius}; \
           background-color: alpha(@reprise_player_accent, 0.12); }}\n\
         .stats-eyebrow {{ \
           font-size: 11px; \
           font-weight: 700; \
           letter-spacing: 0.06em; \
           color: alpha(@window_fg_color, 0.58); }}\n\
         .stats-rank-badge {{ \
           font-weight: 800; \
           color: @accent_color; }}\n\
         .stats-spotlight-name {{ font-size: 28px; font-weight: 800; }}\n\
         .stats-track-chip {{ \
           padding: 4px 8px; \
           border-radius: 999px; \
           background-color: alpha(@window_fg_color, 0.08); }}\n\
         .stats-ghost-rank, .stats-unify-hint {{ \
           color: alpha(@window_fg_color, 0.58); }}\n\
         .stats-genre-bar {{ border-radius: 999px; }}\n\
         .stats-genre-segment-0 {{ background-color: alpha(@accent_bg_color, 1.0); }}\n\
         .stats-genre-segment-1 {{ background-color: alpha(@accent_bg_color, 0.82); }}\n\
         .stats-genre-segment-2 {{ background-color: alpha(@accent_bg_color, 0.68); }}\n\
         .stats-genre-segment-3 {{ background-color: alpha(@accent_bg_color, 0.54); }}\n\
         .stats-genre-segment-4 {{ background-color: alpha(@accent_bg_color, 0.40); }}\n\
         .stats-genre-segment-5 {{ background-color: alpha(@accent_bg_color, 0.26); }}\n\
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
    fn stats_css_defines_the_ribbon_pill_and_spotlight_classes() {
        let css = super::css();
        assert!(css.contains(".stats-ribbon"));
        assert!(css.contains(".stats-pill"));
        assert!(css.contains(".stats-spotlight"));
    }

    #[test]
    fn css_defines_chart_and_headline_classes() {
        let css = super::css();
        assert!(css.contains(".stats-chart"));
        assert!(css.contains(".stats-headline-hours"));
        assert!(css.contains(".stats-section-title"));
        assert!(css.contains(".stats-card"));
        assert!(css.contains(".stats-badge"));
        assert!(css.contains(".stats-cover-thumb"));
        assert!(css.contains(".stats-period-dropdown"));
        assert!(css.contains("@accent_color"));
    }
}
