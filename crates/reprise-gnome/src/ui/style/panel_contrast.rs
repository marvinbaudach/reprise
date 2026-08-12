use super::tokens::{PRIMARY_TEXT_ALPHA, SECONDARY_TEXT_ALPHA};

type CssFn = fn() -> String;

struct PanelRole {
    css: CssFn,
    selector: &'static str,
    role: &'static str,
    alpha: f64,
    minimum: Option<f64>,
}

const PANEL_ROLES: [PanelRole; 10] = [
    PanelRole {
        css: crate::ui::now_playing::css,
        selector: ".reprise-now-playing-stage",
        role: "@sidebar_fg_color",
        alpha: 1.0,
        minimum: Some(4.5),
    },
    PanelRole {
        css: crate::ui::now_playing::css,
        selector: ".reprise-now-playing-title",
        role: "@reprise_primary_fg_color",
        alpha: PRIMARY_TEXT_ALPHA,
        minimum: Some(4.5),
    },
    PanelRole {
        css: crate::ui::now_playing::css,
        selector: ".reprise-now-playing-subtitle",
        role: "@reprise_secondary_fg_color",
        alpha: SECONDARY_TEXT_ALPHA,
        minimum: Some(4.5),
    },
    PanelRole {
        css: crate::ui::now_playing::css,
        selector: ".reprise-now-playing-footer",
        role: "@reprise_secondary_fg_color",
        alpha: SECONDARY_TEXT_ALPHA,
        minimum: Some(4.5),
    },
    PanelRole {
        css: crate::ui::now_playing::up_next_panel::css,
        selector: ".reprise-up-next-section",
        role: "@reprise_secondary_fg_color",
        alpha: SECONDARY_TEXT_ALPHA,
        minimum: Some(4.5),
    },
    PanelRole {
        css: crate::ui::now_playing::up_next_panel::css,
        selector: ".reprise-up-next-title",
        role: "@reprise_primary_fg_color",
        alpha: PRIMARY_TEXT_ALPHA,
        minimum: Some(4.5),
    },
    PanelRole {
        css: crate::ui::now_playing::up_next_panel::css,
        selector: ".reprise-up-next-artist",
        role: "@reprise_secondary_fg_color",
        alpha: SECONDARY_TEXT_ALPHA,
        minimum: Some(4.5),
    },
    PanelRole {
        css: crate::ui::now_playing::up_next_panel::css,
        selector: ".reprise-up-next-empty",
        role: "@reprise_secondary_fg_color",
        alpha: SECONDARY_TEXT_ALPHA,
        minimum: Some(4.5),
    },
    PanelRole {
        css: crate::ui::lyrics_view::css,
        selector: ".lyrics-line {",
        role: "@sidebar_fg_color",
        alpha: 1.0,
        minimum: Some(4.5),
    },
    PanelRole {
        css: crate::ui::lyrics_view::css,
        selector: ".lyrics-unsynced {",
        role: "@reprise_secondary_fg_color",
        alpha: SECONDARY_TEXT_ALPHA,
        minimum: Some(4.5),
    },
];

fn fixed_foregrounds(css: &str) -> Vec<String> {
    css.split(['{', '}', ';'])
        .filter_map(|declaration| {
            let (property, value) = declaration.split_once(':')?;
            let property = property.trim().rsplit([' ', '\n', '\t']).next()?;
            let value = value.to_ascii_lowercase();
            let fixed = value.contains("#ffffff")
                || value.contains("#fff")
                || value.contains("white")
                || value.contains("255, 255, 255");
            (property == "color" && fixed).then(|| declaration.trim().to_owned())
        })
        .collect()
}

#[test]
fn npp_17_the_panel_takes_its_foreground_from_the_appearance() {
    for offender in [
        ".x { color: #ffffff; }",
        ".x { color: #fff; }",
        ".x { color:alpha(#ffffff, 0.5); }",
        ".x { color: white; }",
    ] {
        assert_eq!(
            fixed_foregrounds(offender).len(),
            1,
            "guard missed {offender}"
        );
    }

    for allowed in [
        ".x { background-color: alpha(#ffffff, 0.06); }",
        ".x { border-left: 1px solid rgba(255, 255, 255, 0.06); }",
    ] {
        assert!(
            fixed_foregrounds(allowed).is_empty(),
            "guard false-flagged {allowed}"
        );
    }

    for css in [
        crate::ui::now_playing::css(),
        crate::ui::lyrics_view::css(),
        crate::ui::now_playing::up_next_panel::css(),
    ] {
        let fixed = fixed_foregrounds(&css);
        assert!(
            fixed.is_empty(),
            "Now Playing CSS paints text with a fixed foreground: {fixed:#?}"
        );
    }

    for row in &PANEL_ROLES {
        let css = (row.css)();
        let rules = css
            .split(row.selector)
            .nth(1)
            .and_then(|rest| rest.split('}').next())
            .unwrap_or_else(|| panic!("no rules for {}", row.selector));
        assert!(
            rules.contains(row.role),
            "{} does not consume {}: {rules}",
            row.selector,
            row.role
        );
        assert!(
            !rules.contains("opacity:"),
            "{} locally dims its verified role: {rules}",
            row.selector
        );
    }
}

#[test]
fn contrast_3_now_playing_roles_clear_aa_on_the_panel_surface() {
    use super::color_math::{composite, contrast_ratio, parse_hex_rgb};
    use super::theme::Theme;

    for theme in Theme::all() {
        for (appearance, palette) in [("dark", theme.palette()), ("light", theme.light_palette())] {
            let foreground = parse_hex_rgb(palette.fg).expect("palette fg is valid hex");
            let surface = parse_hex_rgb(palette.sidebar_bg).expect("palette sidebar is valid hex");

            for row in &PANEL_ROLES {
                let Some(minimum) = row.minimum else {
                    continue;
                };
                let rendered = composite(foreground, surface, row.alpha);
                let ratio = contrast_ratio(rendered, surface);
                assert!(
                    ratio >= minimum,
                    "{theme:?} {appearance}: {} reaches only {ratio:.2}:1 (minimum \
                     {minimum:.1}:1)",
                    row.selector
                );
            }
        }
    }
}
