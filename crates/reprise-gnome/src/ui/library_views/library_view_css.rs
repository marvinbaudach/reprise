//! Structural styling for the visual Albums and Artists library views.

use crate::ui::style::tokens;

pub(in crate::ui) fn css() -> String {
    format!(
        ".library-grid {{ padding: 24px; }}\n\
         .library-album-card.reprise-surface {{ padding: 10px; min-width: 184px; }}\n\
         .library-album-card.reprise-hover:hover {{ \
           background-color: alpha(@accent_bg_color, 0.12); }}\n\
         .library-album-cover {{ border-radius: {radius}; }}\n\
         .library-artist-card.reprise-surface {{ padding: 16px; min-width: 230px; }}\n\
         .library-artist-card.reprise-hover:hover {{ \
           background-color: alpha(@accent_bg_color, 0.12); }}\n\
         .library-artist-avatar {{ min-width: 52px; min-height: 52px; \
           border-radius: 999px; background-color: alpha(@accent_bg_color, 0.18); \
           color: @accent_color; }}\n\
         .library-card-title {{ font-weight: 700; font-size: 14px; }}\n\
         .library-card-subtitle {{ font-size: 12px; color: alpha(@window_fg_color, 0.58); }}",
        radius = tokens::RADIUS_SURFACE,
    )
    .into()
}

#[cfg(test)]
mod tests {
    #[test]
    fn css_uses_the_design_system_surface_and_hover_vocabulary() {
        let css = super::css();
        assert!(css.contains(".library-album-card"));
        assert!(css.contains(".library-artist-card"));
        assert!(css.contains(".reprise-surface"));
        assert!(css.contains(".reprise-hover:hover"));
        assert!(css.contains("@window_fg_color"));
    }
}
