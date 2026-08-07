//! Sidebar scan-progress card styling.
//!
//! The chip colours are derived, never mixed by hand: the running state reads
//! the Adwaita accent, the warning state the Adwaita warning colour, each with
//! the same surface/border/text weighting (CONTRAST-1 — a matching named
//! colour outranks a custom alpha, and the chip follows a changed accent).
//! The edge line is the other half of that indicator and derives the same way,
//! so both move together and the track stays visible in light mode too.

/// Chip surface alpha over its named background colour.
const CHIP_SURFACE_ALPHA: &str = "0.13";

/// Chip hairline alpha over its named foreground colour.
const CHIP_BORDER_ALPHA: &str = "0.32";

/// Edge-line track alpha over the named window foreground colour.
const EDGE_TRACK_ALPHA: &str = "0.10";

pub(in crate::ui) fn css() -> String {
    let spin_ms = crate::ui::motion::INDICATOR_SPIN_MS;
    format!(
        "\
    .scan-card {{\
        background: alpha(white, 0.05);\
        border: 1px solid alpha(white, 0.05);\
        border-radius: 10px;\
        padding: 10px;\
        margin: 8px 4px 0 4px;\
    }}\
    .scan-card-title {{\
        font-size: 12px;\
        font-weight: bold;\
    }}\
    .scan-card-percent {{\
        font-size: 12px;\
        font-weight: bold;\
        font-feature-settings: 'tnum';\
    }}\
    .scan-card-detail {{\
        font-size: 10.5px;\
        opacity: 0.45;\
    }}\
    .scan-card progressbar trough {{\
        min-height: 3px;\
        border-radius: 1.5px;\
    }}\
    .scan-card progressbar trough progress {{\
        border-radius: 1.5px;\
    }}\
    .scan-card-spinner {{\
        min-width: 13px;\
        min-height: 13px;\
    }}\
    @keyframes scan-chip-gear-spin {{\
        from {{ transform: rotate(0deg); }}\
        to {{ transform: rotate(360deg); }}\
    }}\
    .scan-chip {{\
        background: alpha(@accent_bg_color, {CHIP_SURFACE_ALPHA});\
        border: 1px solid alpha(@accent_color, {CHIP_BORDER_ALPHA});\
        border-radius: 999px;\
        color: @accent_color;\
    }}\
    .scan-chip.warning {{\
        background: alpha(@warning_bg_color, {CHIP_SURFACE_ALPHA});\
        border-color: alpha(@warning_color, {CHIP_BORDER_ALPHA});\
        color: @warning_color;\
    }}\
    .scan-chip-action {{\
        min-height: 24px;\
        padding: 2px 30px 2px 9px;\
        background: transparent;\
        box-shadow: none;\
        border: none;\
        border-radius: 999px;\
    }}\
    .scan-chip-label {{\
        font-size: 11.5px;\
        font-weight: 600;\
    }}\
    .scan-chip-gear {{\
        color: @accent_color;\
    }}\
    .scan-chip-gear.scan-chip-gear-spinning {{\
        animation: scan-chip-gear-spin {spin_ms}ms linear infinite;\
    }}\
    .scan-chip-cancel {{\
        min-width: 20px;\
        min-height: 20px;\
        padding: 0;\
        margin-right: 3px;\
        border-radius: 999px;\
    }}\
    .scan-edge-line {{\
        margin: 0;\
        padding: 0;\
    }}\
    .scan-edge-line trough {{\
        min-height: 2px;\
        background: alpha(@window_fg_color, {EDGE_TRACK_ALPHA});\
        border: none;\
        border-radius: 0;\
    }}\
    .scan-edge-line trough progress {{\
        min-height: 2px;\
        background: @accent_bg_color;\
        border-radius: 0;\
    }}\
    "
    )
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
