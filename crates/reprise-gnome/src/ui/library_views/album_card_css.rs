//! CSS for the album grid cards — cover styling, hover overlay, play
//! button, now-playing EQ positioning, focus ring, and card text.

use crate::ui::style::tokens;

// CSS class constants — used by album_card.rs to apply classes.
pub(in crate::ui) const CARD_CLASS: &str = "album-card";
pub(in crate::ui) const COVER_CLASS: &str = "album-cover";
pub(in crate::ui) const COVER_CONTAINER_CLASS: &str = "album-cover-container";
pub(in crate::ui) const HOVER_OVERLAY_CLASS: &str = "album-hover-overlay";
pub(in crate::ui) const PLAY_BUTTON_CLASS: &str = "album-play-btn";
pub(in crate::ui) const EQ_CONTAINER_CLASS: &str = "album-eq-container";
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
         .{CARD_CLASS}:focus-visible {{ \
           outline: 2px solid @accent_color; \
           outline-offset: 4px; \
           border-radius: {radius}; }}\n\
         \
         /* Cover container: square, rounded, shadow, hairline. */
         .{COVER_CONTAINER_CLASS} {{ \
           border-radius: 10px; \
           box-shadow: 0 4px 14px rgba(0,0,0,0.30); \
           overflow: hidden; \
           border: 1px solid alpha(white, 0.06); }}\n\
         \
         /* Cover image fills container. */
         .{COVER_CLASS} {{ \
           border-radius: 10px; }}\n\
         \
         /* Hover overlay: gradient + play button, fades in. */
         .{HOVER_OVERLAY_CLASS} {{ \
           opacity: 0; \
           transition: opacity {transition}; \
           border-radius: 10px; }}\n\
         .{CARD_CLASS}:hover .{HOVER_OVERLAY_CLASS}, \
         .{CARD_CLASS}:focus-visible .{HOVER_OVERLAY_CLASS} {{ \
           opacity: 1; }}\n\
         \
         /* Play button: round, accent bg, centered icon. */
         .{PLAY_BUTTON_CLASS} {{ \
           min-width: 42px; min-height: 42px; \
           border-radius: 999px; \
           background-color: @accent_bg_color; \
           color: @accent_fg_color; \
           margin: 8px; \
           box-shadow: 0 2px 8px rgba(0,0,0,0.3); }}\n\
         .{PLAY_BUTTON_CLASS}:hover {{ \
           background-color: lighter(@accent_bg_color); }}\n\
         \
         /* EQ container: bottom-left, always visible when now-playing. */
         .{EQ_CONTAINER_CLASS} {{ \
           margin: 8px; \
           padding: 4px; \
           background-color: rgba(0,0,0,0.5); \
           border-radius: 6px; }}\n\
         \
         /* Text labels below cover. */
         .{TITLE_CLASS} {{ \
           font-weight: 700; font-size: 13.5px; \
           margin-top: 8px; }}\n\
         .{SUBTITLE_CLASS} {{ \
           font-size: 12px; \
           color: alpha(@window_fg_color, 0.50); }}\n\
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
        radius = tokens::RADIUS_SURFACE,
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
        assert!(css.contains(".album-cover"));
        assert!(css.contains(".album-hover-overlay"));
        assert!(css.contains(".album-play-btn"));
        assert!(css.contains(".album-eq-container"));
        assert!(css.contains(".album-card-title"));
        assert!(css.contains(".album-card-subtitle"));
        assert!(css.contains(".album-placeholder"));
        assert!(css.contains("outline: 2px solid @accent_color"));
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
    #[ignore = "requires a display; run via xvfb-run"]
    fn album_card_css_parses_without_errors() {
        gtk4::init().unwrap();
        let errors = crate::ui::style::css_parse_errors(&super::css());
        assert!(errors.is_empty(), "CSS parse errors: {errors:?}");
    }
}
