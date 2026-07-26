//! Radio source structural styles.

pub(super) fn css() -> String {
    ".reprise-radio-source, .reprise-radio-view { min-width: 0; }\n\
     .reprise-radio-table { border-spacing: 0; }\n\
     .reprise-radio-playing { color: @accent_color; \
       background-color: alpha(@accent_bg_color, 0.07); }\n\
     .reprise-radio-view .card { border-radius: 8px; padding: 12px; }\n\
     .reprise-radio-view .reprise-btn-add { border-radius: 8px; }"
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
}
