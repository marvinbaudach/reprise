//! Structural styling owned by the Concerts full view.

pub(in crate::ui) fn css() -> String {
    ".reprise-concerts-view { min-height: 1px; }\n\
     .reprise-concerts-radius-off {\n  \
       opacity: 0.62;\n  \
       border: 1px dashed alpha(currentColor, 0.52);\n\
     }\n\
     .reprise-concerts-location-banner {\n  \
       background-color: alpha(@accent_bg_color, 0.12);\n\
     }\n\
     .reprise-concert-ticket-tag {\
       border-radius: 999px;\
       padding: 2px 8px;\
       font-size: 11px;\
     }\n\
     .reprise-concert-ticket-tag.on-sale {\
       border: 1px solid alpha(@accent_bg_color, 0.45);\
       color: @reprise_accent_text_color;\
       background-color: alpha(@accent_bg_color, 0.08);\
     }\n\
     .reprise-concert-ticket-tag.off-sale {\
       border: 1px solid alpha(@window_fg_color, 0.20);\
       color: @reprise_secondary_fg_color;\
       background-color: alpha(@window_fg_color, 0.08);\
     }\n\
     .reprise-concert-ticket-tag.unknown {\
       border: 1px solid alpha(@window_fg_color, 0.12);\
       color: @reprise_hint_fg_color;\
       background-color: transparent;\
     }\n\
     .reprise-concert-distance-near { color: @reprise_accent_text_color; }\n\
     .reprise-concert-distance-far,\
     .reprise-concert-city { color: @reprise_secondary_fg_color; }\n\
     .reprise-concert-venue { color: @window_fg_color; }\n\
     .reprise-concerts-table row {\
       border-left: 2px solid transparent;\
     }\n\
     .reprise-concerts-table row:hover,\
     .reprise-concerts-table row:focus-within {\
       border-left-color: @accent_bg_color;\
       background-color: alpha(currentColor, 0.06);\
     }"
    .into()
}

#[cfg(test)]
mod tests {
    #[test]
    fn concert_rows_reserve_the_accent_border_before_hover() {
        let css = super::css();
        assert!(css.contains("border-left: 2px solid transparent"));
        assert!(css.contains("border-left-color: @accent_bg_color"));
        assert!(!css.contains("box-shadow"));
    }
}
