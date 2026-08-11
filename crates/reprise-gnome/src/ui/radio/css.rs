//! Radio source structural styles.

pub(super) fn css() -> String {
    ".reprise-radio-source, .reprise-radio-view { min-width: 0; }\n\
     .reprise-radio-table { border-spacing: 0; }\n\
     .reprise-radio-playing { background-color: alpha(@accent_bg_color, 0.07); }\n\
     .reprise-radio-view .card { border-radius: 8px; padding: 12px; }\n\
     .reprise-radio-view .reprise-btn-add { border-radius: 8px; }\n\
     .reprise-radio-initials-tile { font-size: 16px; font-weight: 700; \
       color: @reprise_accent_text_color; border-radius: 8px; \
       background-image: linear-gradient(155deg, alpha(@accent_bg_color, 0.22), \
         alpha(@window_fg_color, 0.05)); }\n\
     .reprise-radio-chips { margin-bottom: 4px; }"
        .to_owned()
}

#[cfg(test)]
mod tests {
    #[test]
    fn radio_css_tints_only_the_connected_row_and_keeps_add_rectangular() {
        let css = super::css();
        assert!(css.contains("alpha(@accent_bg_color, 0.07)"));
        assert!(css.contains(".reprise-btn-add { border-radius: 8px; }"));
        assert!(!css.contains("border-radius: 9999px"));
    }

    #[test]
    fn radio_initials_tile_uses_the_shared_accent_gradient_vocabulary() {
        let css = super::css();
        let tile = css
            .split_once(".reprise-radio-initials-tile")
            .and_then(|(_, rest)| rest.split_once('}'))
            .map(|(block, _)| block)
            .expect("radio initials CSS block");

        assert!(tile.contains("font-size: 16px"));
        assert!(tile.contains("font-weight: 700"));
        assert!(tile.contains("color: @reprise_accent_text_color"));
        assert!(tile.contains("linear-gradient(155deg"));
        assert!(tile.contains("alpha(@accent_bg_color, 0.22)"));
        assert!(tile.contains("alpha(@window_fg_color, 0.05)"));
        assert!(tile.contains("border-radius: 8px"));
    }
}
