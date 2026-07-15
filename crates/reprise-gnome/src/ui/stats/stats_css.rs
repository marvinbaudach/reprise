//! CSS for the My Stats view, registered in [`super::super::style::app_css`].
//!
//! Uses the existing design-system tokens (`.reprise-surface` for cards,
//! `@accent_color` for the chart, `@window_fg_color` for text) so the view
//! recolors with the active theme.

use crate::ui::style::tokens;

pub(in crate::ui) fn css() -> String {
    format!(
        // Chart bars pick up `color` from this selector.
        ".stats-chart {{ color: @accent_color; }}\n\
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
           color: alpha(@window_fg_color, 0.4); }}\n\
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
           color: alpha(@window_fg_color, 0.45); }}\n\
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
         .stats-year-label {{ \
           font-size: 13px; \
           font-weight: 700; \
           letter-spacing: 0.04em; \
           color: @accent_color; }}\n\
         \
         .stats-mini-card {{ \
           border: 1px solid alpha(@window_fg_color, {border_alpha}); \
           border-radius: {radius}; \
           padding: 12px 16px; \
           min-width: 100px; }}\n\
         \
         .stats-mini-card-value {{ \
           font-size: 22px; \
           font-weight: 800; }}\n\
         \
         .stats-mini-card-label {{ \
           font-size: 11px; \
           color: alpha(@window_fg_color, 0.55); }}\n\
         \
         .stats-progress-bar {{ \
           min-height: 6px; \
           border-radius: 3px; }}\n\
         \
         .stats-progress-bar trough {{ \
           min-height: 6px; \
           border-radius: 3px; \
           background-color: alpha(@window_fg_color, 0.06); }}\n\
         \
         .stats-progress-bar trough progress {{ \
           min-height: 6px; \
           border-radius: 3px; \
           background-color: @accent_color; }}\n\
         \
         .stats-albums-strip {{ \
           padding: 8px 0; }}\n\
         \
         .stats-album-thumb {{ \
           border-radius: 6px; \
           background-color: alpha(@window_fg_color, 0.08); \
           min-width: 96px; \
           min-height: 96px; }}\n\
         \
         .stats-link {{ \
           font-size: 12px; \
           font-weight: 600; \
           color: @accent_color; }}\n\
         \
         .stats-genre-name {{ \
           font-size: 13px; \
           font-weight: 600; \
           min-width: 110px; }}\n\
         \
         .stats-genre-pct {{ \
           font-size: 12px; \
           font-weight: 600; \
           min-width: 36px; \
           color: alpha(@window_fg_color, 0.45); }}",
        radius = tokens::RADIUS_SURFACE,
        border_alpha = tokens::SURFACE_BORDER_ALPHA,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn css_defines_chart_and_headline_classes() {
        let css = super::css();
        assert!(css.contains(".stats-chart"));
        assert!(css.contains(".stats-headline-hours"));
        assert!(css.contains(".stats-section-title"));
        assert!(css.contains(".stats-card"));
        assert!(css.contains(".stats-badge"));
        assert!(css.contains("@accent_color"));
    }

    #[test]
    fn css_defines_redesign_card_classes() {
        let css = super::css();
        assert!(css.contains(".stats-year-label"));
        assert!(css.contains(".stats-mini-card"));
        assert!(css.contains(".stats-mini-card-value"));
        assert!(css.contains(".stats-mini-card-label"));
        assert!(css.contains(".stats-progress-bar"));
        assert!(css.contains(".stats-albums-strip"));
        assert!(css.contains(".stats-album-thumb"));
        assert!(css.contains(".stats-link"));
        assert!(css.contains(".stats-genre-name"));
        assert!(css.contains(".stats-genre-pct"));
    }
}
