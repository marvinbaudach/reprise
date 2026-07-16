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
}
