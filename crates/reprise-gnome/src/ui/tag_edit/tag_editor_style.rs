//! Tag-editor redesign chrome: surface treatment, multi-edit badges, and
//! accent hint labels. Installed app-wide by [`super::style`]; palette
//! colors come from the theme provider, not from this structural CSS.

pub(super) fn css() -> String {
    use crate::ui::style::tokens::{
        RADIUS_SURFACE, SURFACE_BORDER_ALPHA, SURFACE_SHADOW, TRANSITION,
    };
    format!(
        ".reprise-tag-editor {{ \
           border-radius: {RADIUS_SURFACE}; \
           border: 1px solid alpha(@window_fg_color, {SURFACE_BORDER_ALPHA}); \
           box-shadow: {SURFACE_SHADOW}; }}\n\
         .reprise-tag-badge {{ \
           font-size: 11px; \
           padding: 1px 8px; \
           border-radius: 9999px; \
           background: alpha(@accent_bg_color, 0.15); \
           color: @reprise_dim_fg_color; \
           transition: background {TRANSITION}; }}\n\
         .reprise-tag-hint {{ \
           color: @accent_color; \
           font-size: 12px; }}"
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn css_defines_tag_editor_styles() {
        let css = super::css();
        assert!(css.contains(".reprise-tag-editor"));
        assert!(css.contains(".reprise-tag-badge"));
        assert!(css.contains(".reprise-tag-hint"));
        assert!(css.contains("@accent_color"));
        assert!(css.contains("@reprise_dim_fg_color"));
    }
}
