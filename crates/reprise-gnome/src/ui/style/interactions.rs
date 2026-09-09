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
         .reprise-hover:hover {{ background-color: @reprise_hover_bg; }}\n\
         .reprise-surface {{ \
           border-radius: {RADIUS_SURFACE}; \
           border: 1px solid alpha(@window_fg_color, {SURFACE_BORDER_ALPHA}); \
           box-shadow: {SURFACE_SHADOW}; }}\n\
         .reprise-build-badge {{ \
           font-size: 0.75em; \
           font-weight: bold; \
           letter-spacing: 0.08em; \
           padding: 2px 8px; \
           margin-right: 6px; \
           border-radius: 6px; \
           color: #ffffff; \
           background-color: #b5432f; }}\n\
         .reprise-panel-toggle {{ \
           transition: color {TRANSITION}, background-color {TRANSITION}; }}\n\
         .reprise-panel-toggle:checked {{ \
           color: @reprise_accent_text_color; \
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
    use super::super::{buttons, tokens};

    #[test]
    fn css_defines_focus_glow_hover_and_surface() {
        let css = super::css();
        assert!(css.contains(":focus-within"));
        assert!(css.contains("@accent_color"));
        let hover_rule = css
            .split(".reprise-hover:hover")
            .nth(1)
            .and_then(|rest| rest.split('}').next())
            .expect("the reprise-hover hover rule is present");
        assert!(
            hover_rule.contains("background-color: @reprise_hover_bg"),
            ".reprise-hover:hover must use the appearance-aware hover token: {hover_rule}"
        );
        assert!(css.contains(".reprise-surface"));
        let panel_toggle_rule = css
            .split(".reprise-panel-toggle:checked")
            .nth(1)
            .and_then(|rest| rest.split('}').next())
            .expect("the checked panel-toggle rule is present");
        assert!(
            panel_toggle_rule.contains(&format!(
                "background-color: alpha(@accent_bg_color, {})",
                super::super::tokens::HOVER_BG_ALPHA
            )),
            ".reprise-panel-toggle:checked must keep its accent state fill: {panel_toggle_rule}"
        );
        assert!(panel_toggle_rule.contains("color: @reprise_accent_text_color"));
        assert!(css.contains("border-radius"));
        assert!(css.contains("floating-sheet > dimming"));
        assert!(css.contains("floating-sheet > sheet"));
    }

    #[test]
    fn collapse_toggle_folded_state_keeps_its_accent_paint() {
        let css = buttons::css();
        let selector = format!(
            ".reprise-panel-toggle.{}",
            buttons::COLLAPSE_TOGGLE_CSS_CLASS
        );
        let rule = css_rule(&css, &selector);

        assert!(
            rule.contains("color: @reprise_accent_text_color"),
            "the folded collapse toggle must keep its accent foreground: {rule}"
        );
        assert!(
            rule.contains(&format!(
                "background-color: alpha(@accent_bg_color, {})",
                tokens::HOVER_BG_ALPHA
            )),
            "the folded collapse toggle must keep its accent background: {rule}"
        );
    }

    #[test]
    fn collapse_rules_never_capture_the_search_toggle() {
        let css = crate::ui::style::app_css_for_test();
        let scoped_selector = format!(
            ".reprise-panel-toggle.{}",
            buttons::COLLAPSE_TOGGLE_CSS_CLASS
        );
        let collapse_rule_count = css
            .split('{')
            .filter_map(|prefix| prefix.rsplit('}').next())
            .map(|selector| {
                selector
                    .rsplit("*/")
                    .next()
                    .expect("rsplit always yields one segment")
                    .trim()
            })
            .filter(|selector| selector.contains(buttons::COLLAPSE_TOGGLE_CSS_CLASS))
            .inspect(|selector_group| {
                for selector_arm in selector_group.split(',').map(str::trim) {
                    assert!(
                        selector_arm.contains(&scoped_selector),
                        "every collapse-rule selector arm must carry both scoping classes: \
                         {selector_arm}"
                    );
                }
            })
            .count();

        assert_eq!(collapse_rule_count, 6, "all six collapse states are scoped");
        let search_toggle_rule = css_rule(&css, ".reprise-panel-toggle:checked");
        let normalized_rule = search_toggle_rule
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            normalized_rule.contains("color: @reprise_accent_text_color;"),
            "the generic checked panel toggle must keep its accent foreground: \
             {search_toggle_rule}"
        );
    }

    fn css_rule<'a>(css: &'a str, selector: &str) -> &'a str {
        css.split('}')
            .filter_map(|rule| rule.split_once('{'))
            .find_map(|(selectors, body)| {
                selectors
                    .rsplit("*/")
                    .next()
                    .expect("rsplit always yields one segment")
                    .split(',')
                    .map(str::trim)
                    .any(|arm| arm == selector)
                    .then_some(body)
            })
            .unwrap_or_else(|| panic!("CSS rule is present for {selector}"))
    }
}
