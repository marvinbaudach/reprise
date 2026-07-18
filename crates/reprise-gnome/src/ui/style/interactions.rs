//! Redesign interaction + surface treatments, installed app-wide by [`super`].
//!
//! Three reusable pieces, all driven by [`super::tokens`] and the theme's
//! `@accent_color`, so they recolor with the active theme:
//! - an accent **focus glow** on text inputs (the redesign's focus language),
//! - a `.reprise-hover` class giving flat interactive elements a smooth,
//!   subtle background hover,
//! - a `.reprise-surface` class giving layered panels rounding, a hairline
//!   border and a soft elevation shadow (depth).
//!
//! The classes are opt-in utilities the feature surfaces (player bar, cards,
//! sidebar rows) attach as they are reskinned in later phases.

pub(super) fn css() -> String {
    use super::tokens::{
        DIALOG_BORDER_ALPHA, DIALOG_CARD_ALPHA, DIALOG_HEADER_TINT_ALPHA, DIALOG_SHADOW,
        FOCUS_GLOW_ALPHA, FOCUS_GLOW_BLUR, HOVER_BG_ALPHA, HOVER_BG_ALPHA_STRONG, RADIUS_SURFACE,
        SCRIM_ALPHA, SURFACE_BORDER_ALPHA, SURFACE_SHADOW, TRANSITION,
    };
    format!(
        "entry:focus-within, .reprise-focus-glow:focus-within {{ \
           box-shadow: 0 0 {FOCUS_GLOW_BLUR} alpha(@accent_color, {FOCUS_GLOW_ALPHA}); \
           transition: box-shadow {TRANSITION}; }}\n\
         .reprise-hover {{ transition: background-color {TRANSITION}; }}\n\
         .reprise-hover:hover {{ background-color: alpha(@accent_bg_color, {HOVER_BG_ALPHA}); }}\n\
         .reprise-surface {{ \
           border-radius: {RADIUS_SURFACE}; \
           border: 1px solid alpha(@window_fg_color, {SURFACE_BORDER_ALPHA}); \
           box-shadow: {SURFACE_SHADOW}; }}\n\
         .reprise-panel-toggle {{ \
           transition: color {TRANSITION}, background-color {TRANSITION}; }}\n\
         .reprise-panel-toggle:checked {{ \
           color: @accent_color; \
           background-color: alpha(@accent_bg_color, {HOVER_BG_ALPHA}); }}\n\
         .reprise-panel-toggle:checked:hover {{ \
           background-color: alpha(@accent_bg_color, {HOVER_BG_ALPHA_STRONG}); }}\n\
         floating-sheet > dimming {{ \
           background-color: rgba(0,0,0,{SCRIM_ALPHA}); }}\n\
         floating-sheet > sheet {{ \
           background-color: @dialog_bg_color; \
           box-shadow: {DIALOG_SHADOW}; \
           outline: 1px solid alpha(white, {DIALOG_BORDER_ALPHA}); \
           outline-offset: -1px; }}\n\
         floating-sheet > sheet headerbar {{ \
           background-color: alpha(white, {DIALOG_HEADER_TINT_ALPHA}); }}\n\
         floating-sheet > sheet .boxed-list {{ \
           background-color: alpha(white, {DIALOG_CARD_ALPHA}); }}"
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn css_defines_focus_glow_hover_and_surface() {
        let css = super::css();
        assert!(css.contains(":focus-within"));
        assert!(css.contains("@accent_color"));
        assert!(css.contains(".reprise-hover:hover"));
        assert!(css.contains(".reprise-surface"));
        assert!(css.contains(".reprise-panel-toggle:checked"));
        assert!(css.contains("border-radius"));
        assert!(css.contains("floating-sheet > dimming"));
        assert!(css.contains("floating-sheet > sheet"));
    }
}
