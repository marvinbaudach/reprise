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
    /* The cancel control is a text link, not a chunky button: in a 240px \
       sidebar a default-padded button starves the title of its allocation \
       until it truncates to three characters. */\
    .scan-card-cancel {{\
        font-size: 11px;\
        min-height: 0;\
        min-width: 0;\
        padding: 0 2px;\
        color: @reprise_accent_text_color;\
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
        color: @reprise_accent_text_color;\
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
        color: @reprise_accent_text_color;\
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
    use gtk4::prelude::*;

    /// The named colours the chip and the edge line are painted from. Kept as
    /// an explicit list so that both a typo *and* a silent removal are caught:
    /// the source test below holds it against what the stylesheet actually
    /// references, the display test resolves every entry for real.
    const NAMED_COLOURS: [&str; 6] = [
        "accent_bg_color",
        "accent_color",
        "reprise_accent_text_color",
        "warning_bg_color",
        "warning_color",
        "window_fg_color",
    ];

    /// The factor the runtime probe multiplies each named colour by. GTK
    /// reports no parse error for an unresolvable `@name`; it drops the whole
    /// declaration and the property falls back to its initial value, which for
    /// `color` is opaque white. So a resolved name is exactly the one that
    /// comes back at this alpha instead of fully opaque — and that holds
    /// whatever RGB the theme gives the colour.
    const PROBE_ALPHA: f64 = 0.5;

    /// CSS at-rules share the `@` sigil with named colours but are not colour
    /// references. GTK4 understands only this handful.
    const AT_RULES: [&str; 4] = ["define-color", "import", "keyframes", "media"];

    /// Every `@name` colour reference in `css`, sorted and deduplicated.
    fn referenced_named_colours(css: &str) -> Vec<String> {
        let mut names: Vec<String> = css
            .split('@')
            .skip(1)
            .map(|tail| {
                tail.chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                    .collect::<String>()
            })
            .filter(|name| !name.is_empty() && !AT_RULES.contains(&name.as_str()))
            .collect();
        names.sort();
        names.dedup();
        names
    }

    #[test]
    fn css_has_scan_card_class() {
        let css = super::css();
        assert!(css.contains(".scan-card"));
        assert!(css.contains("border-radius: 10px"));
    }

    #[test]
    fn fb_9_scan_chrome_paints_from_the_declared_named_colours() {
        assert_eq!(
            referenced_named_colours(&super::css()),
            NAMED_COLOURS,
            "the chip and edge line must keep painting from the named colours \
             the runtime probe below resolves — add or remove entries here in \
             the same commit as the stylesheet change"
        );
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

    /// Parsing cleanly is not the same as resolving: GTK accepts `@anything`
    /// without a murmur and only drops the declaration when it computes the
    /// style, so a misspelt Adwaita colour name paints an invisible chip and
    /// an invisible edge line while every parser test stays green.
    ///
    /// This resolves each name the way the running app does — libadwaita's
    /// stylesheet for the warning colours, the app's own palette provider for
    /// the accent and window colours — and reads the result back off a real
    /// widget.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fb_9_scan_chrome_named_colours_resolve_against_the_live_stylesheet() {
        libadwaita::init().expect("libadwaita must initialise under a display");
        crate::ui::style::install();

        let names = referenced_named_colours(&super::css());
        assert_eq!(
            names, NAMED_COLOURS,
            "the probe must cover every named colour the stylesheet references"
        );

        let probe_css: String = names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                format!(".scan-colour-probe-{index} {{ color: alpha(@{name}, {PROBE_ALPHA}); }}")
            })
            .collect();
        assert!(
            crate::ui::style::css_parse_errors(&probe_css).is_empty(),
            "the probe stylesheet itself must be well-formed"
        );
        crate::ui::style::install_css_string_for_test(&probe_css);

        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let probes: Vec<gtk4::Label> = names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let label = gtk4::Label::new(Some(name));
                label.add_css_class(&format!("scan-colour-probe-{index}"));
                root.append(&label);
                label
            })
            .collect();
        let window = gtk4::Window::builder().child(&root).build();
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        for (name, probe) in names.iter().zip(&probes) {
            let alpha = f64::from(probe.color().alpha());
            assert!(
                (alpha - PROBE_ALPHA).abs() < 0.01,
                "@{name} does not resolve: GTK dropped the declaration and fell back \
                 to the initial opaque colour (alpha {alpha} instead of {PROBE_ALPHA}). \
                 That is what a misspelt Adwaita colour name looks like — no parse \
                 error, just an indicator nobody can see"
            );
        }

        window.close();
    }
}
