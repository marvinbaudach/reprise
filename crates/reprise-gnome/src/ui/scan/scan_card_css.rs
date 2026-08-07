//! Sidebar scan-progress card styling.

pub(in crate::ui) fn css() -> String {
    "\
    .scan-card {\
        background: alpha(white, 0.05);\
        border: 1px solid alpha(white, 0.05);\
        border-radius: 10px;\
        padding: 10px;\
        margin: 8px 4px 0 4px;\
    }\
    .scan-card-title {\
        font-size: 12px;\
        font-weight: bold;\
    }\
    .scan-card-percent {\
        font-size: 12px;\
        font-weight: bold;\
        font-feature-settings: 'tnum';\
    }\
    .scan-card-detail {\
        font-size: 10.5px;\
        opacity: 0.45;\
    }\
    .scan-card progressbar trough {\
        min-height: 3px;\
        border-radius: 1.5px;\
    }\
    .scan-card progressbar trough progress {\
        border-radius: 1.5px;\
    }\
    .scan-card-spinner {\
        min-width: 13px;\
        min-height: 13px;\
    }\
    @keyframes scan-chip-gear-spin {\
        from { transform: rotate(0deg); }\
        to { transform: rotate(360deg); }\
    }\
    .scan-chip {\
        background: rgba(46, 194, 126, 0.13);\
        border: 1px solid rgba(46, 194, 126, 0.32);\
        border-radius: 999px;\
        color: #a9e6c8;\
    }\
    .scan-chip.warning {\
        background: rgba(255, 190, 80, 0.13);\
        border-color: rgba(255, 190, 80, 0.32);\
        color: #ffd79a;\
    }\
    .scan-chip-action {\
        min-height: 24px;\
        padding: 2px 30px 2px 9px;\
        background: transparent;\
        box-shadow: none;\
        border: none;\
        border-radius: 999px;\
    }\
    .scan-chip-label {\
        font-size: 11.5px;\
        font-weight: 600;\
    }\
    .scan-chip-gear {\
        color: @accent_color;\
    }\
    .scan-chip-gear.scan-chip-gear-spinning {\
        animation: scan-chip-gear-spin 1200ms linear infinite;\
    }\
    .scan-chip-cancel {\
        min-width: 20px;\
        min-height: 20px;\
        padding: 0;\
        margin-right: 3px;\
        border-radius: 999px;\
    }\
    .scan-edge-line {\
        margin: 0;\
        padding: 0;\
    }\
    .scan-edge-line trough {\
        min-height: 2px;\
        background: rgba(255, 255, 255, 0.10);\
        border: none;\
        border-radius: 0;\
    }\
    .scan-edge-line trough progress {\
        min-height: 2px;\
        background: #2ec27e;\
        border-radius: 0;\
    }\
    "
    .to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn css_has_scan_card_class() {
        let css = super::css();
        assert!(css.contains(".scan-card"));
        assert!(css.contains("border-radius: 10px"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fb_9_scan_chrome_css_parses_without_gtk_errors() {
        gtk4::init().unwrap();
        let errors = crate::ui::style::css_parse_errors(&super::css());
        assert!(
            errors.is_empty(),
            "GTK reported scan chrome CSS parsing errors: {errors:?}"
        );
    }
}
