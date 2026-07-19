//! CSS for the album grid cards — cover styling, hover overlay, play
//! button, now-playing EQ positioning, focus ring, and card text.

use crate::ui::style::tokens;

// CSS class constants — used by album_card.rs to apply classes.
pub(in crate::ui) const CARD_CLASS: &str = "album-card";
pub(in crate::ui) const COVER_CLASS: &str = "album-cover";
pub(in crate::ui) const COVER_CONTAINER_CLASS: &str = "album-cover-container";
pub(in crate::ui) const FOCUS_FRAME_CLASS: &str = "album-focus-frame";
pub(in crate::ui) const REVEAL_FRAME_CLASS: &str = "album-reveal-frame";
pub(in crate::ui) const REVEAL_PULSE_CLASS: &str = "album-reveal-pulse";
pub(in crate::ui) const REVEAL_PULSE_STATIC_CLASS: &str = "album-reveal-pulse-static";
pub(in crate::ui) const REVEAL_DURATION_MS: u64 = 1_000;
pub(in crate::ui) const PLAYING_FRAME_CLASS: &str = "album-playing-frame";
pub(in crate::ui) const PLAYING_LAYER_CLASS: &str = "album-playing-layer";
pub(in crate::ui) const HOVER_OVERLAY_CLASS: &str = "album-bottom-gradient";
pub(in crate::ui) const META_CLASS: &str = "album-card-meta";
pub(in crate::ui) const PLAY_BUTTON_CLASS: &str = "album-play-btn";
pub(in crate::ui) const TITLE_CLASS: &str = "album-card-title";
pub(in crate::ui) const SUBTITLE_CLASS: &str = "album-card-subtitle";
pub(in crate::ui) const PLACEHOLDER_CLASS: &str = "album-placeholder";
pub(in crate::ui) const PLACEHOLDER_INITIAL_CLASS: &str = "album-placeholder-initial";
pub(in crate::ui) const PLACEHOLDER_GRADIENT_COUNT: usize = 12;

const PLACEHOLDER_GRADIENT_HUES: [(u16, u16); PLACEHOLDER_GRADIENT_COUNT] = [
    (18, 54),
    (42, 88),
    (72, 128),
    (116, 166),
    (154, 204),
    (190, 238),
    (222, 276),
    (258, 314),
    (296, 344),
    (328, 22),
    (352, 64),
    (206, 28),
];

pub(in crate::ui) fn css() -> String {
    let mut output = format!(
        // Card container: transparent bg, no frame, vertical layout.
        ".{CARD_CLASS} {{ \
           padding: 0; margin: 0; \
           background: transparent; border: none; outline: none; }}\n\
         /* Cover container: square, rounded, shadow, hairline. */
         .{COVER_CONTAINER_CLASS} {{ \
           border-radius: 10px; \
           box-shadow: 0 4px 14px rgba(0,0,0,0.30); \
           border: 1px solid alpha(white, 0.06); }}\n\
         .{PLAYING_FRAME_CLASS} {{ \
           border-radius: 10px; \
           box-shadow: inset 0 0 0 1.5px @reprise_player_accent; }}\n\
         .{FOCUS_FRAME_CLASS} {{ \
           opacity: 0; border-radius: 10px; \
           box-shadow: 0 0 0 2px @accent_color; }}\n\
         .library-grid child:focus-visible .{FOCUS_FRAME_CLASS} {{ \
           opacity: 1; }}\n\
         .{REVEAL_FRAME_CLASS}.{REVEAL_PULSE_CLASS} {{ \
           animation: album-reveal-highlight 500ms ease-in-out 2; }}\n\
         .{REVEAL_FRAME_CLASS}.{REVEAL_PULSE_STATIC_CLASS} {{ \
           border-radius: 10px; \
           box-shadow: 0 0 0 3px alpha(@accent_color, 0.55), \
                       0 0 18px alpha(@accent_color, 0.38); }}\n\
         @keyframes album-reveal-highlight {{ \
           0%, 100% {{ box-shadow: 0 0 0 0 alpha(@accent_color, 0.0); }} \
           50% {{ box-shadow: 0 0 0 3px alpha(@accent_color, 0.65), \
                 0 0 20px alpha(@accent_color, 0.45); }} }}\n\
         .{PLAYING_LAYER_CLASS} {{ \
           margin: 8px; padding: 4px; \
           color: @reprise_player_accent; \
           background-color: rgba(0,0,0,0.5); \
           border-radius: 6px; }}\n\
         \
         /* Cover image fills container. */
         .{COVER_CLASS} {{ \
           border-radius: 10px; }}\n\
         \
         /* Bottom interaction gradient: metadata + play button, fades in. */
         .{HOVER_OVERLAY_CLASS} {{ \
           opacity: 0; \
           transition: opacity {transition}; \
           padding: 40px 8px 8px; \
           border-radius: 10px; \
           background: linear-gradient(to bottom, \
             rgba(0,0,0,0.0) 0%, rgba(0,0,0,0.28) 38%, \
             rgba(0,0,0,0.82) 100%); }}\n\
         .{CARD_CLASS}:hover .{HOVER_OVERLAY_CLASS}, \
         .library-grid child:focus-visible .{HOVER_OVERLAY_CLASS} {{ \
           opacity: 1; }}\n\
         \
         /* Play button: round, accent bg, centered icon. */
         .{PLAY_BUTTON_CLASS} {{ \
           min-width: 42px; min-height: 42px; \
           border-radius: 999px; \
           background-color: @reprise_player_accent; \
           color: white; margin: 0; \
           transition: box-shadow {transition}, background-color {transition}; \
           box-shadow: 0 0 12px alpha(@reprise_player_accent, 0.40); }}\n\
         .{PLAY_BUTTON_CLASS}:hover {{ \
           background-color: lighter(@reprise_player_accent); \
           box-shadow: 0 0 18px alpha(@reprise_player_accent, 0.60); }}\n\
         .{META_CLASS} {{ \
           color: alpha(white, 0.78); font-size: 10.5px; }}\n\
         \
         /* Text labels below cover. */
         .{TITLE_CLASS} {{ \
           font-weight: 700; font-size: 13.5px; \
           margin-top: 8px; }}\n\
         .{SUBTITLE_CLASS} {{ \
           font-size: 12px; \
           color: @reprise_secondary_fg_color; }}\n\
         .{SUBTITLE_CLASS}:hover {{ \
           text-decoration: underline; \
           text-decoration-color: alpha(@window_fg_color, 0.35); }}\n\
         \
         /* Placeholder: gradient bg + centered initial. */
         .{PLACEHOLDER_CLASS} {{ \
           border-radius: 10px; }}\n\
         .{PLACEHOLDER_INITIAL_CLASS} {{ \
           font-size: 48px; font-weight: 700; \
           color: alpha(white, 0.85); }}",
        transition = tokens::TRANSITION,
    );
    for (index, (start_hue, end_hue)) in PLACEHOLDER_GRADIENT_HUES.iter().enumerate() {
        output.push_str(&format!(
            ".album-placeholder-gradient-{index} {{ \
             background: linear-gradient(135deg, \
             oklch(0.45 0.08 {start_hue}), oklch(0.18 0.05 {end_hue})); }}\n"
        ));
    }
    output
}

#[cfg(test)]
mod tests {
    #[test]
    fn css_defines_all_card_classes() {
        let css = super::css();
        assert!(css.contains(".album-card"));
        assert!(css.contains(".album-cover-container"));
        assert!(css.contains(".album-focus-frame"));
        assert!(css.contains(".album-reveal-frame"));
        assert!(css.contains(".album-playing-frame"));
        assert!(css.contains(".album-playing-layer"));
        assert!(css.contains(".album-cover"));
        assert!(css.contains(".album-bottom-gradient"));
        assert!(css.contains(".album-card-meta"));
        assert!(css.contains(".album-play-btn"));
        assert!(css.contains(".album-card-title"));
        assert!(css.contains(".album-card-subtitle"));
        assert!(css.contains(".album-placeholder"));
        assert!(css.contains("box-shadow: 0 0 0 2px @accent_color"));
        assert!(css.contains("inset 0 0 0 1.5px @reprise_player_accent"));
        assert!(css.contains("animation: album-reveal-highlight 500ms ease-in-out 2"));
        assert_eq!(super::REVEAL_DURATION_MS, 1_000);
        assert!(css.contains("box-shadow: 0 4px 14px"));
        for index in 0..super::PLACEHOLDER_GRADIENT_COUNT {
            assert!(css.contains(&format!(".album-placeholder-gradient-{index}")));
        }
    }

    #[test]
    fn css_contains_no_invalid_line_comments() {
        let css = super::css();
        assert!(
            css.lines().all(|line| !line.trim_start().starts_with("//")),
            "CSS must use block comments because // discards the next rule"
        );
    }

    #[test]
    fn focus_and_playing_layers_have_independent_cover_only_selectors() {
        let css = super::css();
        assert!(css.contains(".library-grid child:focus-visible .album-focus-frame"));
        assert!(css.contains(".library-grid child:focus-visible .album-bottom-gradient"));
        assert!(css.contains("box-shadow: 0 0 0 2px @accent_color"));
        assert!(css.contains("inset 0 0 0 1.5px @reprise_player_accent"));
        assert!(!css.contains(".album-card:focus-visible"));
    }

    #[test]
    fn grid_4_bottom_gradient_css_contract() {
        let css = super::css();
        assert!(css.contains(".album-bottom-gradient"));
        assert!(css.contains("linear-gradient(to bottom"));
        assert!(css.contains("@reprise_player_accent"));
        assert!(css.contains("transition: opacity 150ms ease-out"));
        assert!(!css.contains("@accent_bg_color"));
        assert!(!css.contains("album-eq-container"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn album_card_css_parses_without_errors() {
        gtk4::init().unwrap();
        let errors = crate::ui::style::css_parse_errors(&super::css());
        assert!(errors.is_empty(), "CSS parse errors: {errors:?}");
    }
}
